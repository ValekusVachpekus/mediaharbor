use mediaharbor_core::{
    media::{encoding_args, publish},
    model::*,
    store::Store,
};
use serde_json::json;

fn profile() -> CompressionOptions {
    CompressionOptions {
        format: "mp4".into(),
        preset: "balanced".into(),
        height: Some(1080),
        audio_bitrate: 192,
        crf: None,
        target_mb: None,
    }
}

#[test]
fn canonical_sources_reject_commands_and_untrusted_hosts() {
    assert_eq!(
        youtube_url("https://youtu.be/abcdefghijk?t=2").unwrap(),
        "https://www.youtube.com/watch?v=abcdefghijk"
    );
    assert_eq!(
        youtube_url("https://www.youtube.com/shorts/abcdefghijk").unwrap(),
        "https://www.youtube.com/watch?v=abcdefghijk"
    );
    for source in [
        "--exec=sh",
        "file:///tmp/test",
        "https://youtube.com.evil.test/watch?v=abcdefghijk",
        "https://user@youtube.com/watch?v=abcdefghijk",
        "https://youtube.com:8443/watch?v=abcdefghijk",
        "https://youtube.com/watch?v=../etc/passwd",
    ] {
        assert!(youtube_url(source).is_err(), "{source}");
    }
}
#[test]
fn encoding_budget_accounts_for_audio_and_rejects_impossible_targets() {
    let info = json!({"streams":[{"codec_type":"video","height":720},{"codec_type":"audio"}]});
    let mut p = profile();
    p.target_mb = Some(10.0);
    let (args, two_pass) = encoding_args(&p, &info, 60.0).unwrap();
    assert!(two_pass);
    assert!(args.contains(&"libx264".to_string()));
    let bitrate: u32 = args[args.iter().position(|a| a == "-b:v").unwrap() + 1]
        .trim_end_matches('k')
        .parse()
        .unwrap();
    assert!(bitrate < 1200 && bitrate > 1000);
    p.target_mb = Some(0.01);
    assert!(encoding_args(&p, &info, 60.0).is_err());
    p.target_mb = None;
    p.crf = Some(99);
    assert!(p.validate().is_err());
}
#[test]
fn audio_profile_requires_an_audio_stream() {
    let mut p = profile();
    p.format = "mp3".into();
    assert!(encoding_args(&p, &json!({"streams":[{"codec_type":"video"}]}), 10.0).is_err());
}
#[test]
fn publication_never_overwrites_an_existing_file() {
    let dir = tempfile::tempdir().unwrap();
    let temp = dir.path().join("temp");
    std::fs::write(&temp, b"new").unwrap();
    std::fs::write(dir.path().join("song.mp3"), b"original").unwrap();
    let path = publish(&temp, dir.path(), "song", "mp3").unwrap();
    assert_eq!(path.file_name().unwrap(), "song (1).mp3");
    assert_eq!(
        std::fs::read(dir.path().join("song.mp3")).unwrap(),
        b"original"
    );
}
#[test]
fn store_marks_unfinished_jobs_interrupted_and_preserves_history() {
    let dir = tempfile::tempdir().unwrap();
    let job = Job {
        id: "test".into(),
        input: JobInput {
            kind: "download".into(),
            source: "https://youtu.be/abcdefghijk".into(),
            title: "Test".into(),
            download: Some(DownloadOptions {
                mode: "audio".into(),
                format: "mp3".into(),
                height: 1080,
                audio_bitrate: 192,
            }),
            compression: None,
        },
        status: "running".into(),
        stage: "download".into(),
        progress: 33.0,
        created_at: now(),
        updated_at: now(),
        output: None,
        original: None,
        error: None,
        warning: None,
    };
    {
        let store = Store::open(dir.path().into()).unwrap();
        store.save(&job).unwrap();
    }
    let store = Store::open(dir.path().into()).unwrap();
    let restored = store.job("test").unwrap();
    assert_eq!(restored.status, "interrupted");
    assert_eq!(restored.progress, 33.0);
}

