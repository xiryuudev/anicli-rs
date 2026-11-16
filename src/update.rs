//! Cek & pasang update dari GitHub Releases.
//!
//! Alur: baca rilis terbaru (`/releases/latest`) → bandingkan versi semver →
//! (opsional) unduh aset sesuai platform → ganti binary yang sedang dipakai.
//!
//! Aset rilis memakai nama **tanpa versi** (`anicli-x86_64-linux`,
//! `anicli-x86_64-windows.exe`, `anicli-aarch64-android`) sehingga URL
//! `releases/latest/download/...` selalu menunjuk berkas yang benar.

use std::path::{Path, PathBuf};

use reqwest::header::{ACCEPT, USER_AGENT};
use serde::Deserialize;
use url::Url;

use crate::error::{SourceError, SourceResult};

const RELEASES_URL: &str = "https://api.github.com/repos/xiryuudev/anicli-rs/releases/latest";
const DOWNLOAD_BASE: &str = "https://github.com/xiryuudev/anicli-rs/releases/latest/download";

/// Platform yang dikenali untuk pemilihan aset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    LinuxX86_64,
    WindowsX86_64,
    AndroidAarch64,
}

impl Platform {
    /// Deteksi platform saat ini (Termux dianggap Android).
    pub fn current() -> Option<Self> {
        if crate::player::android::is_termux() {
            return Some(Self::AndroidAarch64);
        }
        match (std::env::consts::OS, std::env::consts::ARCH) {
            ("linux", "x86_64") => Some(Self::LinuxX86_64),
            ("windows", "x86_64") => Some(Self::WindowsX86_64),
            _ => None,
        }
    }

    /// Nama aset rilis untuk binary utama.
    pub fn asset(self) -> &'static str {
        match self {
            Self::LinuxX86_64 => "anicli-x86_64-linux",
            Self::WindowsX86_64 => "anicli-x86_64-windows.exe",
            Self::AndroidAarch64 => "anicli-aarch64-android",
        }
    }

    pub fn describe(self) -> &'static str {
        match self {
            Self::LinuxX86_64 => "Linux x86_64",
            Self::WindowsX86_64 => "Windows x86_64",
            Self::AndroidAarch64 => "Android/Termux aarch64",
        }
    }

    /// Nama file binary yang diharapkan setelah update.
    pub fn binary_name(self) -> &'static str {
        match self {
            Self::WindowsX86_64 => "anicli.exe",
            _ => "anicli",
        }
    }
}

/// Info rilis terbaru.
#[derive(Debug, Clone)]
pub struct ReleaseInfo {
    /// Versi tanpa awalan `v` (mis. `0.5.0`).
    pub version: String,
    pub tag: String,
    pub html_url: String,
    pub notes: String,
}

#[derive(Debug, Deserialize)]
struct RawRelease {
    // GitHub mengirim `null` untuk field yang kosong (mis. `body`), jadi semuanya
    // opsional lalu di-default manual — bukan `#[serde(default)]` pada `String`.
    #[serde(default)]
    tag_name: Option<String>,
    #[serde(default)]
    html_url: Option<String>,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
}

/// Ambil rilis stabil terbaru dan bandingkan dengan versi berjalan.
pub async fn latest_release(http: &crate::http::HttpClient) -> SourceResult<ReleaseInfo> {
    let url = Url::parse(RELEASES_URL)
        .map_err(|err| SourceError::parse("update", "url", "releases", err.to_string()))?;
    let body = http.get_text("update", &url, None).await?;
    let raw: RawRelease = serde_json::from_str(&body).map_err(|err| {
        SourceError::parse(
            "update",
            "releases",
            "json",
            format!("respons rilis tidak valid: {err}"),
        )
    })?;
    if raw.draft || raw.prerelease {
        return Err(SourceError::parse(
            "update",
            "releases",
            "tag",
            "rilis terbaru bertanda draft/prerelease",
        ));
    }
    let tag_name = raw.tag_name.clone().unwrap_or_default();
    let version = tag_name
        .trim()
        .trim_start_matches('v')
        .trim_start_matches('V')
        .to_string();
    if version.is_empty() {
        return Err(SourceError::parse(
            "update",
            "releases",
            "tag",
            "tag versi kosong",
        ));
    }
    let tag = if tag_name.trim().is_empty() {
        format!("v{version}")
    } else {
        tag_name.trim().to_string()
    };
    Ok(ReleaseInfo {
        version,
        tag,
        html_url: raw.html_url.unwrap_or_default(),
        notes: raw.body.unwrap_or_default(),
    })
}

