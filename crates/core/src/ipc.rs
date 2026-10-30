use crate::{
    engine::Engine,
    model::{Request, MAX_FRAME},
    store::{data_dir, Store},
};
use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Stdio,
    sync::Arc,
};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};

fn secret(dir: &Path) -> Result<String> {
    Ok(std::fs::read_to_string(dir.join("ipc.secret"))?)
}
fn ensure_secret(dir: &Path) -> Result<()> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    match options.open(dir.join("ipc.secret")) {
        Ok(mut file) => file.write_all(uuid::Uuid::new_v4().to_string().as_bytes())?,
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e.into()),
    }
    Ok(())
}
#[cfg(windows)]
fn pipe_name() -> String {
    use sha2::{Digest, Sha256};
    let hash = Sha256::digest(data_dir().to_string_lossy().as_bytes());
    format!(r"\\.\pipe\mediaharbor-{:x}", hash)[..60].to_owned()
}

async fn connection<S>(stream: S, engine: Arc<Engine>, token: String) -> Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let (read, mut write) = tokio::io::split(stream);
    let mut read = BufReader::new(read).take((MAX_FRAME + 1) as u64);
    use tokio::io::AsyncReadExt;
    let mut frame = String::new();
    tokio::time::timeout(
        std::time::Duration::from_secs(10),
        read.read_line(&mut frame),
    )
    .await??;
    anyhow::ensure!(frame.len() <= MAX_FRAME, "Слишком большой запрос");
    let envelope: Value = serde_json::from_str(&frame)?;
    anyhow::ensure!(
        envelope["token"].as_str() == Some(&token),
        "Доступ запрещён"
    );
    let req: Request = serde_json::from_value(envelope["request"].clone())?;
    let response = engine.response(req).await;
    let mut bytes = serde_json::to_vec(&response)?;
    if bytes.len() > MAX_FRAME {
        bytes = serde_json::to_vec(
            &json!({"version":1,"requestId":response["requestId"],"ok":false,"error":{"code":"FRAME_TOO_LARGE","message":"Ответ слишком большой; запросите меньшую страницу"}}),
        )?;
    }
    bytes.push(b'\n');
    write.write_all(&bytes).await?;
    write.shutdown().await?;
    Ok(())
}

pub async fn worker() -> Result<()> {
    let dir = data_dir();
    std::fs::create_dir_all(&dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))?;
    }
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(dir.join("worker.lock"))?;
    if lock.try_lock().is_err() {
        return Ok(());
    }
    ensure_secret(&dir)?;
    let token = secret(&dir)?;
    let engine = Engine::new(Store::open(dir.clone())?);
    let engine_signal = engine.clone();
    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        engine_signal.shutdown.cancel();
    });
    #[cfg(unix)]
    {
        let socket = dir.join("worker.sock");
        if socket.exists() {
            std::fs::remove_file(&socket)?;
        }
        let listener = tokio::net::UnixListener::bind(&socket)?;
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600))?;
        loop {
            tokio::select! {
                _ = engine.shutdown.cancelled() => break,
                result = listener.accept() => { let (stream, _) = result?; let engine = engine.clone(); let token = token.clone(); tokio::spawn(async move { let _ = connection(stream, engine, token).await; }); }
            }
        }
        engine.stop().await;
        let _ = std::fs::remove_file(socket);
    }
    #[cfg(windows)]
    {
        use tokio::net::windows::named_pipe::ServerOptions;
        let name = pipe_name();
        let mut server = ServerOptions::new()
            .first_pipe_instance(true)
            .reject_remote_clients(true)
            .create(&name)?;
        loop {
            tokio::select! {
                _ = engine.shutdown.cancelled() => break,
                result = server.connect() => {
                    result?;
                    let next = ServerOptions::new().reject_remote_clients(true).create(&name)?;
                    let connected = std::mem::replace(&mut server, next);
                    let engine = engine.clone(); let token = token.clone();
                    tokio::spawn(async move { let _ = connection(connected, engine, token).await; });
                }
            }
        }
        engine.stop().await;
    }
    drop(lock);
    Ok(())
}

