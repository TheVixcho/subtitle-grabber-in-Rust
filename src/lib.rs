pub mod grabber;
pub mod ytdlp;

use grabber::{DownloadResult, SavedSubtitle, VideoSubtitles};
use serde::Serialize;
use std::path::PathBuf;
use ytdlp::YtdlpStatus;

const ASCII_BANNER: &str = r#" ___       _   _   _ _   _          ___           _     _             
/ __|_  _ | |_| |_(_) |_| |___ ___ / __|_ _ __ _ | |__ | |__  ___ _ _ 
\__ \ || || _ \  _| |  _| / -_)___| (_ | '_/ _` || '_ \| '_ \/ -_) '_|
|___/\_,_||_.__/\__|_|\__|_\___|   \___|_| \__,_||_.__/|_.__/\___|_|  
     subtitle-graber original idea by Meke and TheVixcho dev GUI"#;

#[derive(Debug, Clone, Serialize)]
pub struct AppInfo {
    pub name: String,
    pub version: String,
    pub default_dir: String,
    pub managed_ytdlp_path: String,
    pub platform: String,
    pub ascii_art: String,
}

#[tauri::command]
fn get_app_info() -> AppInfo {
    AppInfo {
        name: "Subtitle-Grabber in Rust".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        default_dir: grabber::default_subs_dir().display().to_string(),
        managed_ytdlp_path: ytdlp::managed_binary_path().display().to_string(),
        platform: std::env::consts::OS.to_string(),
        ascii_art: ASCII_BANNER.to_string(),
    }
}

#[tauri::command]
async fn check_ytdlp() -> YtdlpStatus {
    ytdlp::check_status().await
}

#[tauri::command]
async fn install_ytdlp() -> Result<YtdlpStatus, String> {
    ytdlp::download_managed_ytdlp().await
}

#[tauri::command]
async fn fetch_subtitles(url: String) -> Result<VideoSubtitles, String> {
    let trimmed = url.trim().to_string();
    if !grabber::is_youtube_url(&trimmed) {
        return Err("That doesn't look like a YouTube URL.".to_string());
    }

    let (exe, _, _) = ytdlp::resolve_ytdlp().await.ok_or_else(|| {
        "yt-dlp isn't installed yet. Click 'Install yt-dlp' in the status bar or Engine tab."
            .to_string()
    })?;

    grabber::get_subtitle_options(&exe, &trimmed)
        .await
        .map_err(|raw| grabber::friendly_error(&raw))
}

#[tauri::command]
async fn download_subtitle_cmd(
    url: String,
    lang_code: String,
    auto: bool,
    out_dir: String,
) -> Result<DownloadResult, String> {
    let (exe, _, _) = ytdlp::resolve_ytdlp().await.ok_or_else(|| {
        "yt-dlp isn't installed yet. Click 'Install yt-dlp' first.".to_string()
    })?;

    let target_dir = grabber::expand_user_path(&out_dir);
    let result = grabber::download_subtitle(&exe, url.trim(), &lang_code, auto, &target_dir)
        .await
        .map_err(|raw| {
            if raw.to_lowercase().contains("permission") || raw.to_lowercase().contains("couldn't use") {
                raw
            } else {
                grabber::friendly_error(&raw)
            }
        })?;

    result.ok_or_else(|| {
        "Download finished, but the .srt file couldn't be found afterwards.".to_string()
    })
}

#[tauri::command]
async fn pick_folder(initial_dir: Option<String>) -> Option<String> {
    let mut dialog = rfd::AsyncFileDialog::new().set_title("Choose subtitle destination folder");
    if let Some(dir) = initial_dir {
        let expanded = grabber::expand_user_path(&dir);
        if expanded.is_dir() {
            dialog = dialog.set_directory(expanded);
        }
    }
    dialog
        .pick_folder()
        .await
        .map(|handle| handle.path().display().to_string())
}

#[tauri::command]
async fn pick_css_file() -> Result<Option<String>, String> {
    let handle = rfd::AsyncFileDialog::new()
        .set_title("Import custom CSS")
        .add_filter("CSS Stylesheet", &["css"])
        .pick_file()
        .await;
    match handle {
        Some(file) => {
            let bytes = file.read().await;
            let text = String::from_utf8(bytes)
                .map_err(|_| "Selected CSS file is not valid UTF-8.".to_string())?;
            Ok(Some(text))
        }
        None => Ok(None),
    }
}

#[tauri::command]
async fn list_saved_subtitles(dir: String) -> Result<Vec<SavedSubtitle>, String> {
    let target = grabber::expand_user_path(&dir);
    grabber::list_saved_subtitles(&target)
}

#[tauri::command]
async fn read_subtitle_preview(path: String) -> Result<String, String> {
    let p = PathBuf::from(path);
    grabber::read_subtitle_preview(&p, 65_536)
}

#[tauri::command]
async fn open_in_file_manager(path: String) -> Result<(), String> {
    let target = grabber::expand_user_path(&path);
    let to_open = if target.is_file() {
        target.parent().unwrap_or(&target).to_path_buf()
    } else {
        target
    };
    if !to_open.exists() {
        std::fs::create_dir_all(&to_open)
            .map_err(|e| format!("Couldn't create folder: {e}"))?;
    }
    opener::open(&to_open).map_err(|e| format!("Couldn't open folder: {e}"))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            get_app_info,
            check_ytdlp,
            install_ytdlp,
            fetch_subtitles,
            download_subtitle_cmd,
            pick_folder,
            pick_css_file,
            list_saved_subtitles,
            read_subtitle_preview,
            open_in_file_manager,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Subtitle-Grabber in Rust");
}
