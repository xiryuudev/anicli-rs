//! Resolusi lokasi config/data/log per platform (DESIGN §14.1).
//!
//! Linux   : `$XDG_CONFIG_HOME/anicli-rs/config.toml`, `$XDG_DATA_HOME/anicli-rs/`
//! Windows : `%APPDATA%\anicli-rs\config.toml`, `%LOCALAPPDATA%\anicli-rs\`
//! macOS   : `~/Library/Application Support/anicli-rs/`
//!
//! Presedensi (DESIGN §14.3): flag CLI > environment (`ANICLI_CONFIG`,
//! `ANICLI_DATA_DIR`) > default platform. File modul ini **tidak** membuat direktori;
//! pemanggil memanggil [`Paths::ensure`] saat benar-benar mau menulis.

use std::env;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use directories::BaseDirs;

/// Semua path yang dipakai aplikasi. Dibuat sekali di startup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paths {
    /// File config (`config.toml`), bukan direktori.
    pub config_file: PathBuf,
    /// Direktori data (riwayat, bookmark, cache, log).
    pub data_dir: PathBuf,
}

impl Paths {
    /// Tentukan path dari default platform + override CLI/env.
    ///
    /// `config_override` dan `data_override` biasanya berasal dari `--config` /
    /// `--data-dir` (yang sudah punya fallback environment lewat `clap`). Kalau
    /// `None`, environment `ANICLI_CONFIG` / `ANICLI_DATA_DIR` tetap dihormati agar
    /// modul ini juga berguna untuk test dan binary lain.
    pub fn resolve(
        config_override: Option<PathBuf>,
        data_override: Option<PathBuf>,
    ) -> Result<Self> {
        let config_file = match config_override.or_else(|| env_path("ANICLI_CONFIG")) {
            Some(path) => expand_tilde(&path)?,
            None => default_config_file()?,
        };
        let data_dir = match data_override.or_else(|| env_path("ANICLI_DATA_DIR")) {
            Some(path) => expand_tilde(&path)?,
            None => default_data_dir()?,
        };

        Ok(Self {
            config_file,
            data_dir,
        })
    }

    /// Direktori log: `<data>/logs`.
    pub fn log_dir(&self) -> PathBuf {
        self.data_dir.join("logs")
    }

    /// Direktori config (induk dari `config.toml`).
    pub fn config_dir(&self) -> PathBuf {
        self.config_file
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."))
    }

    pub fn history_file(&self) -> PathBuf {
        self.data_dir.join("history.json")
    }

    pub fn bookmarks_file(&self) -> PathBuf {
        self.data_dir.join("bookmarks.json")
    }

    pub fn mirror_cache_file(&self) -> PathBuf {
        self.data_dir.join("mirror-cache.json")
    }

    /// Buat direktori config, data, dan log bila belum ada.
    pub fn ensure(&self) -> Result<()> {
        for dir in [self.config_dir(), self.data_dir.clone(), self.log_dir()] {
            std::fs::create_dir_all(&dir)
                .with_context(|| format!("gagal membuat direktori {}", dir.display()))?;
        }
        Ok(())
    }
}

/// `$XDG_CONFIG_HOME/anicli-rs/config.toml` (Windows: `%APPDATA%\anicli-rs\`).
pub fn default_config_file() -> Result<PathBuf> {
    let base = base_dirs()?;
    Ok(base.config_dir().join("anicli-rs").join("config.toml"))
}

/// `$XDG_DATA_HOME/anicli-rs` (Windows: `%LOCALAPPDATA%\anicli-rs`).
pub fn default_data_dir() -> Result<PathBuf> {
    let base = base_dirs()?;
    Ok(base.data_local_dir().join("anicli-rs"))
}

fn base_dirs() -> Result<BaseDirs> {
    BaseDirs::new()
        .context("tidak bisa menentukan direktori home pengguna (HOME/APPDATA tidak di-set?)")
}

fn env_path(key: &str) -> Option<PathBuf> {
    env::var_os(key)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

/// Dukung `~/foo` dan `~` karena user sering menulis path seperti itu.
fn expand_tilde(path: &Path) -> Result<PathBuf> {
    let raw = path.to_string_lossy();
    if raw == "~" || raw.starts_with("~/") || raw.starts_with("~\\") {
        let home = env::var_os("HOME")
            .or_else(|| env::var_os("USERPROFILE"))
            .context("tidak bisa memakai `~`: HOME/USERPROFILE tidak di-set")?;
        let mut expanded = PathBuf::from(home);
        let rest = raw.trim_start_matches('~').trim_start_matches(['/', '\\']);
        if !rest.is_empty() {
            expanded.push(rest);
        }
        Ok(expanded)
    } else {
        Ok(path.to_path_buf())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overrides_win_over_defaults() {
        let paths = Paths::resolve(
            Some(PathBuf::from("/tmp/anicli-test/config.toml")),
            Some(PathBuf::from("/tmp/anicli-test/data")),
        )
        .unwrap();
        assert_eq!(
            paths.config_file,
            PathBuf::from("/tmp/anicli-test/config.toml")
        );
        assert_eq!(paths.config_dir(), PathBuf::from("/tmp/anicli-test"));
        assert_eq!(paths.data_dir, PathBuf::from("/tmp/anicli-test/data"));
        assert_eq!(paths.log_dir(), PathBuf::from("/tmp/anicli-test/data/logs"));
        assert_eq!(
            paths.history_file(),
            PathBuf::from("/tmp/anicli-test/data/history.json")
        );
    }

    #[test]
    fn tilde_is_expanded() {
        let home = env::var_os("HOME").or_else(|| env::var_os("USERPROFILE"));
        if home.is_none() {
            return; // lingkungan tanpa HOME: test tidak relevan
        }
        let paths = Paths::resolve(
            Some(PathBuf::from("~/anicli-rs-test/config.toml")),
            Some(PathBuf::from("~/anicli-rs-test/data")),
        )
        .unwrap();
        assert!(!paths.config_file.to_string_lossy().starts_with('~'));
        assert!(paths.config_file.is_absolute());
    }

    #[test]
    fn ensure_creates_all_directories() {
        let tmp = tempfile::TempDir::new().unwrap();
        let paths = Paths::resolve(
            Some(tmp.path().join("cfg").join("config.toml")),
            Some(tmp.path().join("data")),
        )
        .unwrap();
        paths.ensure().unwrap();
        assert!(paths.config_dir().is_dir());
        assert!(paths.data_dir.is_dir());
        assert!(paths.log_dir().is_dir());
    }
}
