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
        io::Write,
        process::{Command, Stdio},
    };
    let binary = option_env!("CARGO_BIN_EXE_offline-core")
        .or(option_env!("CARGO_BIN_EXE_media-core"))
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let ping = request("ping", json!(null));
    let result = Command::new(binary)
        .args(["--request", &serde_json::to_string(&ping).unwrap()])
        .env("MEDIAHARBOR_DATA_DIR", dir.path())
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&result.stdout).unwrap()["data"]["name"],
        "MediaHarbor"
    );
    let mut child = Command::new(binary)
        .arg("--native-host")
        .env("MEDIAHARBOR_DATA_DIR", dir.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let bytes = serde_json::to_vec(&request("jobs.list", json!({"offset":0}))).unwrap();
    let mut stdin = child.stdin.take().unwrap();
    stdin
        .write_all(&(bytes.len() as u32).to_le_bytes())
        .unwrap();
    stdin.write_all(&bytes).unwrap();
    drop(stdin);
    let result = child.wait_with_output().unwrap();
    assert!(result.status.success());
    let count = u32::from_le_bytes(result.stdout[..4].try_into().unwrap()) as usize;
    assert_eq!(count, result.stdout.len() - 4);
    let response: serde_json::Value = serde_json::from_slice(&result.stdout[4..]).unwrap();
    assert_eq!(response["ok"], true);
    assert_eq!(response["data"]["total"], 0);
    let result = Command::new(binary)
        .args([
            "--request",
            &serde_json::to_string(&request("worker.shutdown", json!(null))).unwrap(),
        ])
        .env("MEDIAHARBOR_DATA_DIR", dir.path())
        .output()
        .unwrap();
    assert!(result.status.success());
}
