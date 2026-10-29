use anyhow::Result;
use serde_json::{json, Value};

/// Read-only upstream version check. Installing/updating binaries is a separate
/// release operation, using the reviewed SHA-256 lock in prepare-tools.py.
pub async fn check() -> Result<Value> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .redirect(reqwest::redirect::Policy::none())
        .user_agent("MediaHarbor/0.1.0")
        .build()?;
    let mut updates = vec![];
    for (name, repository) in [
        ("yt-dlp", "yt-dlp/yt-dlp"),
        ("FFmpeg", "BtbN/FFmpeg-Builds"),
        ("Deno", "denoland/deno"),
    ] {
        let release: Value = client
            .get(format!(
                "https://api.github.com/repos/{repository}/releases/latest"
            ))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        updates.push(json!({"name":name,"version":release["tag_name"],"url":format!("https://github.com/{repository}/releases/latest")}));
    }
    Ok(json!(updates))
}
