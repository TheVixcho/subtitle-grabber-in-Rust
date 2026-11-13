use serde::Serialize;
use std::path::{Path, PathBuf};
use tokio::process::Command;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum YtdlpSource {
    System,
    Managed,
    Missing,
}

#[derive(Debug, Clone, Serialize)]
pub struct YtdlpStatus {
    pub available: bool,
    pub source: YtdlpSource,
    pub version: Option<String>,
    pub path: Option<String>,
}

pub fn managed_binary_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "yt-dlp.exe"
    } else {
        "yt-dlp"
    }
}

pub fn official_download_url() -> &'static str {
    if cfg!(target_os = "windows") {
        "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe"
    } else if cfg!(target_os = "macos") {
        "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp_macos"
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp_linux"
    } else if cfg!(all(target_os = "linux", target_arch = "aarch64")) {
        "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp_linux_aarch64"
    } else {
        "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp"
    }
}

pub fn managed_binary_path() -> PathBuf {
    let base = dirs::data_local_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("subtitle-grabber-in-rust")
        .join(managed_binary_name())
}

async fn probe_version(exe: &Path) -> Option<String> {
    let mut cmd = Command::new(exe);
    cmd.arg("--version");
    #[cfg(target_os = "windows")]
    {
        cmd.creation_flags(0x08000000);
    }
    let output = cmd.output().await.ok()?;
    if !output.status.success() {
        return None;
    }
    let ver = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if ver.is_empty() {
        None
    } else {
        Some(ver)
    }
}

pub async fn resolve_ytdlp() -> Option<(PathBuf, YtdlpSource, String)> {
    let system_cmd = PathBuf::from(managed_binary_name());
    if let Some(version) = probe_version(&system_cmd).await {
        return Some((system_cmd, YtdlpSource::System, version));
    }

    let managed = managed_binary_path();
    if managed.is_file() {
        if let Some(version) = probe_version(&managed).await {
            return Some((managed, YtdlpSource::Managed, version));
        }
    }

    None
}

pub async fn check_status() -> YtdlpStatus {
    match resolve_ytdlp().await {
        Some((path, source, version)) => YtdlpStatus {
            available: true,
            source,
            version: Some(version),
            path: Some(path.display().to_string()),
        },
        None => YtdlpStatus {
            available: false,
            source: YtdlpSource::Missing,
            version: None,
            path: None,
        },
    }
}

pub async fn download_managed_ytdlp() -> Result<YtdlpStatus, String> {
    let target = managed_binary_path();
    let parent = target
        .parent()
        .ok_or_else(|| "Invalid managed yt-dlp path.".to_string())?;
    tokio::fs::create_dir_all(parent)
        .await
        .map_err(|e| format!("Failed to create app data folder: {e}"))?;

    let url = official_download_url();
    let client = reqwest::Client::builder()
        .user_agent("subtitle-grabber-in-rust/0.2.0")
        .build()
        .map_err(|e| format!("Failed to initialize HTTP client: {e}"))?;

    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("Failed to download yt-dlp: {e}"))?;

    if !response.status().is_success() {
        return Err(format!(
            "Failed to download yt-dlp (HTTP {}).",
            response.status()
        ));
    }

    let bytes = response
        .bytes()
        .await
        .map_err(|e| format!("Failed while reading yt-dlp download: {e}"))?;

    let tmp_path = parent.join(format!("{}.tmp", managed_binary_name()));
    tokio::fs::write(&tmp_path, &bytes)
        .await
        .map_err(|e| format!("Failed to write yt-dlp binary: {e}"))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o755);
        tokio::fs::set_permissions(&tmp_path, perms)
            .await
            .map_err(|e| format!("Failed to mark yt-dlp executable: {e}"))?;
    }

    tokio::fs::rename(&tmp_path, &target)
        .await
        .map_err(|e| format!("Failed to finalize yt-dlp binary: {e}"))?;

    let version = probe_version(&target).await.ok_or_else(|| {
        "Downloaded yt-dlp binary failed verification (--version).".to_string()
    })?;

    Ok(YtdlpStatus {
        available: true,
        source: YtdlpSource::Managed,
        version: Some(version),
        path: Some(target.display().to_string()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn managed_path_ends_with_expected_binary() {
        let path = managed_binary_path();
        assert_eq!(
            path.file_name().and_then(|s| s.to_str()),
            Some(managed_binary_name())
        );
    }

    #[test]
    fn official_url_uses_https_github_releases() {
        let url = official_download_url();
        assert!(url.starts_with("https://github.com/yt-dlp/yt-dlp/releases/latest/download/"));
    }
}