/// Bandingkan versi semver sederhana (`major.minor.patch`, prerelease diabaikan).
///
/// Mengembalikan `true` bila `latest` lebih baru dari `current`.
pub fn is_newer(current: &str, latest: &str) -> bool {
    fn parse(raw: &str) -> (u64, u64, u64) {
        let core = raw.split(['-', '+']).next().unwrap_or(raw);
        let mut parts = core.split('.').map(|part| part.parse::<u64>().unwrap_or(0));
        (
            parts.next().unwrap_or(0),
            parts.next().unwrap_or(0),
            parts.next().unwrap_or(0),
        )
    }
    parse(latest) > parse(current)
}

/// URL unduh aset untuk platform ini.
pub fn asset_url(platform: Platform) -> SourceResult<Url> {
    Url::parse(&format!("{DOWNLOAD_BASE}/{}", platform.asset())).map_err(|err| {
        SourceError::parse(
            "update",
            "url",
            "asset",
            format!("URL aset tidak valid: {err}"),
        )
    })
}

/// Unduh aset dan tulis ke `dest` (boleh file sementara).
pub async fn download_asset(platform: Platform, dest: &Path) -> SourceResult<u64> {
    let url = asset_url(platform)?;
    let client = crate::http::client_builder()
        .user_agent(format!("anicli-rs/{}", crate::VERSION))
        .build()
        .map_err(|err| SourceError::app(format!("gagal membuat client update: {err}")))?;

    let response = client
        .get(url.clone())
        .header(USER_AGENT, format!("anicli-rs/{}", crate::VERSION))
        .header(ACCEPT, "application/octet-stream")
        .send()
        .await
        .map_err(|err| SourceError::Network {
            site: "update",
            url: url.to_string(),
            detail: err.to_string(),
        })?;
    if !response.status().is_success() {
        return Err(SourceError::Http {
            site: "update",
            url: url.to_string(),
            status: response.status().as_u16(),
        });
    }
    let bytes = response.bytes().await.map_err(|err| SourceError::Network {
        site: "update",
        url: url.to_string(),
        detail: err.to_string(),
    })?;
    if bytes.len() < 1024 * 64 {
        return Err(SourceError::parse(
            "update",
            "download",
            "size",
            "berkas hasil unduhan terlalu kecil (mungkin halaman error)",
        ));
    }
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|err| {
            SourceError::app(format!("gagal membuat folder {}: {err}", parent.display()))
        })?;
    }
    std::fs::write(dest, &bytes)
        .map_err(|err| SourceError::app(format!("gagal menulis {}: {err}", dest.display())))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(dest, std::fs::Permissions::from_mode(0o755));
    }
    Ok(bytes.len() as u64)
}

/// Lokasi binary yang sedang berjalan (target penggantian).
pub fn current_binary() -> SourceResult<PathBuf> {
    std::env::current_exe()
        .map_err(|err| SourceError::app(format!("tidak bisa mengetahui path binary: {err}")))
}

/// Apakah path bisa ditulis (update perlu izin di folder binary).
pub fn is_writable(path: &Path) -> bool {
    let Some(parent) = path.parent() else {
        return false;
    };
    let probe = parent.join(".anicli-write-probe");
    match std::fs::write(&probe, b"probe") {
        Ok(()) => {
            let _ = std::fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

/// Pasang binary baru: ganti file yang berjalan.
///
/// Urutan: tulis `.new` di folder yang sama → rename (POSIX menimpa; Windows
/// butuh hapus tujuan dulu, dan file yang sedang berjalan tidak bisa ditimpa —
/// pada Windows fungsi ini memakai rencana `pending` dan mengembalikan instruksi).
pub fn install_binary(new_binary: &Path, target: &Path) -> SourceResult<InstallOutcome> {
    #[cfg(windows)]
    {
        // Binary yang sedang berjalan terkunci di Windows.
        let pending = target.with_extension("new.exe");
        std::fs::copy(new_binary, &pending).map_err(|err| {
            SourceError::app(format!("gagal menyalin ke {}: {err}", pending.display()))
        })?;
        let _ = std::fs::remove_file(new_binary);
        Ok(InstallOutcome::PendingWindows { path: pending })
    }
    #[cfg(not(windows))]
    {
        let staged = target.with_extension("new");
        std::fs::copy(new_binary, &staged).map_err(|err| {
            SourceError::app(format!("gagal menyiapkan {}: {err}", staged.display()))
        })?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&staged, std::fs::Permissions::from_mode(0o755)).map_err(
                |err| SourceError::app(format!("gagal chmod {}: {err}", staged.display())),
            )?;
        }
        std::fs::rename(&staged, target).map_err(|err| {
            SourceError::app(format!(
                "gagal mengganti {}: {err}. Jalankan dengan izin tulis (mis. sudo) atau \
                 letakkan binary di folder milik user.",
                target.display()
            ))
        })?;
        let _ = std::fs::remove_file(new_binary);
        Ok(InstallOutcome::Installed {
            path: target.to_path_buf(),
        })
    }
}

/// Hasil pemasangan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallOutcome {
    Installed {
        path: PathBuf,
    },
    /// Windows: binary baru disiapkan; pengguna harus mengganti manual.
    PendingWindows {
        path: PathBuf,
    },
}

