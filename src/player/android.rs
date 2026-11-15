//! Peluncuran player di Android/Termux.
//!
//! Di Termux tidak ada `mpv` GUI; media harus diserahkan ke aplikasi Android lewat
//! intent. Urutan percobaan (mengikuti praktik ani-cli/termux):
//! `termux-am-starter` → `termux-am` → `am` → `termux-open` → `termux-open-url`.
//!
//! Untuk memilih aplikasi, intent memakai `-p <package>` (implicit VIEW + package
//! filter) sehingga tidak bergantung pada nama activity internal yang bisa berubah:
//! mpv-android = `is.xyz.mpv`, VLC Android = `org.videolan.vlc`.

use std::path::PathBuf;

use crate::config::AndroidPlayer;

pub const MPV_PACKAGE: &str = "is.xyz.mpv";
pub const VLC_PACKAGE: &str = "org.videolan.vlc";

/// Apakah proses berjalan di Termux (env khas Termux)?
pub fn is_termux() -> bool {
    std::env::var_os("TERMUX_VERSION").is_some()
        || std::env::var("PREFIX")
            .map(|prefix| prefix.contains("com.termux"))
            .unwrap_or(false)
}

/// Satu percobaan peluncuran: program + argumen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchAttempt {
    pub program: String,
    pub args: Vec<String>,
}

impl LaunchAttempt {
    pub fn display(&self) -> String {
        format!("{} {}", self.program, self.args.join(" "))
    }
}

/// MIME type untuk intent berdasarkan container/ekstensi URL.
pub fn media_mime(url: &str, container: Option<crate::model::Container>) -> &'static str {
    let lowered = url.to_ascii_lowercase();
    if lowered.contains(".m3u8") {
        return "application/vnd.apple.mpegurl";
    }
    match container {
        Some(crate::model::Container::Mkv) => "video/x-matroska",
        Some(crate::model::Container::Mp4) => "video/mp4",
        _ => "video/*",
    }
}

fn package_for(prefer: AndroidPlayer) -> Option<&'static str> {
    match prefer {
        AndroidPlayer::Mpv => Some(MPV_PACKAGE),
        AndroidPlayer::Vlc => Some(VLC_PACKAGE),
        AndroidPlayer::Auto => None,
    }
}

fn am_attempt(program: &str, url: &str, mime: &str, package: Option<&str>) -> LaunchAttempt {
    let mut args = vec![
        "start".to_string(),
        "-a".to_string(),
        "android.intent.action.VIEW".to_string(),
        "-d".to_string(),
        url.to_string(),
        "-t".to_string(),
        mime.to_string(),
    ];
    if let Some(package) = package {
        args.push("-p".to_string());
        args.push(package.to_string());
    }
    LaunchAttempt {
        program: program.to_string(),
        args,
    }
}

/// Urutan percobaan peluncuran untuk URL tertentu.
pub fn attempts(url: &str, mime: &str, prefer: AndroidPlayer) -> Vec<LaunchAttempt> {
    let package = package_for(prefer);
    let mut candidates = Vec::new();

    if let Some(package) = package {
        // Preferensi eksplisit: coba paket itu dulu lewat `am`.
        candidates.push(am_attempt("am", url, mime, Some(package)));
    }
    candidates.push(am_attempt("termux-am-starter", url, mime, package));
    candidates.push(am_attempt("termux-am", url, mime, package));
    candidates.push(am_attempt("am", url, mime, None));
    candidates.push(LaunchAttempt {
        program: "termux-open".to_string(),
        args: vec![
            "--content-type".to_string(),
            mime.to_string(),
            url.to_string(),
        ],
    });
    candidates.push(LaunchAttempt {
        program: "termux-open-url".to_string(),
        args: vec![url.to_string()],
    });
    candidates
}

fn find_in_path(program: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(program))
        .find(|candidate| candidate.is_file())
}

/// Coba semua kandidat sampai ada yang berhasil. Mengembalikan program yang dipakai.
pub fn launch(url: &str, mime: &str, prefer: AndroidPlayer) -> Result<String, String> {
    let mut log = Vec::new();
    for attempt in attempts(url, mime, prefer) {
        let Some(program) = find_in_path(&attempt.program) else {
            log.push(format!("{}: tidak ditemukan di PATH", attempt.program));
            continue;
        };
        match std::process::Command::new(&program)
            .args(&attempt.args)
            .status()
        {
            Ok(status) if status.success() => return Ok(attempt.program),
            Ok(status) => log.push(format!(
                "{}: exit {}",
                attempt.program,
                status.code().unwrap_or(-1)
            )),
            Err(err) => log.push(format!("{}: {err}", attempt.program)),
        }
    }
    Err(format!(
        "tidak ada peluncur Android yang berhasil ({}). \
         Install mpv-android (F-Droid) atau VLC, dan paket termux-am.",
        log.join("; ")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Container;

    #[test]
    fn mime_follows_container_and_hls() {
        assert_eq!(
            media_mime("https://x/a.m3u8", None),
            "application/vnd.apple.mpegurl"
        );
        assert_eq!(
            media_mime("https://x/a.mkv", Some(Container::Mkv)),
            "video/x-matroska"
        );
        assert_eq!(
            media_mime("https://x/a.mp4", Some(Container::Mp4)),
            "video/mp4"
        );
        assert_eq!(media_mime("https://x/a", None), "video/*");
    }

    #[test]
    fn mpv_preference_uses_package_filter_first() {
        let attempts = attempts("https://x/a.mp4", "video/mp4", AndroidPlayer::Mpv);
        let first = &attempts[0];
        assert_eq!(first.program, "am");
        assert!(first.args.contains(&"-p".to_string()));
        assert!(first.args.contains(&MPV_PACKAGE.to_string()));
        // Fallback tetap ada.
        assert!(attempts.iter().any(|a| a.program == "termux-open"));
        assert!(attempts.iter().any(|a| a.program == "termux-open-url"));
    }

    #[test]
    fn auto_preference_skips_package_filter() {
        let attempts = attempts("https://x/a.mp4", "video/mp4", AndroidPlayer::Auto);
        assert!(attempts.iter().all(|a| !a.args.contains(&"-p".to_string())));
        assert_eq!(attempts.last().unwrap().program, "termux-open-url");
    }
}
