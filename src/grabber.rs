use serde::Serialize;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use tokio::process::Command;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SubtitleTrack {
    pub code: String,
    pub name: String,
    pub auto: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct VideoSubtitles {
    pub title: String,
    pub channel: Option<String>,
    pub duration: Option<String>,
    pub thumbnail: Option<String>,
    pub webpage_url: Option<String>,
    pub languages: Vec<SubtitleTrack>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DownloadResult {
    pub path: String,
    pub filename: String,
    pub size_bytes: u64,
    pub preview: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SavedSubtitle {
    pub filename: String,
    pub path: String,
    pub size_bytes: u64,
    pub modified_epoch_secs: u64,
}

pub fn default_subs_dir() -> PathBuf {
    let downloads = dirs::download_dir().or_else(|| dirs::home_dir().map(|h| h.join("Downloads")));
    downloads
        .unwrap_or_else(|| PathBuf::from("."))
        .join("subtitles")
}

pub fn expand_user_path(raw: &str) -> PathBuf {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return default_subs_dir();
    }
    if trimmed == "~" {
        return dirs::home_dir().unwrap_or_else(|| PathBuf::from(trimmed));
    }
    if let Some(rest) = trimmed.strip_prefix("~/").or_else(|| trimmed.strip_prefix("~\\")) {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(trimmed)
}

pub fn is_youtube_url(url: &str) -> bool {
    let lower = url.to_lowercase();
    lower.contains("youtube.com") || lower.contains("youtu.be")
}

pub fn is_real_caption(fmt: &Value) -> bool {
    match fmt.get("url").and_then(Value::as_str) {
        Some(url) => !url.contains("tlang="),
        None => true,
    }
}

pub fn last_error_line(stderr: &str) -> String {
    stderr
        .lines()
        .rev()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(ToString::to_string)
        .unwrap_or_else(|| "yt-dlp failed for an unknown reason.".to_string())
}

pub fn friendly_error(error_text: &str) -> String {
    let text = error_text.to_lowercase();
    if text.contains("private video") {
        return "That video is private.".to_string();
    }
    if text.contains("video unavailable") {
        return "That video is unavailable (deleted or region-blocked).".to_string();
    }
    if text.contains("sign in") || text.contains("confirm your age") {
        return "That video requires sign-in and can't be downloaded here.".to_string();
    }
    if text.contains("no module named") || text.contains("not found") || text.contains("missing yt-dlp") {
        return "yt-dlp isn't installed or available. Click 'Install yt-dlp' in the bottom bar or Settings.".to_string();
    }
    "Couldn't read that video. Double-check the URL and try again.".to_string()
}

fn format_duration(info: &Value) -> Option<String> {
    if let Some(s) = info.get("duration_string").and_then(Value::as_str) {
        if !s.is_empty() {
            return Some(s.to_string());
        }
    }
    let secs = info.get("duration").and_then(Value::as_u64)?;
    let hours = secs / 3600;
    let mins = (secs % 3600) / 60;
    let rem = secs % 60;
    if hours > 0 {
        Some(format!("{hours}:{mins:02}:{rem:02}"))
    } else {
        Some(format!("{mins}:{rem:02}"))
    }
}

pub fn parse_subtitle_options(json_str: &str) -> Result<VideoSubtitles, String> {
    let info: Value =
        serde_json::from_str(json_str).map_err(|e| format!("Invalid JSON from yt-dlp: {e}"))?;

    let title = info
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("Unknown title")
        .to_string();

    let channel = info
        .get("channel")
        .or_else(|| info.get("uploader"))
        .and_then(Value::as_str)
        .map(ToString::to_string);

    let duration = format_duration(&info);
    let thumbnail = info
        .get("thumbnail")
        .and_then(Value::as_str)
        .map(ToString::to_string);
    let webpage_url = info
        .get("webpage_url")
        .and_then(Value::as_str)
        .map(ToString::to_string);

    let mut tracks: HashMap<String, SubtitleTrack> = HashMap::new();

    if let Some(subtitles) = info.get("subtitles").and_then(Value::as_object) {
        for (code, formats_val) in subtitles {
            if let Some(formats) = formats_val.as_array() {
                if let Some(first) = formats.first() {
                    let name = first
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or(code.as_str())
                        .to_string();
                    tracks.insert(
                        code.clone(),
                        SubtitleTrack {
                            code: code.clone(),
                            name,
                            auto: false,
                        },
                    );
                }
            }
        }
    }

    if let Some(auto_caps) = info.get("automatic_captions").and_then(Value::as_object) {
        for (code, formats_val) in auto_caps {
            if tracks.contains_key(code) {
                continue;
            }
            if let Some(formats) = formats_val.as_array() {
                let real_formats: Vec<&Value> =
                    formats.iter().filter(|f| is_real_caption(f)).collect();
                if let Some(first) = real_formats.first() {
                    let name = first
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or(code.as_str())
                        .to_string();
                    tracks.insert(
                        code.clone(),
                        SubtitleTrack {
                            code: code.clone(),
                            name,
                            auto: true,
                        },
                    );
                }
            }
        }
    }

    let mut languages: Vec<SubtitleTrack> = tracks.into_values().collect();
    languages.sort_by(|a, b| a.auto.cmp(&b.auto).then_with(|| a.code.cmp(&b.code)));

    Ok(VideoSubtitles {
        title,
        channel,
        duration,
        thumbnail,
        webpage_url,
        languages,
    })
}

async fn run_ytdlp(ytdlp_exe: &Path, args: &[&str]) -> Result<std::process::Output, String> {
    let mut cmd = Command::new(ytdlp_exe);
    cmd.args(args);
    #[cfg(target_os = "windows")]
    {
        cmd.creation_flags(0x08000000);
    }
    cmd.output()
        .await
        .map_err(|e| format!("Failed to run yt-dlp: {e}"))
}

pub async fn get_subtitle_options(ytdlp_exe: &Path, url: &str) -> Result<VideoSubtitles, String> {
    let output = run_ytdlp(ytdlp_exe, &["--dump-json", "--skip-download", url]).await?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(last_error_line(&stderr));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_subtitle_options(&stdout)
}

fn collect_srt_files(dir: &Path) -> HashSet<PathBuf> {
    let mut set = HashSet::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("srt") {
                set.insert(path);
            }
        }
    }
    set
}

fn modified_time(path: &Path) -> SystemTime {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .unwrap_or(SystemTime::UNIX_EPOCH)
}

pub fn find_downloaded_srt(
    before: &HashSet<PathBuf>,
    out_dir: &Path,
    lang_code: &str,
) -> Option<PathBuf> {
    let after = collect_srt_files(out_dir);
    let new_files: Vec<PathBuf> = after.difference(before).cloned().collect();
    if !new_files.is_empty() {
        return new_files.into_iter().max_by_key(|p| modified_time(p));
    }

    let suffix = format!(".{lang_code}.srt");
    let mut matches: Vec<PathBuf> = after
        .into_iter()
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|name| name.ends_with(&suffix))
        })
        .collect();
    matches.sort_by_key(|p| modified_time(p));
    matches.pop()
}

