//! Unduhan langsung (DownloadOnly) dengan progres di TUI.
//!
//! Link download dari source sering berupa halaman perantara (Gofile, Mega,
//! Mediafire, Gofile, safelink) yang tidak bisa diunduh sebagai file; yang seperti
//! itu dibuka di browser. Yang benar-benar file (mis. Pixeldrain `/api/file/`,
//! URL berakhiran `.mkv/.mp4/.zip/.rar`) diunduh di dalam aplikasi.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use reqwest::header::{HeaderValue, REFERER, USER_AGENT};
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc::UnboundedSender;
use url::Url;

use crate::ui::AppEvent;

/// Seberapa sering progres dikirim ke UI.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(150);

#[derive(Debug, Clone)]
pub struct DownloadSpec {
    pub url: Url,
    pub dest: PathBuf,
    pub referer: Option<Url>,
    pub user_agent: Option<String>,
}

/// Apakah URL menunjuk langsung ke file (bisa diunduh tanpa halaman perantara)?
/// Normalisasi URL halaman menjadi URL file langsung bila polanya diketahui.
///
/// Pixeldrain: `https://pixeldrain.com/u/<id>` → `https://pixeldrain.com/api/file/<id>`.
pub fn direct_file_url(url: &Url) -> Option<Url> {
    let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
    if host.contains("pixeldrain") {
        let segments: Vec<&str> = url.path_segments().map(|s| s.collect()).unwrap_or_default();
        if let Some(index) = segments.iter().position(|segment| *segment == "u") {
            if let Some(id) = segments.get(index + 1) {
                if !id.is_empty() {
                    return Url::parse(&format!("https://pixeldrain.com/api/file/{id}")).ok();
                }
            }
        }
    }
    None
}

pub fn is_direct_file(url: &Url) -> bool {
    let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
    let path = url.path().to_ascii_lowercase();
    if host.contains("pixeldrain") && path.starts_with("/api/file/") {
        return true;
    }
    if host.starts_with("dl.") || host.starts_with("download.") {
        return true;
    }
    let direct_ext = [".mp4", ".mkv", ".webm", ".zip", ".rar", ".7z", ".avi"];
    direct_ext.iter().any(|ext| path.ends_with(ext))
}

/// Nama file aman dari judul/episode/label.
pub fn safe_file_name(raw: &str, fallback_ext: &str) -> String {
    let mut name: String = raw
        .chars()
        .map(|ch| match ch {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '-',
            ch if ch.is_control() => ' ',
            ch => ch,
        })
        .collect();
    name = name.split_whitespace().collect::<Vec<_>>().join(" ");
    let name = name.trim_matches([' ', '.', '-']).to_string();
    let mut name = if name.is_empty() {
        "unduhan".to_string()
    } else {
        name
    };
    if name.len() > 120 {
        name.truncate(120);
        name = name.trim_end().to_string();
    }
    if !name.contains('.') {
        name.push('.');
        name.push_str(fallback_ext);
    }
    name
}

/// Format byte menjadi "12.3 MB".
pub fn format_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let value = bytes as f64;
    if value >= GB {
        format!("{:.2} GB", value / GB)
    } else if value >= MB {
        format!("{:.1} MB", value / MB)
    } else if value >= KB {
        format!("{:.0} KB", value / KB)
    } else {
        format!("{bytes} B")
    }
}

/// Unduh `spec` sambil mengirim progres (AppEvent) ke UI.
pub async fn run(job: u64, spec: DownloadSpec, tx: UnboundedSender<AppEvent>) {
    match download(job, &spec, &tx).await {
        Ok(path) => {
            let _ = tx.send(AppEvent::DownloadDone { job, path });
        }
        Err(message) => {
            let _ = tx.send(AppEvent::DownloadFailed { job, message });
        }
    }
}

