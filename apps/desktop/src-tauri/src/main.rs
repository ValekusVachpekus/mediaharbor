#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use mediaharbor_core::{ipc, model::Request};
use std::path::PathBuf;
use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    Manager,
};

fn core_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let suffix = std::env::consts::EXE_SUFFIX;
    let beside = std::env::current_exe()
        .map_err(|e| e.to_string())?
        .parent()
        .unwrap()
        .join(format!("media-core{suffix}"));
    if beside.is_file() {
        return Ok(beside);
    }
    let resource = app
        .path()
        .resource_dir()
        .map_err(|e| e.to_string())?
        .join(format!("media-core{suffix}"));
    if resource.is_file() {
        return Ok(resource);
    }
    let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(format!("../../../target/debug/media-core{suffix}"));
    if dev.is_file() {
        return Ok(dev);
    }
    Err("Ядро не найдено. Сначала выполните cargo build -p mediaharbor-core".into())
}
#[tauri::command]
async fn rpc(app: tauri::AppHandle, request: Request) -> Result<serde_json::Value, String> {
    let core = core_path(&app)?;
    let tools = app
        .path()
        .resource_dir()
        .map_err(|e| e.to_string())?
        .join("tools");
    ipc::ensure_worker(&core, Some(&tools))
        .await
        .map_err(|e| e.to_string())?;
    ipc::send(request).await.map_err(|e| e.to_string())
}
#[tauri::command]
async fn pick_files() -> Option<Vec<String>> {
    rfd::AsyncFileDialog::new()
        .set_title("Выберите медиафайлы")
        .pick_files()
        .await
        .map(|items| {
            items
                .iter()
                .map(|p| p.path().to_string_lossy().into_owned())
                .collect()
        })
}
#[tauri::command]
async fn pick_directory() -> Option<String> {
    rfd::AsyncFileDialog::new()
        .pick_folder()
        .await
        .map(|p| p.path().to_string_lossy().into_owned())
}
#[tauri::command]
fn register_browser(app: tauri::AppHandle, chrome_id: String) -> Result<Vec<PathBuf>, String> {
    let tools = app
        .path()
        .resource_dir()
        .map_err(|e| e.to_string())?
        .join("tools");
    let core = ipc::install_bridge(&core_path(&app)?, Some(&tools)).map_err(|e| e.to_string())?;
    ipc::register_host(&chrome_id, "mediaharbor@mediaharbor.local", &core)
        .map_err(|e| e.to_string())
}
#[tauri::command]
fn open_external(url: String) -> Result<(), String> {
    let url = reqwest_url_check(&url)?;
    #[cfg(target_os = "linux")]
    let mut command = std::process::Command::new("xdg-open");
    #[cfg(windows)]
    let mut command = {
        let mut c = std::process::Command::new("rundll32.exe");
        c.arg("url.dll,FileProtocolHandler");
        c
    };
    command.arg(url).spawn().map_err(|e| e.to_string())?;
    Ok(())
}
fn reqwest_url_check(url: &str) -> Result<&str, String> {
    if url.starts_with("https://accounts.spotify.com/authorize?")
        || url.starts_with("https://www.youtube.com/watch?v=")
        || url.starts_with("https://open.spotify.com/")
        || url.starts_with("https://github.com/yt-dlp/yt-dlp/releases")
        || url.starts_with("https://github.com/BtbN/FFmpeg-Builds/releases")
        || url.starts_with("https://github.com/denoland/deno/releases")
    {
        Ok(url)
    } else {
        Err("Недопустимый адрес".into())
    }
}
fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            rpc,
            pick_files,
            pick_directory,
            register_browser,
            open_external
        ])
        .setup(|app| {
            if core_path(app.handle()).is_ok() {
                let id = std::fs::read_to_string(
                    mediaharbor_core::store::data_dir().join("native/chromium-id.txt"),
                )
                .unwrap_or_else(|_| {
                    include_str!("../../../extension/public/chromium-id.txt").into()
                });
                if let Err(error) = register_browser(app.handle().clone(), id.trim().into()) {
                    eprintln!("Browser registration: {error}");
                }
            }
            let show = MenuItem::with_id(app, "show", "Открыть MediaHarbor", true, None::<&str>)?;
            let quit =
                MenuItem::with_id(app, "quit", "Выйти и остановить задачи", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &quit])?;
            let mut tray = TrayIconBuilder::new()
                .menu(&menu)
                .tooltip("MediaHarbor")
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    "quit" => {
                        let app = app.clone();
                        tauri::async_runtime::spawn(async move {
                            let _ = ipc::send(Request {
                                version: 1,
                                request_id: "exit".into(),
                                op: "worker.shutdown".into(),
                                payload: serde_json::Value::Null,
                            })
                            .await;
                            app.exit(0);
                        });
                    }
                    _ => {}
                });
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            let _tray = tray.build(app)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .run(tauri::generate_context!())
        .expect("Не удалось запустить MediaHarbor");
}