pub fn read_subtitle_preview(path: &Path, max_bytes: usize) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("Couldn't read subtitle file: {e}"))?;
    let slice = if bytes.len() > max_bytes {
        &bytes[..max_bytes]
    } else {
        &bytes[..]
    };
    let mut text = String::from_utf8_lossy(slice).to_string();
    if bytes.len() > max_bytes {
        text.push_str("\n\n... (preview truncated)");
    }
    Ok(text)
}

pub async fn download_subtitle(
    ytdlp_exe: &Path,
    url: &str,
    lang_code: &str,
    auto: bool,
    out_dir: &Path,
) -> Result<Option<DownloadResult>, String> {
    std::fs::create_dir_all(out_dir)
        .map_err(|e| format!("Couldn't use that folder: {e}"))?;

    let before = collect_srt_files(out_dir);
    let template = out_dir.join("%(title)s.%(ext)s");
    let template_str = template.to_string_lossy().to_string();
    let sub_flag = if auto { "--write-auto-subs" } else { "--write-subs" };

    let output = run_ytdlp(
        ytdlp_exe,
        &[
            sub_flag,
            "--sub-langs",
            lang_code,
            "--sub-format",
            "srt",
            "--skip-download",
            "-o",
            &template_str,
            url,
        ],
    )
    .await?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(last_error_line(&stderr));
    }

    let Some(path) = find_downloaded_srt(&before, out_dir, lang_code) else {
        return Ok(None);
    };

    let filename = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("subtitle.srt")
        .to_string();
    let size_bytes = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    let preview = read_subtitle_preview(&path, 8192).unwrap_or_default();

    Ok(Some(DownloadResult {
        path: path.display().to_string(),
        filename,
        size_bytes,
        preview,
    }))
}