pub async fn send(req: Request) -> Result<Value> {
    let dir = data_dir();
    #[cfg(unix)]
    let stream = tokio::net::UnixStream::connect(dir.join("worker.sock")).await?;
    #[cfg(windows)]
    let stream = tokio::net::windows::named_pipe::ClientOptions::new().open(pipe_name())?;
    let (read, mut write) = tokio::io::split(stream);
    let mut payload = serde_json::to_vec(&json!({"token":secret(&dir)?,"request":req}))?;
    anyhow::ensure!(payload.len() <= MAX_FRAME, "Слишком большой запрос");
    payload.push(b'\n');
    write.write_all(&payload).await?;
    use tokio::io::AsyncReadExt;
    let mut reader = BufReader::new(read).take((MAX_FRAME + 1) as u64);
    let mut line = String::new();
    tokio::time::timeout(
        std::time::Duration::from_secs(180),
        reader.read_line(&mut line),
    )
    .await??;
    anyhow::ensure!(line.len() <= MAX_FRAME, "Слишком большой ответ");
    Ok(serde_json::from_str(&line)?)
}

pub fn spawn_worker(executable: &Path, tools: Option<&Path>) -> Result<()> {
    let dir = data_dir();
    std::fs::create_dir_all(&dir)?;
    let log = std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(dir.join("worker.log"))?;
    let mut cmd = std::process::Command::new(executable);
    cmd.arg("--worker")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(log);
    if let Some(tools) = tools {
        cmd.env("MEDIAHARBOR_TOOLS_DIR", tools);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }
    cmd.spawn().context("Не удалось запустить фоновое ядро")?;
    Ok(())
}
pub async fn ensure_worker(executable: &Path, tools: Option<&Path>) -> Result<()> {
    let ping = || Request {
        version: 1,
        request_id: uuid::Uuid::new_v4().to_string(),
        op: "ping".into(),
        payload: Value::Null,
    };
    // A normal request may take 180s. A readiness probe must not use that
    // budget on every retry (100 retries could otherwise take five hours).
    let ready = || async {
        matches!(
            tokio::time::timeout(std::time::Duration::from_millis(500), send(ping())).await,
            Ok(Ok(response)) if response["ok"] == true
        )
    };
    if ready().await {
        return Ok(());
    }
    spawn_worker(executable, tools)?;
    let startup = async {
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            if ready().await {
                return;
            }
        }
    };
    if tokio::time::timeout(std::time::Duration::from_secs(10), startup)
        .await
        .is_ok()
    {
        return Ok(());
    }
    anyhow::bail!(
        "Фоновое ядро не отвечает. Подробности: {}",
        data_dir().join("worker.log").display()
    )
}

pub fn validate_native(req: &Request) -> Result<()> {
    let allowed = [
        "ping",
        "jobs.list",
        "jobs.create",
        "jobs.cancel",
        "jobs.retry",
        "media.search",
        "media.analyze",
        "collection.page",
        "spotify.title",
        "spotify.status",
        "spotify.collection",
    ];
    anyhow::ensure!(
        allowed.contains(&req.op.as_str()),
        "Команда недоступна расширению"
    );
    if req.op == "jobs.create" {
        let inputs = req.payload["items"]
            .as_array()
            .context("Нет списка задач")?;
        anyhow::ensure!(
            inputs.iter().all(|i| i["kind"] == "download"),
            "Расширение может создавать только задачи загрузки"
        );
    }
    Ok(())
}
pub fn native_host() -> Result<()> {
    let runtime = tokio::runtime::Runtime::new()?;
    let executable = std::env::current_exe()?;
    runtime.block_on(ensure_worker(&executable, None))?;
    let stdin = std::io::stdin();
    let mut input = stdin.lock();
    let stdout = std::io::stdout();
    let mut output = stdout.lock();
    while let Some(req) = read_native_frame(&mut input)? {
        let result = validate_native(&req);
        let response = match result {
            Ok(()) => runtime.block_on(send(req.clone())).unwrap_or_else(|_|json!({"version":1,"requestId":req.request_id,"ok":false,"error":{"code":"WORKER_OFFLINE","message":"Ядро недоступно. Откройте приложение и повторите"}})),
            Err(e) => json!({"version":1,"requestId":req.request_id,"ok":false,"error":{"code":"FORBIDDEN","message":e.to_string()}}),
        };
        write_native_frame(&mut output, &response)?;
    }
    Ok(())
}

