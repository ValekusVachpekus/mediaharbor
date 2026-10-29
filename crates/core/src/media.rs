use crate::{
    model::*,
    process::{capture, run, strings, tool},
    store::Store,
};
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio_util::sync::CancellationToken;

pub async fn inspect(source: &str, configured: &str) -> Result<Vec<MediaCandidate>> {
    let source = youtube_url(source)?;
    let text = capture(
        &tool("yt-dlp", configured),
        &strings(&[
            "--ignore-config",
            "--no-warnings",
            "--flat-playlist",
            "--skip-download",
            "--ignore-errors",
            "--dump-single-json",
            "--",
            &source,
        ]),
    )
    .await?;
    let data: Value = serde_json::from_str(&text).context("Некорректный ответ yt-dlp")?;
    if data.get("is_live").and_then(Value::as_bool) == Some(true) {
        bail!("Прямые эфиры пока не поддерживаются");
    }
    if let Some(entries) = data.get("entries").and_then(Value::as_array) {
        Ok(entries.iter().filter_map(candidate).collect())
    } else {
        Ok(candidate(&data).into_iter().collect())
    }
}

pub async fn search(query: &str, configured: &str) -> Result<Vec<MediaCandidate>> {
    anyhow::ensure!(
        !query.trim().is_empty() && query.len() <= 500,
        "Введите запрос до 500 символов"
    );
    let text = capture(
        &tool("yt-dlp", configured),
        &strings(&[
            "--ignore-config",
            "--no-warnings",
            "--flat-playlist",
            "--dump-single-json",
            "--",
            &format!("ytsearch5:{}", query.trim()),
        ]),
    )
    .await?;
    let data: Value = serde_json::from_str(&text)?;
    Ok(data["entries"]
        .as_array()
        .map(|e| e.iter().filter_map(candidate).collect())
        .unwrap_or_default())
}
fn candidate(v: &Value) -> Option<MediaCandidate> {
    let id = v["id"].as_str()?;
    let url = format!("https://www.youtube.com/watch?v={id}");
    youtube_url(&url).ok()?;
    Some(MediaCandidate {
        id: id.into(),
        title: v["title"].as_str().unwrap_or("Видео недоступно").into(),
        url,
        artist: v["channel"]
            .as_str()
            .or_else(|| v["uploader"].as_str())
            .map(Into::into),
        duration: v["duration"].as_f64(),
        thumbnail: Some(format!("https://i.ytimg.com/vi/{id}/mqdefault.jpg")),
    })
}

pub async fn probe(path: &Path, configured: &str) -> Result<Value> {
    let args = strings(&[
        "-v",
        "error",
        "-show_format",
        "-show_streams",
        "-of",
        "json",
        "-i",
        &path.to_string_lossy(),
    ]);
    let text = capture(&tool("ffprobe", configured), &args).await?;
    Ok(serde_json::from_str(&text)?)
}
fn duration(probe: &Value) -> Result<f64> {
    let seconds = probe["format"]["duration"]
        .as_str()
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(0.0);
    anyhow::ensure!(
        seconds.is_finite() && seconds > 0.0,
        "Не удалось определить длительность файла"
    );
    Ok(seconds)
}

pub fn progress(store: &Store, id: &str, stage: &str, value: f64) {
    if let Ok(mut j) = store.job(id) {
        if j.status != "running" {
            return;
        }
        j.stage = stage.into();
        j.progress = value.clamp(0.0, 99.9);
        j.updated_at = now();
        let _ = store.save(&j);
    }
}