/// Cache hasil pengecekan terakhir (agar tidak memanggil API setiap start).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CheckCache {
    /// Waktu pengecekan terakhir (detik epoch).
    pub checked_at: u64,
    pub latest: String,
}

impl CheckCache {
    pub fn path(data_dir: &Path) -> PathBuf {
        data_dir.join("update-check.json")
    }

    pub fn load(data_dir: &Path) -> Option<Self> {
        let raw = std::fs::read_to_string(Self::path(data_dir)).ok()?;
        serde_json::from_str(&raw).ok()
    }

    pub fn save(&self, data_dir: &Path) {
        let path = Self::path(data_dir);
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(raw) = serde_json::to_string(self) {
            let _ = crate::config::write_atomic(&path, &raw);
        }
    }

    /// Sudah lewat interval (jam)?
    pub fn is_fresh(&self, interval_hours: u64) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_secs())
            .unwrap_or(0);
        let interval = interval_hours.max(1).saturating_mul(3600);
        now.saturating_sub(self.checked_at) < interval
    }
}

/// Perintah penulisan untuk subcommand `update` (dipakai UI & CLI).
pub fn update_hint(tag: &str, platform: Option<Platform>) -> String {
    match platform {
        Some(Platform::AndroidAarch64) => format!(
            "anicli update            # unduh & pasang otomatis\n\
             manual: curl -L -o \"$PREFIX/bin/anicli\" \\\n  \
             https://github.com/xiryuudev/anicli-rs/releases/download/{tag}/anicli-aarch64-android"
        ),
        Some(Platform::WindowsX86_64) => format!(
            "anicli update            # unduh; ganti binary setelah keluar\n\
             manual: unduh anicli-x86_64-windows.exe dari rilis {tag}"
        ),
        _ => format!(
            "anicli update            # unduh & pasang otomatis\n\
             manual: unduh anicli-x86_64-linux dari rilis {tag}"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_comparison_handles_semver() {
        assert!(is_newer("0.5.0", "0.6.0"));
        assert!(is_newer("0.5.0", "0.5.1"));
        assert!(is_newer("0.5.9", "0.6.0"));
        assert!(!is_newer("1.0.0", "1.0.0"));
        assert!(!is_newer("0.5.0", "0.5.0"));
        assert!(!is_newer("0.6.0", "0.5.0"));
        assert!(!is_newer("0.5.0", "0.5.0-beta"));
        assert!(is_newer("0.9.9", "1.0.0"));
    }

    #[test]
    fn asset_names_per_platform() {
        assert_eq!(Platform::LinuxX86_64.asset(), "anicli-x86_64-linux");
        assert_eq!(Platform::WindowsX86_64.asset(), "anicli-x86_64-windows.exe");
        assert_eq!(Platform::AndroidAarch64.asset(), "anicli-aarch64-android");
        assert_eq!(Platform::WindowsX86_64.binary_name(), "anicli.exe");
        assert_eq!(Platform::LinuxX86_64.binary_name(), "anicli");
    }

    #[test]
    fn hint_mentions_platform_specific_command() {
        let android = update_hint("v0.6.0", Some(Platform::AndroidAarch64));
        assert!(android.contains("$PREFIX/bin/anicli"));
        let windows = update_hint("v0.6.0", Some(Platform::WindowsX86_64));
        assert!(windows.contains("anicli-x86_64-windows.exe"));
    }
}