pub fn read_native_frame(input: &mut impl Read) -> Result<Option<Request>> {
    let mut size = [0u8; 4];
    if input.read(&mut size[..1])? == 0 {
        return Ok(None);
    }
    input.read_exact(&mut size[1..])?;
    let length = u32::from_le_bytes(size) as usize;
    anyhow::ensure!(
        length > 0 && length <= MAX_FRAME,
        "Некорректный размер сообщения"
    );
    let mut bytes = vec![0u8; length];
    input.read_exact(&mut bytes)?;
    Ok(Some(serde_json::from_slice(&bytes)?))
}
pub fn write_native_frame(output: &mut impl Write, response: &Value) -> Result<()> {
    let bytes = serde_json::to_vec(response)?;
    anyhow::ensure!(bytes.len() <= MAX_FRAME, "Слишком большой ответ");
    output.write_all(&(bytes.len() as u32).to_le_bytes())?;
    output.write_all(&bytes)?;
    output.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn authenticated_ipc_over_in_memory_transport() {
        let dir = tempfile::tempdir().unwrap();
        let engine = Engine::new(Store::open(dir.path().into()).unwrap());
        let (mut client, server) = tokio::io::duplex(4096);
        let handle = tokio::spawn(connection(server, engine, "test-secret".into()));
        client.write_all(b"{\"token\":\"test-secret\",\"request\":{\"version\":1,\"requestId\":\"test\",\"op\":\"ping\"}}\n").await.unwrap();
        let mut line = String::new();
        BufReader::new(client).read_line(&mut line).await.unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&line).unwrap()["data"]["name"],
            "MediaHarbor"
        );
        assert!(handle.await.unwrap().is_ok());
    }
    #[test]
    fn framing_rejects_truncated_or_oversized_messages() {
        let req = json!({"version":1,"requestId":"frame","op":"ping"});
        let mut bytes = vec![];
        write_native_frame(&mut bytes, &req).unwrap();
        assert_eq!(
            read_native_frame(&mut &bytes[..]).unwrap().unwrap().op,
            "ping"
        );
        assert!(read_native_frame(&mut &bytes[..bytes.len() - 1]).is_err());
        assert!(read_native_frame(&mut &[1u8, 2][..]).is_err());
        assert!(read_native_frame(&mut &(MAX_FRAME as u32 + 1).to_le_bytes()[..]).is_err());
        assert!(read_native_frame(&mut &[][..]).unwrap().is_none());
    }
}

pub fn install_bridge(executable: &Path, tools: Option<&Path>) -> Result<PathBuf> {
    use std::hash::Hasher;
    let dir = data_dir().join("native");
    std::fs::create_dir_all(&dir)?;
    let mut source = std::fs::File::open(executable)?;
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    let mut chunk = [0u8; 65536];
    loop {
        let count = source.read(&mut chunk)?;
        if count == 0 {
            break;
        }
        hash.write(&chunk[..count]);
    }
    let stable = dir.join(format!(
        "media-core-{}-{:x}{}",
        env!("CARGO_PKG_VERSION"),
        hash.finish(),
        std::env::consts::EXE_SUFFIX
    ));
    if !stable.exists() {
        let temporary = dir.join(format!("bridge-{}.tmp", uuid::Uuid::new_v4()));
        std::fs::copy(executable, &temporary)?;
        std::fs::rename(temporary, &stable)?;
    }
    let beside = executable.parent().unwrap_or(Path::new(".")).join("tools");
    let tools = tools.unwrap_or(&beside);
    let destination = dir.join("tools");
    if tools.is_dir() && tools.canonicalize().ok() != destination.canonicalize().ok() {
        std::fs::create_dir_all(&destination)?;
        let manifest = tools.join("DEPENDENCIES.json");
        let unchanged = manifest.is_file()
            && std::fs::read(&manifest).ok()
                == std::fs::read(destination.join("DEPENDENCIES.json")).ok();
        if !unchanged {
            for file in std::fs::read_dir(tools)? {
                let file = file?;
                if file.file_type()?.is_file() {
                    std::fs::copy(file.path(), destination.join(file.file_name()))?;
                }
            }
        }
    }
    Ok(stable)
}