pub async fn download(store: Arc<Store>, job: &Job, cancel: &CancellationToken) -> Result<PathBuf> {
    let settings = store.settings()?;
    let options = job
        .input
        .download
        .as_ref()
        .context("Нет параметров загрузки")?;
    let source_dir = PathBuf::from(&settings.output_dir)
        .join("sources")
        .join(&job.id);
    tokio::fs::create_dir_all(&source_dir).await?;
    let out = source_dir.join("source.%(ext)s");
    let selector = if options.mode == "audio" {
        if options.format == "original" {
            "ba"
        } else {
            "ba/b"
        }
        .into()
    } else if options.format == "mp4" {
        format!("bv*[ext=mp4][vcodec^=avc1][height<={}]+ba[ext=m4a]/b[ext=mp4][vcodec^=avc1][height<={}]/bv*[height<={}]+ba/b[height<={}]", options.height, options.height, options.height, options.height)
    } else {
        format!(
            "bv*[height<={}]+ba/b[height<={}]",
            options.height, options.height
        )
    };
    let mut args = strings(&[
        "--ignore-config",
        "--no-playlist",
        "--match-filter",
        "!is_live",
        "--no-warnings",
        "--newline",
        "--no-simulate",
        "--continue",
        "--no-overwrites",
        "--no-cache-dir",
        "--progress",
        "--progress-template",
        "download:PROGRESS:%(progress._percent_str)s",
        "--print",
        "after_move:MEDIAFILE:%(filepath)j",
        "--merge-output-format",
        "mkv",
        "-f",
        &selector,
        "-o",
        &out.to_string_lossy(),
    ]);
    let ffmpeg = tool("ffmpeg", &settings.tools_dir);
    if ffmpeg.is_absolute() {
        args.extend(strings(&["--ffmpeg-location", &ffmpeg.to_string_lossy()]));
    }
    let deno = tool("deno", &settings.tools_dir);
    if deno.is_absolute() {
        args.extend(strings(&[
            "--js-runtimes",
            &format!("deno:{}", deno.to_string_lossy()),
        ]));
    }
    args.extend(strings(&["--", &youtube_url(&job.input.source)?]));
    let mut last = std::time::Instant::now() - std::time::Duration::from_secs(1);
    let text = run(
        &tool("yt-dlp", &settings.tools_dir),
        &args,
        cancel,
        |line| {
            if let Some(v) = line
                .strip_prefix("PROGRESS:")
                .and_then(|s| s.trim().trim_end_matches('%').trim().parse::<f64>().ok())
            {
                if last.elapsed().as_millis() > 300 {
                    progress(&store, &job.id, "Скачивание", v);
                    last = std::time::Instant::now();
                }
            }
        },
    )
    .await?;
    let path = text
        .lines()
        .rev()
        .find_map(|s| {
            s.strip_prefix("MEDIAFILE:")
                .and_then(|s| serde_json::from_str::<String>(s).ok())
        })
        .map(PathBuf::from)
        .context("Загрузчик не вернул путь результата")?;
    anyhow::ensure!(
        path.is_file() && path.canonicalize()?.starts_with(source_dir.canonicalize()?),
        "Неверный путь результата загрузки"
    );
    Ok(path)
}

pub fn encoding_args(
    profile: &CompressionOptions,
    info: &Value,
    total: f64,
) -> Result<(Vec<String>, bool)> {
    profile.validate()?;
    let streams = info["streams"]
        .as_array()
        .context("Файл не содержит потоков")?;
    let has_audio = streams.iter().any(|v| v["codec_type"] == "audio");
    let video = streams
        .iter()
        .find(|v| v["codec_type"] == "video" && v["disposition"]["attached_pic"] != 1);
    let mut args = vec![];
    let two_pass = profile.video() && profile.target_mb.is_some();
    let budget = profile
        .target_mb
        .map(|mb| mb * 1024.0 * 1024.0 * 8.0 * 0.97 / total / 1000.0);
    let audio_rate = if profile.video() {
        profile.audio_bitrate
    } else if let Some(b) = budget {
        b.floor().min(profile.audio_bitrate as f64) as u32
    } else {
        profile.audio_bitrate
    };
    if !profile.video() && audio_rate < 32 {
        bail!("Целевой размер слишком мал: требуется менее 32 кбит/с");
    }
    if profile.video() {
        anyhow::ensure!(video.is_some(), "В исходном файле нет видео");
        args.extend(strings(&[
            "-map", "0:V:0", "-map", "0:a:0?", "-c:v", "libx264", "-preset", "medium", "-pix_fmt",
            "yuv420p",
        ]));
        let max_height = profile
            .height
            .unwrap_or_else(|| video.unwrap()["height"].as_u64().unwrap_or(1080) as u32);
        args.extend(strings(&[
            "-vf",
            &format!("scale=w=-2:h='trunc(min({max_height},ih)/2)*2'"),
        ]));
        if let Some(budget) = budget {
            let video_rate = budget - if has_audio { audio_rate as f64 } else { 0.0 };
            anyhow::ensure!(
                video_rate >= 100.0,
                "Целевой размер слишком мал для видео и выбранного аудио"
            );
            args.extend(strings(&[
                "-b:v",
                &format!("{}k", video_rate.floor() as u64),
            ]));
        } else {
            let crf = profile.crf.unwrap_or(match profile.preset.as_str() {
                "quality" => 18,
                "small" => 28,
                _ => 23,
            });
            args.extend(strings(&["-crf", &crf.to_string()]));
        }
        args.extend(strings(&["-c:a", "aac", "-b:a", &format!("{audio_rate}k")]));
        if profile.format == "mp4" {
            args.extend(strings(&["-movflags", "+faststart"]));
        }
    } else {
        anyhow::ensure!(has_audio, "В исходном файле нет аудиодорожки");
        let codec = match profile.format.as_str() {
            "mp3" => "libmp3lame",
            "m4a" => "aac",
            _ => "libopus",
        };
        args.extend(strings(&[
            "-map",
            "0:a:0",
            "-vn",
            "-c:a",
            codec,
            "-b:a",
            &format!("{audio_rate}k"),
        ]));
    }
    args.extend(strings(&["-map_metadata", "0"]));
    Ok((args, two_pass))
}