async fn download(
    job: u64,
    spec: &DownloadSpec,
    tx: &UnboundedSender<AppEvent>,
) -> Result<PathBuf, String> {
    // Provider TLS harus terpasang sebelum client pertama (fitur
    // rustls-no-provider di reqwest).
    crate::http::ensure_tls_provider();
    let client = reqwest::Client::builder()
        .use_preconfigured_tls(crate::http::tls_config())
        .build()
        .map_err(|err| format!("gagal membuat client: {err}"))?;
    let mut request = client.get(spec.url.clone());
    if let Some(user_agent) = &spec.user_agent {
        if let Ok(value) = HeaderValue::from_str(user_agent) {
            request = request.header(USER_AGENT, value);
        }
    }
    if let Some(referer) = &spec.referer {
        if let Ok(value) = HeaderValue::from_str(referer.as_str()) {
            request = request.header(REFERER, value);
        }
    }

    let mut response = request
        .send()
        .await
        .map_err(|err| format!("permintaan gagal: {err}"))?;
    if !response.status().is_success() {
        return Err(format!("server menolak (HTTP {})", response.status()));
    }
    let total = response.content_length();

    if let Some(parent) = spec.dest.parent() {
        if let Err(err) = tokio::fs::create_dir_all(parent).await {
            return Err(format!("gagal membuat folder {}: {err}", parent.display()));
        }
    }
    let mut file = tokio::fs::File::create(&spec.dest)
        .await
        .map_err(|err| format!("gagal membuat file {}: {err}", spec.dest.display()))?;

    let _ = tx.send(AppEvent::DownloadStarted {
        job,
        path: spec.dest.clone(),
        total,
    });

    let mut downloaded = 0_u64;
    let mut last_emit = Instant::now();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|err| format!("gagal membaca data: {err}"))?
    {
        file.write_all(&chunk)
            .await
            .map_err(|err| format!("gagal menulis file: {err}"))?;
        downloaded += chunk.len() as u64;
        if last_emit.elapsed() >= PROGRESS_INTERVAL {
            last_emit = Instant::now();
            let _ = tx.send(AppEvent::DownloadProgress {
                job,
                downloaded,
                total,
            });
        }
    }
    file.flush()
        .await
        .map_err(|err| format!("gagal flush file: {err}"))?;
    let _ = tx.send(AppEvent::DownloadProgress {
        job,
        downloaded,
        total: total.or(Some(downloaded)),
    });
    Ok(spec.dest.clone())
}

/// Jalur relatif untuk ditampilkan (mengganti home dir dengan `~`).
pub fn display_path(path: &Path) -> String {
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        if let Ok(rest) = path.strip_prefix(&home) {
            return format!("~/{}", rest.display());
        }
    }
    path.display().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(raw: &str) -> Url {
        Url::parse(raw).unwrap()
    }

    #[test]
    fn direct_file_detection() {
        assert!(is_direct_file(&url(
            "https://pixeldrain.com/api/file/wTkHbvZw"
        )));
        assert!(is_direct_file(&url("https://dl.example.com/a.mkv")));
        assert!(!is_direct_file(&url("https://gofile.io/d/abc")));
        assert!(!is_direct_file(&url("https://mediafire.com/file/x")));
        assert!(!is_direct_file(&url("https://acefile.co/f/x")));
    }

    #[test]
    fn pixeldrain_page_is_normalised_to_api_file() {
        let direct = direct_file_url(&url("https://pixeldrain.com/u/MzVGWHTe")).unwrap();
        assert_eq!(direct.as_str(), "https://pixeldrain.com/api/file/MzVGWHTe");
        assert!(is_direct_file(&direct));
        assert!(direct_file_url(&url("https://gofile.io/d/abc")).is_none());
    }

    #[test]
    fn safe_names_are_sanitised() {
        assert_eq!(safe_file_name("a/b:c?d", "mkv"), "a-b-c-d.mkv");
        assert_eq!(safe_file_name("  ", "mp4"), "unduhan.mp4");
        assert_eq!(safe_file_name("Episode 1.mkv", "mkv"), "Episode 1.mkv");
    }

    #[test]
    fn byte_formatting() {
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(2048), "2 KB");
        assert_eq!(format_bytes(5 * 1024 * 1024), "5.0 MB");
    }
}
