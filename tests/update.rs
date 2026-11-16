//! Uji fitur update: perbandingan versi, cache, dan pemasangan binary.

use anicli_rs::update::{asset_url, is_newer, update_hint, CheckCache, Platform};

#[cfg(unix)]
use anicli_rs::update::InstallOutcome;
#[cfg(unix)]
use std::path::PathBuf;

#[test]
fn version_compare_covers_common_cases() {
    assert!(is_newer("0.5.0", "0.5.1"));
    assert!(is_newer("0.5.0", "0.6.0"));
    assert!(is_newer("0.9.0", "1.0.0"));
    assert!(!is_newer("1.0.0", "1.0.0"));
    assert!(!is_newer("1.1.0", "1.0.0"));
}

#[test]
fn cache_roundtrip_and_freshness() {
    let dir = tempfile::TempDir::new().unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let cache = CheckCache {
        checked_at: now,
        latest: "9.9.9".into(),
    };
    cache.save(dir.path());

    let loaded = CheckCache::load(dir.path()).expect("cache tersimpan");
    assert_eq!(loaded.latest, "9.9.9");
    assert!(loaded.is_fresh(12), "baru saja dicek harus fresh");
    // interval 0 di-clamp ke 1 jam (minimal), jadi cache baru tetap fresh.
    assert!(loaded.is_fresh(0), "interval 0 di-clamp ke 1 jam");

    let old = CheckCache {
        checked_at: now.saturating_sub(48 * 3600),
        latest: "1.0.0".into(),
    };
    assert!(!old.is_fresh(12), "48 jam lalu harus basi");
}

#[test]
fn asset_url_points_to_latest_download() {
    let url = asset_url(Platform::LinuxX86_64).unwrap();
    assert!(url
        .as_str()
        .contains("releases/latest/download/anicli-x86_64-linux"));
    let win = asset_url(Platform::WindowsX86_64).unwrap();
    assert!(win.as_str().ends_with("anicli-x86_64-windows.exe"));
}

#[test]
fn hint_mentions_the_platform_command() {
    assert!(update_hint("v0.6.0", Some(Platform::AndroidAarch64)).contains("$PREFIX/bin/anicli"));
    assert!(update_hint("v0.6.0", Some(Platform::LinuxX86_64)).contains("anicli update"));
}

#[cfg(unix)]
#[test]
fn install_replaces_target_and_keeps_it_executable() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::TempDir::new().unwrap();
    let staged: PathBuf = dir.path().join("unduhan-berkas");
    std::fs::write(&staged, b"binary-baru").unwrap();

    let target = dir.path().join("anicli");
    std::fs::write(&target, b"binary-lama").unwrap();

    let outcome = anicli_rs::update::install_binary(&staged, &target).unwrap();
    match outcome {
        InstallOutcome::Installed { path } => assert_eq!(path, target),
        other => panic!("harus Installed, dapat {other:?}"),
    }
    assert_eq!(std::fs::read(&target).unwrap(), b"binary-baru");
    let mode = std::fs::metadata(&target).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o755, "binary harus executable");
    assert!(!staged.exists(), "berkas sementara harus dihapus");
    // tidak menyisakan file .new
    assert!(!dir.path().join("anicli.new").exists());
}

#[test]
fn writability_probe_detects_read_only_dir() {
    let dir = tempfile::TempDir::new().unwrap();
    let target = dir.path().join("anicli");
    assert!(anicli_rs::update::is_writable(&target));
}