pub async fn encode(
    store: Arc<Store>,
    job: &Job,
    source: &Path,
    profile: &CompressionOptions,
    cancel: &CancellationToken,
) -> Result<(PathBuf, Option<String>)> {
    let settings = store.settings()?;
    let info = probe(source, &settings.tools_dir).await?;
    let total = duration(&info)?;
    let (codec_args, two_pass) = encoding_args(profile, &info, total)?;
    let root = PathBuf::from(&settings.output_dir);
    let work = root.join(".mediaharbor-work").join(&job.id);
    tokio::fs::create_dir_all(&work).await?;
    let temporary = work.join(format!("result.{}", profile.format));
    let common = strings(&[
        "-hide_banner",
        "-nostdin",
        "-v",
        "error",
        "-y",
        "-i",
        &source.to_string_lossy(),
    ]);
    let passes: &[u32] = if two_pass { &[1, 2] } else { &[0] };
    for pass in passes {
        let mut args = common.clone();
        args.extend(codec_args.clone());
        if *pass > 0 {
            args.extend(strings(&[
                "-pass",
                &pass.to_string(),
                "-passlogfile",
                &work.join("pass").to_string_lossy(),
            ]));
        }
        args.extend(strings(&["-progress", "pipe:1", "-nostats"]));
        if *pass == 1 {
            args.extend(strings(&[
                "-an",
                "-f",
                "null",
                if cfg!(windows) { "NUL" } else { "/dev/null" },
            ]));
        } else {
            args.push(temporary.to_string_lossy().into());
        }
        let mut last = std::time::Instant::now() - std::time::Duration::from_secs(1);
        run(
            &tool("ffmpeg", &settings.tools_dir),
            &args,
            cancel,
            |line| {
                if let Some(micros) = line
                    .strip_prefix("out_time_us=")
                    .and_then(|s| s.parse::<f64>().ok())
                {
                    if last.elapsed().as_millis() > 300 {
                        let percent = (micros / 1_000_000.0 / total * 100.0).clamp(0.0, 99.9);
                        let percent = if two_pass {
                            percent / 2.0 + if *pass == 2 { 50.0 } else { 0.0 }
                        } else {
                            percent
                        };
                        progress(
                            &store,
                            &job.id,
                            if *pass == 1 {
                                "Сжатие: проход 1/2"
                            } else {
                                "Обработка медиа"
                            },
                            percent,
                        );
                        last = std::time::Instant::now();
                    }
                }
            },
        )
        .await?;
    }
    if cancel.is_cancelled() {
        bail!("Задача отменена");
    }
    let checked = probe(&temporary, &settings.tools_dir).await?;
    anyhow::ensure!(
        (duration(&checked)? - total).abs() <= 2.0_f64.max(total * 0.01),
        "Проверка длительности результата не пройдена"
    );
    let source_size = tokio::fs::metadata(source).await?.len();
    let output_size = tokio::fs::metadata(&temporary).await?.len();
    anyhow::ensure!(output_size > 0, "Инструмент создал пустой файл");
    let mut warnings = vec![];
    if output_size > source_size {
        warnings.push("Результат больше исходника".to_string());
    }
    if let Some(target) = profile.target_mb {
        if output_size as f64 > target * 1024.0 * 1024.0 {
            warnings.push(
                "Результат превышает целевой размер; уменьшите битрейт или разрешение".into(),
            );
        }
    }
    let final_path = publish(
        &temporary,
        &root,
        &filename(&job.input.title),
        &profile.format,
    )?;
    let _ = tokio::fs::remove_dir_all(&work).await;
    Ok((
        final_path,
        if warnings.is_empty() {
            None
        } else {
            Some(warnings.join(". "))
        },
    ))
}

pub fn publish(temp: &Path, dir: &Path, stem: &str, extension: &str) -> Result<PathBuf> {
    for i in 0..10000 {
        let name = if i == 0 {
            format!("{stem}.{extension}")
        } else {
            format!("{stem} ({i}).{extension}")
        };
        let destination = dir.join(name);
        match std::fs::hard_link(temp, &destination) {
            Ok(()) => return Ok(destination),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e).context("Не удалось атомарно сохранить файл: выберите диск с поддержкой hard links (NTFS/ext4)"),
        }
    }
    bail!("Слишком много файлов с одинаковым именем")
}

pub async fn tools(configured: &str) -> Value {
    let mut result = vec![];
    for name in ["yt-dlp", "ffmpeg", "ffprobe", "deno"] {
        let flag = if name.starts_with("ff") {
            "-version"
        } else {
            "--version"
        };
        let status = capture(&tool(name, configured), &strings(&[flag])).await;
        result.push(json!({"name":name,"available":status.is_ok(),"version":status.map(|s| s.lines().next().unwrap_or("").to_string()).unwrap_or_else(|e|e.to_string())}));
    }
    Value::Array(result)
}
