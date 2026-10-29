use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

pub const VERSION: u32 = 1;
pub const MAX_FRAME: usize = 512 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Request {
    pub version: u32,
    pub request_id: String,
    pub op: String,
    #[serde(default)]
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompressionOptions {
    pub format: String,
    pub preset: String,
    pub height: Option<u32>,
    pub audio_bitrate: u32,
    pub crf: Option<u32>,
    pub target_mb: Option<f64>,
}
impl CompressionOptions {
    pub fn validate(&self) -> Result<()> {
        if !["mp4", "mkv", "mp3", "m4a", "opus"].contains(&self.format.as_str()) {
            bail!("Неподдерживаемый формат");
        }
        if !["quality", "balanced", "small"].contains(&self.preset.as_str()) {
            bail!("Неизвестный пресет");
        }
        if !(32..=320).contains(&self.audio_bitrate) {
            bail!("Битрейт: 32–320 кбит/с");
        }
        if self.height.is_some_and(|v| !(144..=4320).contains(&v)) {
            bail!("Высота: 144–4320");
        }
        if self.crf.is_some_and(|v| v > 40) {
            bail!("CRF: 0–40");
        }
        if self
            .target_mb
            .is_some_and(|v| !v.is_finite() || v <= 0.0 || v > 1_000_000.0)
        {
            bail!("Некорректный целевой размер");
        }
        Ok(())
    }
    pub fn video(&self) -> bool {
        ["mp4", "mkv"].contains(&self.format.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadOptions {
    pub mode: String,
    pub format: String,
    pub height: u32,
    pub audio_bitrate: u32,
}
impl DownloadOptions {
    pub fn validate(&self) -> Result<()> {
        let allowed = if self.mode == "video" {
            vec!["mp4", "mkv"]
        } else if self.mode == "audio" {
            vec!["mp3", "m4a", "opus", "original"]
        } else {
            bail!("Неизвестный режим");
        };
        if !allowed.contains(&self.format.as_str())
            || !(144..=4320).contains(&self.height)
            || !(32..=320).contains(&self.audio_bitrate)
        {
            bail!("Некорректные параметры скачивания");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobInput {
    pub kind: String,
    pub source: String,
    pub title: String,
    pub download: Option<DownloadOptions>,
    pub compression: Option<CompressionOptions>,
}
impl JobInput {
    pub fn validate(&self) -> Result<()> {
        if self.title.len() > 2000 || self.source.len() > 8192 {
            bail!("Слишком длинное значение");
        }
        if let Some(c) = &self.compression {
            c.validate()?;
        }
        if self.download.as_ref().is_some_and(|d| d.mode == "audio")
            && self.compression.as_ref().is_some_and(|c| c.video())
        {
            bail!("Для загрузки аудио выберите аудиоформат сжатия");
        }
        match self.kind.as_str() {
            "download" => {
                youtube_url(&self.source)?;
                self.download
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("Не заданы параметры загрузки"))?
                    .validate()?;
            }
            "compress" => {
                if self.compression.is_none() {
                    bail!("Не задан профиль сжатия");
                }
                let path = std::path::Path::new(&self.source);
                if !path.is_absolute() || !path.is_file() {
                    bail!("Исходный файл не найден");
                }
            }
            _ => bail!("Неизвестный тип задачи"),
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    pub id: String,
    pub input: JobInput,
    pub status: String,
    pub stage: String,
    pub progress: f64,
    pub created_at: u64,
    pub updated_at: u64,
    pub output: Option<String>,
    pub original: Option<String>,
    pub error: Option<String>,
    pub warning: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub output_dir: String,
    pub theme: String,
    pub spotify_client_id: String,
    pub tools_dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaCandidate {
    pub id: String,
    pub title: String,
    pub url: String,
    pub artist: Option<String>,
    pub duration: Option<f64>,
    pub thumbnail: Option<String>,
}

pub fn youtube_url(value: &str) -> Result<String> {
    let value = value.trim();
    let rest = value
        .strip_prefix("https://")
        .ok_or_else(|| anyhow::anyhow!("Нужна HTTPS-ссылка YouTube"))?;
    anyhow::ensure!(
        rest.is_ascii() && !rest.chars().any(|c| c.is_control() || c.is_whitespace()),
        "Некорректная ссылка YouTube"
    );
    let (authority, suffix) = rest
        .split_once('/')
        .ok_or_else(|| anyhow::anyhow!("Нужна ссылка на ролик или плейлист"))?;
    let host = authority.to_ascii_lowercase();
    if ![
        "youtube.com",
        "www.youtube.com",
        "m.youtube.com",
        "music.youtube.com",
        "youtu.be",
    ]
    .contains(&host.as_str())
    {
        bail!("Поддерживаются ссылки YouTube без логина и нестандартного порта");
    }
    let suffix = suffix.split('#').next().unwrap_or_default();
    let (path, query) = suffix.split_once('?').unwrap_or((suffix, ""));
    let parameter = |key: &str| -> Option<String> {
        query
            .split('&')
            .filter_map(|part| part.split_once('='))
            .find(|(k, _)| *k == key)
            .map(|(_, v)| v.to_owned())
    };
    if host == "youtu.be" {
        validate_video_id(path)?;
        return Ok(format!("https://www.youtube.com/watch?v={path}"));
    }
    if path == "watch" {
        let id = parameter("v").unwrap_or_default();
        validate_video_id(&id)?;
        return Ok(format!("https://www.youtube.com/watch?v={id}"));
    }
    for prefix in ["shorts/", "live/"] {
        if let Some(id) = path.strip_prefix(prefix) {
            validate_video_id(id)?;
            return Ok(format!("https://www.youtube.com/watch?v={id}"));
        }
    }
    if path == "playlist" {
        let id = parameter("list").unwrap_or_default();
        if id.is_empty()
            || id.len() > 160
            || !id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            bail!("Некорректный ID плейлиста");
        }
        return Ok(format!("https://www.youtube.com/playlist?list={id}"));
    }
    bail!("Нужна ссылка на ролик или плейлист YouTube")
}

fn validate_video_id(id: &str) -> Result<()> {
    if id.len() != 11
        || !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        bail!("Некорректный ID видео");
    }
    Ok(())
}
pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
pub fn filename(value: &str) -> String {
    let mut clean = String::new();
    for c in value.chars() {
        let c = if c.is_control() || "<>:\"/\\|?*".contains(c) {
            '_'
        } else {
            c
        };
        if clean.len() + c.len_utf8() > 180 {
            break;
        }
        clean.push(c);
    }
    let clean = clean.trim_matches([' ', '.']);
    if clean.is_empty() {
        "media".into()
    } else {
        format!("media-{clean}")
    }
}
