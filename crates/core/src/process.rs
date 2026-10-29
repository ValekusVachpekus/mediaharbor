use anyhow::{bail, Context, Result};
use std::{
    path::{Path, PathBuf},
    process::Stdio,
};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::Command,
};
use tokio_util::sync::CancellationToken;

pub fn tool(name: &str, configured: &str) -> PathBuf {
    let executable = if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_owned()
    };
    for dir in [
        (!configured.is_empty()).then(|| PathBuf::from(configured)),
        std::env::var_os("MEDIAHARBOR_TOOLS_DIR").map(PathBuf::from),
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.join("tools"))),
    ]
    .into_iter()
    .flatten()
    {
        let path = dir.join(&executable);
        if path.is_file() {
            return path;
        }
    }
    PathBuf::from(executable)
}

pub async fn run<F>(
    exe: &Path,
    args: &[String],
    cancel: &CancellationToken,
    mut on_line: F,
) -> Result<String>
where
    F: FnMut(&str) + Send,
{
    let mut cmd = Command::new(exe);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(unix)]
    {
        cmd.process_group(0);
    }
    #[cfg(windows)]
    cmd.creation_flags(0x08000000);
    let mut child = cmd.spawn().with_context(|| {
        format!(
            "Не удалось запустить {}. Проверьте инструменты в настройках",
            exe.display()
        )
    })?;
    let mut tree = ProcessTree::new(&child)?;
    let mut stdout = BufReader::new(child.stdout.take().unwrap()).lines();
    let mut stderr = BufReader::new(child.stderr.take().unwrap()).lines();
    let mut result = String::new();
    let mut errors = String::new();
    let mut out_done = false;
    let mut err_done = false;
    while !out_done || !err_done {
        tokio::select! {
            biased;
            _ = cancel.cancelled() => { let _ = child.kill().await; let _ = child.wait().await; bail!("Задача отменена"); }
            line = stdout.next_line(), if !out_done => match line? {
                Some(line) => { on_line(&line); if result.len() + line.len() < 16 * 1024 * 1024 { result.push_str(&line); result.push('\n'); } else { let _ = child.kill().await; bail!("Ответ инструмента превышает 16 МБ; разделите коллекцию"); } }
                None => out_done = true,
            },
            line = stderr.next_line(), if !err_done => match line? {
                Some(line) => { if errors.len() + line.len() > 16000 { errors.clear(); } errors.push_str(&line); errors.push('\n'); }
                None => err_done = true,
            },
            else => break,
        }
    }
    let status = tokio::select! {
        _ = cancel.cancelled() => { let _ = child.kill().await; let _ = child.wait().await; bail!("Задача отменена"); },
        status = child.wait() => status?,
    };
    if !status.success() {
        bail!(
            "{}: {}",
            exe.file_name().unwrap_or_default().to_string_lossy(),
            errors.trim()
        );
    }
    tree.disarm();
    Ok(result)
}

pub async fn capture(exe: &Path, args: &[String]) -> Result<String> {
    let cancel = CancellationToken::new();
    match tokio::time::timeout(
        std::time::Duration::from_secs(90),
        run(exe, args, &cancel, |_| {}),
    )
    .await
    {
        Ok(result) => result,
        Err(_) => bail!("Превышено время ожидания инструмента"),
    }
}

pub fn strings(args: &[&str]) -> Vec<String> {
    args.iter().map(|s| (*s).into()).collect()
}

// Killing the group/job also stops ffmpeg launched by yt-dlp. The guard runs on
// timeout, cancellation and errors, including when an async future is dropped.
#[cfg(unix)]
struct ProcessTree(u32);
#[cfg(unix)]
impl ProcessTree {
    fn new(child: &tokio::process::Child) -> Result<Self> {
        Ok(Self(child.id().context("Нет PID процесса")?))
    }
    fn disarm(&mut self) {
        self.0 = 0;
    }
}
#[cfg(unix)]
impl Drop for ProcessTree {
    fn drop(&mut self) {
        if self.0 != 0 {
            unsafe {
                libc::kill(-(self.0 as i32), libc::SIGKILL);
            }
        }
    }
}

#[cfg(windows)]
struct ProcessTree(windows_sys::Win32::Foundation::HANDLE);
#[cfg(windows)]
unsafe impl Send for ProcessTree {}
#[cfg(windows)]
impl ProcessTree {
    fn disarm(&mut self) {}
    fn new(child: &tokio::process::Child) -> Result<Self> {
        use windows_sys::Win32::System::JobObjects::*;
        unsafe {
            let handle = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            anyhow::ensure!(!handle.is_null(), "Не удалось создать группу процессов");
            let guard = Self(handle);
            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            anyhow::ensure!(
                SetInformationJobObject(
                    handle,
                    JobObjectExtendedLimitInformation,
                    &info as *const _ as *const _,
                    std::mem::size_of_val(&info) as u32
                ) != 0,
                "Не удалось настроить группу процессов"
            );
            anyhow::ensure!(
                AssignProcessToJobObject(
                    handle,
                    child.raw_handle().context("Нет handle процесса")? as _
                ) != 0,
                "Не удалось присоединить процесс к группе"
            );
            Ok(guard)
        }
    }
}
#[cfg(windows)]
impl Drop for ProcessTree {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.0);
        }
    }
}