pub fn register_host(chrome_id: &str, firefox_id: &str, executable: &Path) -> Result<Vec<PathBuf>> {
    anyhow::ensure!(
        chrome_id.len() == 32 && chrome_id.chars().all(|c| ('a'..='p').contains(&c)),
        "Некорректный ID Chromium-расширения"
    );
    anyhow::ensure!(
        firefox_id == "mediaharbor@mediaharbor.local",
        "Неизвестный ID Firefox"
    );
    let executable = install_bridge(executable, None)?;
    let dir = data_dir().join("native");
    std::fs::create_dir_all(&dir)?;
    let chrome = json!({"name":"io.mediaharbor.host","description":"MediaHarbor bridge","path":executable.canonicalize()?,"type":"stdio","allowed_origins":[format!("chrome-extension://{chrome_id}/")]});
    let firefox = json!({"name":"io.mediaharbor.host","description":"MediaHarbor bridge","path":executable.canonicalize()?,"type":"stdio","allowed_extensions":[firefox_id]});
    // The dedicated executable enters native mode when invoked by a browser.
    let mut installed = vec![];
    #[cfg(unix)]
    {
        let home = PathBuf::from(std::env::var_os("HOME").context("HOME не задан")?);
        let config = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".config"));
        for (root, value) in [
            (config.join("google-chrome/NativeMessagingHosts"), &chrome),
            (config.join("chromium/NativeMessagingHosts"), &chrome),
            (config.join("microsoft-edge/NativeMessagingHosts"), &chrome),
            (home.join(".mozilla/native-messaging-hosts"), &firefox),
        ] {
            std::fs::create_dir_all(&root)?;
            let path = root.join("io.mediaharbor.host.json");
            std::fs::write(&path, serde_json::to_vec_pretty(value)?)?;
            installed.push(path);
        }
    }
    #[cfg(windows)]
    {
        for (name, value, keys) in [
            (
                "chromium",
                &chrome,
                vec![
                    r"HKCU\Software\Google\Chrome\NativeMessagingHosts\io.mediaharbor.host",
                    r"HKCU\Software\Microsoft\Edge\NativeMessagingHosts\io.mediaharbor.host",
                ],
            ),
            (
                "firefox",
                &firefox,
                vec![r"HKCU\Software\Mozilla\NativeMessagingHosts\io.mediaharbor.host"],
            ),
        ] {
            let path = dir.join(format!("{name}.json"));
            std::fs::write(&path, serde_json::to_vec_pretty(value)?)?;
            for key in keys {
                let status = std::process::Command::new("reg.exe")
                    .args([
                        "ADD",
                        key,
                        "/ve",
                        "/t",
                        "REG_SZ",
                        "/d",
                        &path.to_string_lossy(),
                        "/f",
                    ])
                    .status()?;
                anyhow::ensure!(
                    status.success(),
                    "Не удалось зарегистрировать Native Messaging"
                );
            }
            installed.push(path);
        }
    }
    std::fs::write(dir.join("chromium-id.txt"), chrome_id)?;
    Ok(installed)
}

pub fn unregister_host() -> Result<()> {
    #[cfg(unix)]
    {
        let home = PathBuf::from(std::env::var_os("HOME").context("HOME не задан")?);
        let config = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".config"));
        for directory in [
            config.join("google-chrome/NativeMessagingHosts"),
            config.join("chromium/NativeMessagingHosts"),
            config.join("microsoft-edge/NativeMessagingHosts"),
            home.join(".mozilla/native-messaging-hosts"),
        ] {
            let path = directory.join("io.mediaharbor.host.json");
            if path.exists() {
                std::fs::remove_file(path)?;
            }
        }
    }
    #[cfg(windows)]
    {
        for key in [
            r"HKCU\Software\Google\Chrome\NativeMessagingHosts\io.mediaharbor.host",
            r"HKCU\Software\Microsoft\Edge\NativeMessagingHosts\io.mediaharbor.host",
            r"HKCU\Software\Mozilla\NativeMessagingHosts\io.mediaharbor.host",
        ] {
            let _ = std::process::Command::new("reg.exe")
                .args(["DELETE", key, "/f"])
                .output()?;
        }
    }
    Ok(())
}
