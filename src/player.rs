//! Integrasi player eksternal mpv/vlc (DESIGN §12).
//!
//! Modul ini murni membangun perintah + menjalankan proses; keputusan "link mana
//! yang diputar" ada di [`crate::resolve`] dan UI. Rencana pemutaran dipisah sebagai
//! [`PlaybackPlan`] supaya bisa dites tanpa benar-benar men-spawn proses.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use tokio::process::{Child, Command};

use crate::config::{AndroidPlayer, Config, EmbedStrategy, Player as PlayerKind};
use crate::error::{SourceError, SourceResult};
use crate::model::{StreamKind, StreamLink};

pub mod android;

pub const MPV_BIN: &str = "mpv";
pub const VLC_BIN: &str = "vlc";
pub const YTDLP_BIN: &str = "yt-dlp";

/// Perintah player yang siap dijalankan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerCommand {
    pub program: PathBuf,
    pub args: Vec<String>,
}

impl PlayerCommand {
    /// Representasi untuk log/UI (tidak untuk dieksekusi via shell).
    pub fn display(&self) -> String {
        format!("{} {}", self.program.display(), self.args.join(" "))
    }
}

/// Cari binary di path konfigurasi atau di `PATH` (eksplisit agar mudah dites).
pub fn find_binary(name: &str, configured: &str, path_env: Option<&OsStr>) -> Option<PathBuf> {
    let configured = configured.trim();
    if !configured.is_empty() {
        let path = PathBuf::from(configured);
        if path.is_file() {
            return Some(path);
        }
    }

    for dir in std::env::split_paths(path_env?) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
        #[cfg(windows)]
        {
            let candidate = dir.join(format!("{name}.exe"));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

pub fn find_player(
    kind: PlayerKind,
    configured: &str,
    path_env: Option<&OsStr>,
) -> Option<PathBuf> {
    match kind {
        PlayerKind::Mpv => find_binary(MPV_BIN, configured, path_env),
        PlayerKind::Vlc => find_binary(VLC_BIN, configured, path_env),
    }
}

fn media_headers(link: &StreamLink, fallback_ua: &str) -> (String, Option<String>) {
    let user_agent = link
        .headers
        .user_agent
        .clone()
        .unwrap_or_else(|| fallback_ua.to_string());
    let referer = link.headers.referer.as_ref().map(ToString::to_string);
    (user_agent, referer)
}

/// Argumen mpv untuk satu link.
pub fn mpv_args(
    link: &StreamLink,
    title: &str,
    fallback_ua: &str,
    start: Option<u64>,
    foreground: bool,
    extra: &[String],
    embed: bool,
) -> Vec<String> {
    let (user_agent, referer) = media_headers(link, fallback_ua);
    let mut args = Vec::new();
    if !foreground {
        args.push("--no-terminal".to_string());
    }
    args.push(format!("--force-media-title={title}"));
    args.push(format!("--user-agent={user_agent}"));
    if let Some(referer) = referer {
        args.push(format!("--referrer={referer}"));
    }
    args.push("--cache=yes".to_string());
    if let Some(start) = start {
        args.push(format!("--start={start}"));
    }
    if embed {
        args.push("--ytdl=yes".to_string());
    }
    args.extend(extra.iter().cloned());
    args.push(link.url.to_string());
    args
}

/// Argumen vlc untuk satu link.
pub fn vlc_args(
    link: &StreamLink,
    title: &str,
    fallback_ua: &str,
    start: Option<u64>,
    extra: &[String],
) -> Vec<String> {
    let (user_agent, referer) = media_headers(link, fallback_ua);
    let mut args = vec!["--play-and-exit".to_string()];
    args.push(format!("--input-title-format={title}"));
    args.push(format!("--http-user-agent={user_agent}"));
    if let Some(referer) = referer {
        args.push(format!("--http-referrer={referer}"));
    }
    if let Some(start) = start {
        args.push(format!("--start-time={start}"));
    }
    args.extend(extra.iter().cloned());
    args.push(link.url.to_string());
    args
}

/// Bangun perintah lengkap untuk player tertentu.
// Banyak parameter lebih jelas daripada struct builder satu-pakai di sini.
#[allow(clippy::too_many_arguments)]
pub fn build_command(
    kind: PlayerKind,
    program: &Path,
    link: &StreamLink,
    title: &str,
    fallback_ua: &str,
    start: Option<u64>,
    foreground: bool,
    extra: &[String],
) -> PlayerCommand {
    let embed = link.stream_kind == StreamKind::Embed;
    let args = match kind {
        PlayerKind::Mpv => mpv_args(link, title, fallback_ua, start, foreground, extra, embed),
        PlayerKind::Vlc => vlc_args(link, title, fallback_ua, start, extra),
    };
    PlayerCommand {
        program: program.to_path_buf(),
        args,
    }
}

/// Rencana pemutaran: spawn player, buka browser (link embed), atau tolak dengan alasan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlaybackPlan {
    Spawn(PlayerCommand),
    /// Di Android/Termux: serahkan media ke aplikasi player lewat intent.
    Android {
        url: String,
        mime: String,
        prefer: AndroidPlayer,
    },
    Browser {
        url: String,
        reason: String,
    },
    Refuse(String),
}

impl PlaybackPlan {
    pub fn describe(&self) -> String {
        match self {
            Self::Spawn(command) => format!("memutar via {}", command.program.display()),
            Self::Android { prefer, .. } => format!("membuka player Android ({prefer:?})"),
            Self::Browser { reason, .. } => format!("membuka browser: {reason}"),
            Self::Refuse(reason) => reason.clone(),
        }
    }
}

/// Tentukan rencana dari config + link + keberadaan binary.
///
/// Parameter binary dibuat eksplisit supaya test tidak perlu mengubah `PATH` proses.
pub fn plan_playback(
    config: &Config,
    link: &StreamLink,
    title: &str,
    start: Option<u64>,
    player: Option<PathBuf>,
    ytdlp: Option<PathBuf>,
) -> PlaybackPlan {
    plan_playback_with(
        config,
        link,
        title,
        start,
        player,
        ytdlp,
        android::is_termux(),
    )
}

/// Sama seperti [`plan_playback`], tetapi status Android diberikan eksplisit
/// supaya bisa dites tanpa mengubah environment proses.
#[allow(clippy::too_many_arguments)]
pub fn plan_playback_with(
    config: &Config,
    link: &StreamLink,
    title: &str,
    start: Option<u64>,
    player: Option<PathBuf>,
    ytdlp: Option<PathBuf>,
    on_android: bool,
) -> PlaybackPlan {
    match link.stream_kind {
        StreamKind::DownloadOnly => {
            return PlaybackPlan::Refuse(
                "Link ini hanya untuk unduhan (bukan streaming). Pilih link lain.".to_string(),
            );
        }
        StreamKind::Direct => {}
        StreamKind::Embed => {
            if config.playback.embed_strategy == EmbedStrategy::Skip {
                return PlaybackPlan::Refuse(
                    "Link embed dilewati sesuai config (embed_strategy = skip).".to_string(),
                );
            }
            if config.playback.embed_strategy == EmbedStrategy::Browser
                || config.playback.player == PlayerKind::Vlc
                || ytdlp.is_none()
            {
                return PlaybackPlan::Browser {
                    url: link.url.to_string(),
                    reason: if ytdlp.is_none() {
                        "yt-dlp tidak ditemukan; embed dibuka di browser".to_string()
                    } else {
                        "embed dibuka di browser".to_string()
                    },
                };
            }
        }
    }

    if on_android {
        // Android/Termux: mpv/vlc di PATH adalah build terminal tanpa GUI, jadi
        // serahkan ke aplikasi lewat intent. Embed tetap ke browser.
        if link.stream_kind == StreamKind::Embed
            && config.playback.embed_strategy == EmbedStrategy::Browser
        {
            return PlaybackPlan::Browser {
                url: link.url.to_string(),
                reason: "embed dibuka di browser".to_string(),
            };
        }
        let mime = android::media_mime(link.url.as_str(), link.container);
        return PlaybackPlan::Android {
            url: link.url.to_string(),
            mime: mime.to_string(),
            prefer: config.playback.android_player,
        };
    }

    let Some(program) = player else {
        return PlaybackPlan::Refuse(format!(
            "{} tidak ditemukan. Install {} atau set path-nya di config [playback].",
            config.playback.player,
            if config.playback.player == PlayerKind::Mpv {
                "mpv"
            } else {
                "vlc"
            }
        ));
    };

    let command = build_command(
        config.playback.player,
        &program,
        link,
        title,
        &config.http.user_agent,
        start,
        config.playback.foreground,
        &config.playback.extra_args,
    );
    PlaybackPlan::Spawn(command)
}

/// Jalankan player. `foreground` = warisi terminal (mpv dengan kontrol keyboard).
pub async fn spawn_player(command: &PlayerCommand, foreground: bool) -> SourceResult<Child> {
    let mut process = Command::new(&command.program);
    process.args(&command.args);
    process.stdin(std::process::Stdio::null());
    if foreground {
        process.stdout(std::process::Stdio::inherit());
        // stderr tetap ditangkap: mpv menulis error ke sini walau TUI aktif.
        process.stderr(std::process::Stdio::piped());
    } else {
        process.stdout(std::process::Stdio::null());
        // Tangkap stderr supaya kegagalan mpv (mis. yt-dlp error) bisa ditampilkan,
        // bukan sekadar "selesai diputar".
        process.stderr(std::process::Stdio::piped());
    }
    process.spawn().map_err(|err| {
        SourceError::app(format!(
            "gagal menjalankan {}: {err}",
            command.program.display()
        ))
    })
}

/// Tunggu player selesai sambil membaca stderr (untuk pesan error yang berguna).
pub async fn wait_with_stderr(mut child: Child) -> (Option<i32>, String) {
    use tokio::io::AsyncReadExt as _;

    let stderr = child.stderr.take();
    let reader = tokio::spawn(async move {
        let mut buffer = String::new();
        if let Some(mut stderr) = stderr {
            let _ = stderr.read_to_string(&mut buffer).await;
        }
        buffer
    });

    let code = child.wait().await.ok().and_then(|status| status.code());
    let stderr = reader.await.unwrap_or_default();
    (code, stderr)
}

/// Ringkas stderr menjadi satu baris pendek untuk status/error TUI.
pub fn summarize_stderr(stderr: &str, max_chars: usize) -> String {
    let noisy = ["[ffmpeg]", "[ytdl_hook]", "[ytdl]", "[file]"];
    let line = stderr
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .rev()
        .find(|line| !noisy.iter().any(|prefix| line.starts_with(prefix)))
        .or_else(|| stderr.lines().map(str::trim).find(|line| !line.is_empty()))
        .unwrap_or("tanpa detail");
    let mut text = line.to_string();
    if text.chars().count() > max_chars {
        text = text.chars().take(max_chars).collect::<String>() + "…";
    }
    text
}

/// Buka URL di browser default (untuk link embed yang tidak bisa diputar langsung).
pub fn open_browser(url: &str) -> SourceResult<()> {
    // Android/Termux tidak punya xdg-open: gunakan termux-open-url.
    if android::is_termux() {
        for program in ["termux-open-url", "termux-open"] {
            let mut command = std::process::Command::new(program);
            command.arg(url);
            command
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null());
            if command.spawn().is_ok() {
                return Ok(());
            }
        }
        return Err(SourceError::app(
            "gagal membuka browser: termux-open-url tidak tersedia (pkg install termux-tools)",
        ));
    }

    let (program, args): (&str, Vec<&str>) = if cfg!(target_os = "windows") {
        ("cmd", vec!["/C", "start", "", url])
    } else if cfg!(target_os = "macos") {
        ("open", vec![url])
    } else {
        ("xdg-open", vec![url])
    };
    std::process::Command::new(program)
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|err| SourceError::app(format!("gagal membuka browser: {err}")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{RequestHeaders, Resolution, ServerKind, SourceId};
    use url::Url;

    fn link(kind: StreamKind) -> StreamLink {
        StreamLink {
            source: SourceId::new("test"),
            resolution: Some(Resolution::P720),
            label: "720p".into(),
            server: ServerKind::Icefile,
            container: None,
            url: Url::parse("https://cdn.test/720.m3u8").unwrap(),
            headers: RequestHeaders {
                referer: Some(Url::parse("https://source.test/episode/1").unwrap()),
                user_agent: Some("UA-Link".to_string()),
            },
            stream_kind: kind,
        }
    }

    fn fake_binary(dir: &Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, "#!/bin/sh\nexit 0\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        path
    }

    #[test]
    fn finds_player_in_explicit_path_then_path_env() {
        let tmp = tempfile::TempDir::new().unwrap();
        let mpv = fake_binary(tmp.path(), "mpv");

        // Path konfigurasi menang.
        assert_eq!(
            find_player(PlayerKind::Mpv, mpv.to_str().unwrap(), None),
            Some(mpv.clone())
        );

        // Lewat PATH eksplisit.
        let path_env = tmp.path().as_os_str();
        assert_eq!(find_player(PlayerKind::Mpv, "", Some(path_env)), Some(mpv));
        assert_eq!(find_player(PlayerKind::Vlc, "", Some(path_env)), None);
    }

    #[test]
    fn mpv_args_include_headers_start_and_no_terminal() {
        let args = mpv_args(
            &link(StreamKind::Direct),
            "Shuiro no Kamen E2",
            "UA-Fallback",
            Some(123),
            false,
            &["--hwdec=auto".to_string()],
            false,
        );
        let joined = args.join(" ");
        assert!(joined.contains("--no-terminal"));
        assert!(joined.contains("--force-media-title=Shuiro no Kamen E2"));
        assert!(
            joined.contains("--user-agent=UA-Link"),
            "header link menang: {joined}"
        );
        assert!(joined.contains("--referrer=https://source.test/episode/1"));
        assert!(joined.contains("--start=123"));
        assert!(joined.contains("--hwdec=auto"));
        assert!(joined.ends_with("https://cdn.test/720.m3u8"));
    }

    #[test]
    fn vlc_args_use_vlc_header_switches() {
        let args = vlc_args(
            &link(StreamKind::Direct),
            "Judul",
            "UA-Fallback",
            Some(30),
            &[],
        );
        let joined = args.join(" ");
        assert!(joined.contains("--play-and-exit"));
        assert!(joined.contains("--http-user-agent=UA-Link"));
        assert!(joined.contains("--http-referrer=https://source.test/episode/1"));
        assert!(joined.contains("--start-time=30"));
    }

    #[test]
    fn direct_links_spawn_player() {
        let tmp = tempfile::TempDir::new().unwrap();
        let mpv = fake_binary(tmp.path(), "mpv");
        let plan = plan_playback(
            &Config::default(),
            &link(StreamKind::Direct),
            "Judul",
            None,
            Some(mpv),
            None,
        );
        match plan {
            PlaybackPlan::Spawn(command) => {
                assert!(command.args.contains(&"--no-terminal".to_string()));
            }
            other => panic!("harus spawn, dapat {other:?}"),
        }
    }

    #[test]
    fn embed_with_ytdl_uses_mpv_ytdl() {
        let tmp = tempfile::TempDir::new().unwrap();
        let mpv = fake_binary(tmp.path(), "mpv");
        let ytdlp = fake_binary(tmp.path(), "yt-dlp");
        let plan = plan_playback(
            &Config::default(),
            &link(StreamKind::Embed),
            "Judul",
            None,
            Some(mpv),
            Some(ytdlp),
        );
        match plan {
            PlaybackPlan::Spawn(command) => {
                assert!(command.args.contains(&"--ytdl=yes".to_string()));
                assert!(command
                    .args
                    .iter()
                    .any(|arg| arg.contains("mega.nz") || arg.contains("cdn.test")));
            }
            other => panic!("harus spawn, dapat {other:?}"),
        }
    }

    #[test]
    fn android_uses_intent_plan_instead_of_spawning_mpv() {
        let config = Config::default();
        let mut link = link(StreamKind::Direct);
        link.url = Url::parse("https://cdn.example/stream.m3u8").unwrap();
        let plan = plan_playback_with(&config, &link, "Judul", None, None, None, true);
        match plan {
            PlaybackPlan::Android { url, mime, prefer } => {
                assert!(url.ends_with("stream.m3u8"));
                assert_eq!(mime, "application/vnd.apple.mpegurl");
                assert_eq!(prefer, crate::config::AndroidPlayer::Auto);
            }
            other => panic!("harus Android plan, dapat {other:?}"),
        }
    }

    #[test]
    fn embed_without_ytdl_falls_back_to_browser() {
        let tmp = tempfile::TempDir::new().unwrap();
        let mpv = fake_binary(tmp.path(), "mpv");
        let plan = plan_playback(
            &Config::default(),
            &link(StreamKind::Embed),
            "Judul",
            None,
            Some(mpv),
            None,
        );
        assert!(matches!(plan, PlaybackPlan::Browser { .. }), "{plan:?}");
    }

    #[test]
    fn download_only_is_refused() {
        let plan = plan_playback(
            &Config::default(),
            &link(StreamKind::DownloadOnly),
            "Judul",
            None,
            None,
            None,
        );
        assert!(matches!(plan, PlaybackPlan::Refuse(_)), "{plan:?}");
    }

    #[test]
    fn missing_player_is_refused_with_install_hint() {
        let plan = plan_playback(
            &Config::default(),
            &link(StreamKind::Direct),
            "Judul",
            None,
            None,
            None,
        );
        match plan {
            PlaybackPlan::Refuse(message) => {
                assert!(message.contains("mpv"), "{message}");
                assert!(
                    message.contains("install") || message.contains("Install"),
                    "{message}"
                );
            }
            other => panic!("harus Refuse, dapat {other:?}"),
        }
    }

    #[test]
    fn vlc_cannot_play_embed_so_browser_is_used() {
        let tmp = tempfile::TempDir::new().unwrap();
        let vlc = fake_binary(tmp.path(), "vlc");
        let mut config = Config::default();
        config.playback.player = PlayerKind::Vlc;
        let plan = plan_playback(
            &config,
            &link(StreamKind::Embed),
            "Judul",
            None,
            Some(vlc),
            None,
        );
        assert!(matches!(plan, PlaybackPlan::Browser { .. }), "{plan:?}");
    }
}