#[tokio::test]
async fn cancellation_terminates_a_running_process() {
    #[cfg(unix)]
    {
        let token = tokio_util::sync::CancellationToken::new();
        let cancel = token.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            cancel.cancel();
        });
        let started = std::time::Instant::now();
        let result = mediaharbor_core::process::run(
            std::path::Path::new("sleep"),
            &["30".into()],
            &token,
            |_| {},
        )
        .await;
        assert!(result.is_err());
        assert!(started.elapsed().as_secs() < 3);
    }
}

#[tokio::test]
async fn real_ffmpeg_compression_keeps_input_and_produces_playable_output() {
    if std::process::Command::new("ffmpeg")
        .arg("-version")
        .output()
        .is_err()
    {
        eprintln!("ffmpeg absent: integration test skipped");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("input.wav");
    assert!(std::process::Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:duration=2",
            "-y"
        ])
        .arg(&source)
        .status()
        .unwrap()
        .success());
    let store = std::sync::Arc::new(Store::open(dir.path().join("state")).unwrap());
    let mut settings = store.settings().unwrap();
    settings.output_dir = dir.path().join("output").to_string_lossy().into();
    store.set_settings(&settings).unwrap();
    let input = JobInput {
        kind: "compress".into(),
        source: source.to_string_lossy().into(),
        title: "tone".into(),
        download: None,
        compression: Some(CompressionOptions {
            format: "mp3".into(),
            ..profile()
        }),
    };
    let job = Job {
        id: "encode-test".into(),
        input,
        status: "running".into(),
        stage: "encode".into(),
        progress: 0.0,
        created_at: now(),
        updated_at: now(),
        output: None,
        original: None,
        error: None,
        warning: None,
    };
    store.save(&job).unwrap();
    let (output, _) = mediaharbor_core::media::encode(
        store,
        &job,
        &source,
        job.input.compression.as_ref().unwrap(),
        &tokio_util::sync::CancellationToken::new(),
    )
    .await
    .unwrap();
    assert!(source.is_file());
    assert!(output.is_file());
    let probe = mediaharbor_core::media::probe(&output, "").await.unwrap();
    assert_eq!(probe["streams"][0]["codec_name"], "mp3");
}

#[tokio::test]
async fn real_two_pass_video_does_not_upscale_and_preserves_duration() {
    if std::process::Command::new("ffmpeg")
        .arg("-version")
        .output()
        .is_err()
    {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("input.mp4");
    assert!(std::process::Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=320x180:rate=12:duration=2",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:duration=2",
            "-c:v",
            "libx264",
            "-c:a",
            "aac",
            "-shortest",
            "-y"
        ])
        .arg(&source)
        .status()
        .unwrap()
        .success());
    let store = std::sync::Arc::new(Store::open(dir.path().join("state")).unwrap());
    let mut settings = store.settings().unwrap();
    settings.output_dir = dir.path().join("output").to_string_lossy().into();
    store.set_settings(&settings).unwrap();
    let profile = CompressionOptions {
        target_mb: Some(0.2),
        height: Some(1080),
        audio_bitrate: 96,
        ..profile()
    };
    let job = Job {
        id: "video-test".into(),
        input: JobInput {
            kind: "compress".into(),
            source: source.to_string_lossy().into(),
            title: "video".into(),
            download: None,
            compression: Some(profile.clone()),
        },
        status: "running".into(),
        stage: "encode".into(),
        progress: 0.0,
        created_at: now(),
        updated_at: now(),
        output: None,
        original: None,
        error: None,
        warning: None,
    };
    store.save(&job).unwrap();
    let (output, _) = mediaharbor_core::media::encode(
        store,
        &job,
        &source,
        &profile,
        &tokio_util::sync::CancellationToken::new(),
    )
    .await
    .unwrap();
    let info = mediaharbor_core::media::probe(&output, "").await.unwrap();
    let stream = info["streams"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["codec_type"] == "video")
        .unwrap();
    assert_eq!(stream["codec_name"], "h264");
    assert_eq!(stream["height"], 180);
    assert!(
        (info["format"]["duration"]
            .as_str()
            .unwrap()
            .parse::<f64>()
            .unwrap()
            - 2.0)
            .abs()
            < 0.2
    );
    assert!(std::fs::metadata(output).unwrap().len() < 300_000);
    assert!(source.is_file());
}
