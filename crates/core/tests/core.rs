use mediaharbor_core::{engine::Engine, ipc::validate_native, model::Request, store::Store};
use serde_json::json;
fn request(op: &str, payload: serde_json::Value) -> Request {
    Request {
        version: 1,
        request_id: uuid::Uuid::new_v4().to_string(),
        op: op.into(),
        payload,
    }
}
#[test]
fn native_client_cannot_transcode_local_paths_or_change_tools() {
    assert!(validate_native(&request("settings.set", json!({"toolsDir":"/tmp"}))).is_err());
    assert!(validate_native(&request(
        "jobs.create",
        json!({"items":[{"kind":"compress","source":"/etc/passwd"}]})
    ))
    .is_err());
    assert!(validate_native(&request("worker.shutdown", json!(null))).is_err());
    assert!(validate_native(&request("jobs.list", json!({}))).is_ok());
}
#[tokio::test]
async fn worker_rejects_incompatible_protocol_and_unknown_operations() {
    let dir = tempfile::tempdir().unwrap();
    let engine = Engine::new(Store::open(dir.path().into()).unwrap());
    let mut req = request("ping", json!(null));
    req.version = 99;
    assert_eq!(
        engine.response(req).await["error"]["code"],
        "VERSION_MISMATCH"
    );
    assert_eq!(
        engine.response(request("unknown", json!(null))).await["ok"],
        false
    );
}

#[tokio::test]
async fn duplicate_creation_is_idempotent_and_request_id_cannot_change_payload() {
    let dir = tempfile::tempdir().unwrap();
    let engine = Engine::new(Store::open(dir.path().into()).unwrap());
    let source = dir.path().join("invalid-media.wav");
    std::fs::write(&source, b"not media").unwrap();
    let req = request(
        "jobs.create",
        json!({"items":[{"kind":"compress","title":"Test","source":source,"compression":{"format":"mp3","preset":"balanced","audioBitrate":192}}]}),
    );
    let first = engine.response(req.clone()).await;
    let second = engine.response(req.clone()).await;
    assert_eq!(first, second);
    assert_eq!(engine.store.jobs().unwrap().len(), 1);
    let mut changed = req;
    changed.payload["items"][0]["title"] = json!("Changed");
    assert_eq!(engine.response(changed).await["ok"], false);
    engine.stop().await;
}

#[test]
#[ignore = "requires OS sockets; run with --include-ignored outside restricted sandboxes"]
fn native_framing_and_worker_lifecycle_roundtrip() {
    use std::{
        process::{Command, Output, Stdio},
        time::{Duration, Instant},
    };
    fn run(mut command: Command, dir: &std::path::Path, stage: &str, input: &[u8]) -> Output {
        eprintln!("IPC lifecycle: {stage}");
        // Files avoid waiting for pipe EOF from handles inherited by a
        // long-lived Windows worker. Also bound process exit separately.
        let stdout = dir.join(format!("{stage}.stdout"));
        let stderr = dir.join(format!("{stage}.stderr"));
        let stdin = dir.join(format!("{stage}.stdin"));
        std::fs::write(&stdin, input).unwrap();
        let mut child = command
            .stdin(Stdio::from(std::fs::File::open(stdin).unwrap()))
            .stdout(Stdio::from(std::fs::File::create(&stdout).unwrap()))
            .stderr(Stdio::from(std::fs::File::create(&stderr).unwrap()))
            .spawn()
            .unwrap();
        let started = Instant::now();
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            if started.elapsed() > Duration::from_secs(30) {
                let _ = child.kill();
                let _ = child.wait();
                panic!(
                    "IPC lifecycle stage {stage} timed out. stderr: {}\nworker.log: {}",
                    std::fs::read_to_string(&stderr).unwrap_or_default(),
                    std::fs::read_to_string(dir.join("worker.log")).unwrap_or_default(),
                );
            }
            std::thread::sleep(Duration::from_millis(25));
        };
        let output = Output {
            status,
            stdout: std::fs::read(stdout).unwrap(),
            stderr: std::fs::read(stderr).unwrap(),
        };
        assert!(
            output.status.success(),
            "IPC lifecycle stage {stage}: {}\nworker.log: {}",
            String::from_utf8_lossy(&output.stderr),
            std::fs::read_to_string(dir.join("worker.log")).unwrap_or_default(),
        );
        output
    }
    let binary = option_env!("CARGO_BIN_EXE_offline-core")
        .or(option_env!("CARGO_BIN_EXE_media-core"))
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let ping = request("ping", json!(null));
    let mut command = Command::new(binary);
    command
        .args(["--request", &serde_json::to_string(&ping).unwrap()])
        .env("MEDIAHARBOR_DATA_DIR", dir.path());
    let result = run(command, dir.path(), "startup-ping", &[]);
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&result.stdout).unwrap()["data"]["name"],
        "MediaHarbor"
    );
    let mut command = Command::new(binary);
    command
        .arg("--native-host")
        .env("MEDIAHARBOR_DATA_DIR", dir.path());
    let bytes = serde_json::to_vec(&request("jobs.list", json!({"offset":0}))).unwrap();
    let mut input = (bytes.len() as u32).to_le_bytes().to_vec();
    input.extend_from_slice(&bytes);
    let result = run(command, dir.path(), "native-frame", &input);
    assert!(result.stdout.len() >= 4, "Missing native response frame");
    let count = u32::from_le_bytes(result.stdout[..4].try_into().unwrap()) as usize;
    assert_eq!(count, result.stdout.len() - 4);
    let response: serde_json::Value = serde_json::from_slice(&result.stdout[4..]).unwrap();
    assert_eq!(response["ok"], true);
    assert_eq!(response["data"]["total"], 0);
    let mut command = Command::new(binary);
    command
        .args([
            "--request",
            &serde_json::to_string(&request("worker.shutdown", json!(null))).unwrap(),
        ])
        .env("MEDIAHARBOR_DATA_DIR", dir.path());
    run(command, dir.path(), "shutdown", &[]);
}