pub fn list_saved_subtitles(dir: &Path) -> Result<Vec<SavedSubtitle>, String> {
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let entries =
        std::fs::read_dir(dir).map_err(|e| format!("Couldn't read folder: {e}"))?;

    let mut items = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("srt") {
            let meta = std::fs::metadata(&path).ok();
            let size_bytes = meta.as_ref().map(|m| m.len()).unwrap_or(0);
            let modified_epoch_secs = meta
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let filename = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("subtitle.srt")
                .to_string();
            items.push(SavedSubtitle {
                filename,
                path: path.display().to_string(),
                size_bytes,
                modified_epoch_secs,
            });
        }
    }

    items.sort_by(|a, b| b.modified_epoch_secs.cmp(&a.modified_epoch_secs));
    Ok(items)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::fs::File;
    use std::thread;
    use std::time::Duration;

    fn temp_test_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "subgrabber_rust_test_{}_{}_{}",
            name,
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn youtube_url_validation() {
        assert!(is_youtube_url("https://www.youtube.com/watch?v=abc123"));
        assert!(is_youtube_url("https://youtu.be/abc123"));
        assert!(!is_youtube_url("https://vimeo.com/12345"));
        assert!(!is_youtube_url("not a url"));
    }

    #[test]
    fn real_caption_filtering() {
        assert!(is_real_caption(&json!({"url": "https://example.com/caption.vtt"})));
        assert!(!is_real_caption(&json!({"url": "https://example.com/caption.vtt?tlang=fr"})));
        assert!(is_real_caption(&json!({})));
    }

    #[test]
    fn friendly_error_mapping() {
        assert_eq!(friendly_error("ERROR: Private video"), "That video is private.");
        assert_eq!(
            friendly_error("Video unavailable"),
            "That video is unavailable (deleted or region-blocked)."
        );
        assert_eq!(
            friendly_error("Sign in to confirm your age"),
            "That video requires sign-in and can't be downloaded here."
        );
        assert!(friendly_error("No module named yt_dlp").contains("yt-dlp"));
        assert_eq!(
            friendly_error("some unexpected failure"),
            "Couldn't read that video. Double-check the URL and try again."
        );
    }

    #[test]
    fn parse_manual_and_automatic_subtitles() {
        let info = json!({
            "title": "Test Video",
            "channel": "Meke Dev",
            "duration": 125,
            "subtitles": {
                "en": [{"name": "English", "url": "https://example.com/en.vtt"}]
            },
            "automatic_captions": {
                "en": [{"name": "English (auto)", "url": "https://example.com/en-auto.vtt"}],
                "fr": [{"name": "French", "url": "https://example.com/fr.vtt"}],
                "de": [{"name": "German (translated)", "url": "https://example.com/de.vtt?tlang=de"}]
            }
        });

        let parsed = parse_subtitle_options(&info.to_string()).unwrap();
        assert_eq!(parsed.title, "Test Video");
        assert_eq!(parsed.channel.as_deref(), Some("Meke Dev"));
        assert_eq!(parsed.duration.as_deref(), Some("2:05"));
        assert_eq!(
            parsed.languages,
            vec![
                SubtitleTrack {
                    code: "en".to_string(),
                    name: "English".to_string(),
                    auto: false,
                },
                SubtitleTrack {
                    code: "fr".to_string(),
                    name: "French".to_string(),
                    auto: true,
                },
            ]
        );
    }

    #[test]
    fn parse_no_subtitles_available() {
        let parsed = parse_subtitle_options(r#"{"title":"No Subs"}"#).unwrap();
        assert_eq!(parsed.title, "No Subs");
        assert!(parsed.languages.is_empty());
    }

    #[test]
    fn find_downloaded_srt_detects_new_and_overwritten_files() {
        let dir = temp_test_dir("srt_detect");
        let existing = dir.join("Existing Video.en.srt");
        File::create(&existing).unwrap();
        let before = collect_srt_files(&dir);

        let fallback = find_downloaded_srt(&before, &dir, "en").unwrap();
        assert_eq!(fallback.file_name().unwrap(), "Existing Video.en.srt");

        thread::sleep(Duration::from_millis(20));
        let created = dir.join("New Video.en.srt");
        File::create(&created).unwrap();
        let detected = find_downloaded_srt(&before, &dir, "en").unwrap();
        assert_eq!(detected.file_name().unwrap(), "New Video.en.srt");

        assert!(find_downloaded_srt(&collect_srt_files(&dir), &dir, "es").is_none());

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
