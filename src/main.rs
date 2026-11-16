//! Entry point CLI `anicli` (Tahap 3).
//!
//! Tahap ini belum menyalakan TUI. Yang dibuktikan: CLI (clap), penentuan path per
//! platform, load/validasi config dengan pembuatan otomatis, logging file harian,
//! dan ringkasan konfigurasi efektif. TUI masuk di Tahap 6.

#![warn(clippy::uninlined_format_args)]
#![forbid(unsafe_code)]

use std::io::IsTerminal;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};

use anicli_rs::config::{self, Config, LoadedConfig, Overrides, Player};
use anicli_rs::logging::{self, LogHandle};
use anicli_rs::model::{ParseResolutionError, Resolution};
use anicli_rs::paths::Paths;
use anicli_rs::sources::SourceRegistry;
use anicli_rs::ui;

/// Exit code untuk kesalahan yang bisa diperbaiki user di config/CLI.
const EXIT_CONFIG: u8 = 2;
/// Exit code untuk kegagalan jaringan (mis. cek update).
const EXIT_NETWORK: u8 = 3;

#[derive(Debug, Parser)]
#[command(
    name = "anicli",
    version,
    about = "TUI streaming anime subtitle Indonesia (mpv/vlc)",
    long_about = "anicli-rs — cari anime, pilih episode & resolusi, lalu putar ke mpv.\n\
                  Config: ~/.config/anicli-rs/config.toml (Linux) / %APPDATA%\\anicli-rs\\ (Windows).",
    propagate_version = true
)]
struct Cli {
    /// File config yang dipakai (default: lokasi platform).
    #[arg(long, global = true, value_name = "FILE", env = "ANICLI_CONFIG")]
    config: Option<PathBuf>,

    /// Direktori data (riwayat, bookmark, log).
    #[arg(long, global = true, value_name = "DIR", env = "ANICLI_DATA_DIR")]
    data_dir: Option<PathBuf>,

    /// Source yang dipakai sesi ini (harus ada & enabled di config).
    #[arg(long, global = true, value_name = "NAME", env = "ANICLI_SOURCE")]
    source: Option<String>,

    /// Resolusi default sesi ini, mis. 720p / 1080p.
    #[arg(
        long,
        global = true,
        value_name = "RES",
        value_parser = parse_resolution,
        env = "ANICLI_RESOLUTION"
    )]
    resolution: Option<Resolution>,

    /// Player untuk sesi ini.
    #[arg(long, global = true, value_enum, env = "ANICLI_PLAYER")]
    player: Option<PlayerArg>,

    /// Level/direktif log (trace, debug, info, warn, error, atau direktif EnvFilter).
    #[arg(long, global = true, value_name = "LEVEL", env = "ANICLI_LOG")]
    log_level: Option<String>,

    /// Jangan nyalakan TUI; cukup cetak ringkasan (berguna untuk CI/pipe/SSH).
    #[arg(long, global = true)]
    no_tui: bool,

    /// Mode mock: pakai fixture HTML dari direktori ini, tanpa jaringan (Tahap 5+).
    #[arg(long, global = true, value_name = "DIR")]
    mock: Option<PathBuf>,

    /// Cek update lalu pasang binary versi terbaru (sama dengan `anicli update`).
    #[arg(short = 'U', long = "update", global = true)]
    update_flag: bool,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum PlayerArg {
    Mpv,
    Vlc,
}

impl From<PlayerArg> for Player {
    fn from(value: PlayerArg) -> Self {
        match value {
            PlayerArg::Mpv => Self::Mpv,
            PlayerArg::Vlc => Self::Vlc,
        }
    }
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Kelola file konfigurasi.
    Config(ConfigArgs),
    /// Cek & pasang versi terbaru dari GitHub Releases.
    Update(UpdateArgs),
}

#[derive(Debug, Args)]
struct UpdateArgs {
    /// Hanya cek versi terbaru, jangan unduh/pasang.
    #[arg(long)]
    check: bool,
}

#[derive(Debug, Args)]
struct ConfigArgs {
    #[command(subcommand)]
    action: ConfigAction,
}

#[derive(Debug, Subcommand)]
enum ConfigAction {
    /// Buat config.toml dari template berkomentar.
    Init {
        /// Timpa file yang sudah ada.
        #[arg(long)]
        force: bool,
    },
    /// Cetak lokasi file config, data, dan log.
    Path,
    /// Cetak config efektif (setelah override CLI) dalam format TOML.
    Show,
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli).await {
        Ok(code) => code,
        Err(err) => {
            eprintln!("anicli: {err:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run(cli: Cli) -> Result<ExitCode> {
    let paths = Paths::resolve(cli.config.clone(), cli.data_dir.clone())
        .context("gagal menentukan lokasi config/data")?;

    // Update tidak butuh config (dan harus jalan walau config rusak).
    if cli.update_flag || matches!(cli.command, Some(Command::Update(_))) {
        let check_only = matches!(
            cli.command,
            Some(Command::Update(UpdateArgs { check: true }))
        );
        return update_cli(&paths, check_only).await;
    }

    // Subcommand yang tidak butuh memuat config.
    if let Some(Command::Config(args)) = &cli.command {
        match &args.action {
            ConfigAction::Init { force } => return init_config(&paths, *force),
            ConfigAction::Path => return print_paths(&paths),
            ConfigAction::Show => { /* lanjut: perlu config ter-load */ }
        }
    }

    let loaded = match config::load(&paths) {
        Ok(loaded) => loaded,
        Err(err) if is_user_fixable(&err) => {
            eprintln!("anicli: {err}");
            return Ok(ExitCode::from(EXIT_CONFIG));
        }
        Err(err) => return Err(err.into()),
    };

    let mut config = loaded.config.clone();
    let mut warnings = loaded.warnings.clone();

    match config.apply_overrides(&Overrides {
        source: cli.source.clone(),
        resolution: cli.resolution,
        player: cli.player.map(Player::from),
    }) {
        Ok(more) => warnings.extend(more),
        Err(err) if is_user_fixable(&err) => {
            eprintln!("anicli: {err}");
            return Ok(ExitCode::from(EXIT_CONFIG));
        }
        Err(err) => return Err(err.into()),
    }

    let interactive =
        !cli.no_tui && std::io::stdin().is_terminal() && std::io::stdout().is_terminal();
    let log = init_logging(&cli, &paths, &config, interactive)?;

    tracing::info!(
        version = anicli_rs::VERSION,
        config = %paths.config_file.display(),
        data = %paths.data_dir.display(),
        created = loaded.created,
        sources = config.playback_order().join(","),
        "anicli-rs mulai"
    );
    for warning in &warnings {
        tracing::warn!(target: "anicli_rs::config", "{}", warning);
    }

    // Registry source: yang enabled di config tetapi belum diimplementasikan akan
    // dilaporkan (Tahap 5 mengisi implementasinya).
    let registry_build = SourceRegistry::from_config(&config);
    for warning in &registry_build.warnings {
        tracing::warn!(target: "anicli_rs::sources", "{}", warning);
    }
    tracing::info!(
        installed = registry_build.registry.len(),
        missing = registry_build.missing.len(),
        "source registry siap"
    );
    warnings.extend(registry_build.warnings.iter().cloned());

    if let Some(mock) = &cli.mock {
        if !mock.is_dir() {
            anyhow::bail!(
                "--mock {} bukan direktori (mode mock dipakai di Tahap 5)",
                mock.display()
            );
        }
        tracing::warn!(dir = %mock.display(), "mode mock aktif (Tahap 5+)");
    }

    if let Some(Command::Config(args)) = &cli.command {
        if matches!(args.action, ConfigAction::Show) {
            let text = config.to_toml()?;
            print!("{text}");
            return Ok(ExitCode::SUCCESS);
        }
    }

    if interactive {
        let registry = Arc::new(registry_build.registry);
        let source_name = config
            .playback_order()
            .first()
            .map(|name| name.to_string())
            .unwrap_or_else(|| config.general.default_source.clone());
        let mut app = ui::App::new(
            config,
            registry,
            source_name,
            paths.clone(),
            Some(log.buffer().clone()),
        );
        app.mock_mode = cli.mock.is_some();
        ui::run(app).await?;
        drop(log);
        return Ok(ExitCode::SUCCESS);
    }

    print_summary(
        &cli,
        &paths,
        &config,
        &loaded,
        &warnings,
        &log,
        &registry_build.registry,
    );
    Ok(ExitCode::SUCCESS)
}

/// `anicli update` / `anicli -U`: cek rilis terbaru, lalu unduh & pasang.
async fn update_cli(paths: &Paths, check_only: bool) -> Result<ExitCode> {
    use anicli_rs::update::{self, InstallOutcome, Platform};

    let http = anicli_rs::http::HttpClient::new(&anicli_rs::config::HttpConfig::default())
        .context("gagal membuat HTTP client untuk cek update")?;
    println!("anicli {} — memeriksa rilis terbaru…", anicli_rs::VERSION);

    let release = match update::latest_release(&http).await {
        Ok(release) => release,
        Err(err) => {
            eprintln!("anicli: gagal memeriksa update: {}", err.user_message());
            return Ok(ExitCode::from(EXIT_NETWORK));
        }
    };

    let platform = Platform::current();
    if !update::is_newer(anicli_rs::VERSION, &release.version) {
        println!(
            "✓ sudah versi terbaru ({}){}",
            anicli_rs::VERSION,
            platform
                .map(|platform| format!(" · {}", platform.describe()))
                .unwrap_or_default()
        );
        return Ok(ExitCode::SUCCESS);
    }

    println!(
        "↑ versi baru tersedia: {} → {} ({})",
        anicli_rs::VERSION,
        release.version,
        release.tag
    );
    if !release.notes.trim().is_empty() {
        let first = release
            .notes
            .lines()
            .find(|line| !line.trim().is_empty() && !line.trim_start().starts_with('#'))
            .unwrap_or("")
            .trim();
        if !first.is_empty() {
            println!("  {first}");
        }
    }
    println!("  {}", release.html_url);

    let Some(platform) = platform else {
        println!(
            "\nPlatform ini belum punya aset otomatis. Unduh manual dari {}",
            release.html_url
        );
        return Ok(ExitCode::SUCCESS);
    };
    println!("  aset: {} ({})", platform.asset(), platform.describe());

    if check_only {
        println!(
            "\nJalankan `anicli update` untuk memasang {}.",
            release.version
        );
        return Ok(ExitCode::SUCCESS);
    }

    let target = update::current_binary()?;
    if !update::is_writable(&target) {
        println!(
            "\n! Folder binary tidak bisa ditulis ({}).\n  {}",
            target.display(),
            update::update_hint(&release.tag, Some(platform))
        );
        return Ok(ExitCode::from(EXIT_CONFIG));
    }

    let data_dir = paths.data_dir.clone();
    let staged = data_dir.join("update").join(platform.asset());
    println!("↓ mengunduh {}…", platform.asset());
    let size = match update::download_asset(platform, &staged).await {
        Ok(size) => size,
        Err(err) => {
            eprintln!("anicli: unduhan gagal: {}", err.user_message());
            return Ok(ExitCode::from(EXIT_NETWORK));
        }
    };
    println!("  {size} byte terunduh");

    match update::install_binary(&staged, &target)? {
        InstallOutcome::Installed { path } => {
            println!(
                "✓ terpasang: {} (versi {})",
                path.display(),
                release.version
            );
            println!("  jalankan ulang anicli untuk memakai versi baru.");
        }
        InstallOutcome::PendingWindows { path } => {
            println!("✓ berkas baru disiapkan: {}", path.display());
            println!(
                "  Windows mengunci binary yang sedang berjalan. Tutup anicli lalu jalankan:\n\
                 \x20   move /Y \"{}\" \"{}\"",
                path.display(),
                target.display()
            );
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn init_config(paths: &Paths, force: bool) -> Result<ExitCode> {
    match config::init(paths, force)? {
        config::InitOutcome::Created(path) => {
            println!("config dibuat: {}", path.display());
        }
        config::InitOutcome::AlreadyExists(path) => {
            println!("config sudah ada: {}", path.display());
            println!("pakai --force untuk menimpa dengan template default.");
        }
        config::InitOutcome::Overwritten(path) => {
            println!("config ditimpa dengan template: {}", path.display());
        }
    }
    println!("data: {}", paths.data_dir.display());
    Ok(ExitCode::SUCCESS)
}

fn print_paths(paths: &Paths) -> Result<ExitCode> {
    println!("config : {}", paths.config_file.display());
    println!("data   : {}", paths.data_dir.display());
    println!("logs   : {}", paths.log_dir().display());
    println!("history: {}", paths.history_file().display());
    println!("bookmark: {}", paths.bookmarks_file().display());
    Ok(ExitCode::SUCCESS)
}

fn init_logging(cli: &Cli, paths: &Paths, config: &Config, interactive: bool) -> Result<LogHandle> {
    // Presedensi (DESIGN §14.3): CLI > RUST_LOG > config.
    let level = match (&cli.log_level, std::env::var("RUST_LOG")) {
        (Some(level), _) => level.clone(),
        (None, Ok(env_level)) if !env_level.trim().is_empty() => env_level,
        _ => config.logging.level.clone(),
    };

    logging::init(logging::Options {
        log_dir: paths.log_dir(),
        level,
        // stderr hanya aman kalau TUI tidak aktif.
        to_stderr: !interactive || config.logging.to_stderr,
        buffer_capacity: 1000,
        max_days: config.logging.max_days,
    })
}

fn print_summary(
    cli: &Cli,
    paths: &Paths,
    config: &Config,
    loaded: &LoadedConfig,
    warnings: &[String],
    log: &LogHandle,
    registry: &SourceRegistry,
) {
    println!("anicli-rs {}", anicli_rs::VERSION);
    println!();
    println!("config   : {}", paths.config_file.display());
    println!("data     : {}", paths.data_dir.display());
    println!("logs     : {}", log.log_dir().display());
    println!();
    println!("source   : {}", config.general.default_source);
    println!("fallback : {}", config.playback_order().join(" -> "));
    println!(
        "resolusi : {} ({})",
        config.playback.default_resolution, config.playback.resolution_policy
    );
    println!(
        "player   : {}{}",
        config.playback.player,
        if cli.player.is_some() {
            " (override CLI)"
        } else {
            ""
        }
    );
    println!(
        "http     : delay {} ms + jitter {} ms, timeout {} s, max {} paralel, retry {}",
        config.http.request_delay_ms,
        config.http.jitter_ms,
        config.http.timeout_secs,
        config.http.max_concurrent,
        config.http.retries
    );
    println!(
        "embed    : {} (ytdl tersedia: {})",
        config.playback.embed_strategy,
        player_available("yt-dlp")
    );
    println!(
        "player bin: mpv={} vlc={}",
        player_available("mpv"),
        player_available("vlc")
    );
    println!();
    println!("sources  :");
    // Urutkan sesuai urutan pemakaian, lalu sisa source nonaktif.
    let mut ordered: Vec<&str> = config.playback_order();
    let remaining: Vec<&str> = config
        .sources
        .keys()
        .map(String::as_str)
        .filter(|name| !ordered.contains(name))
        .collect();
    ordered.extend(remaining);
    for name in ordered {
        let Some(source) = config.sources.get(name) else {
            continue;
        };
        let status = if source.enabled { "aktif" } else { "nonaktif" };
        println!("  [{status:8}] {name:12} {}", source.base_url);
    }
    println!();
    println!(
        "registry : {} source terpasang ({} enabled di config)",
        registry.len(),
        config.enabled_source_names().len()
    );
    if registry.is_empty() && !config.sources.is_empty() {
        println!("           implementasi source menyusul di Tahap 5");
    }
    println!();
    if !warnings.is_empty() {
        println!("peringatan ({}):", warnings.len());
        for warning in warnings {
            println!("  ! {warning}");
        }
        println!();
    }
    println!("log      : {} baris di ring buffer", log.buffer().len());
    if loaded.created {
        println!("catatan  : config baru dibuat dari template");
    }
    println!();
    println!("TUI belum aktif di tahap ini — ringkasan di atas untuk verifikasi config & logging.");
}

/// Apakah binary player/extractor tersedia di PATH (untuk ringkasan saja).
fn player_available(binary: &str) -> &'static str {
    let found = std::env::var_os("PATH").is_some_and(|paths| {
        std::env::split_paths(&paths).any(|dir| {
            let candidate = dir.join(binary);
            candidate.is_file() || cfg!(windows) && dir.join(format!("{binary}.exe")).is_file()
        })
    });
    if found {
        "ada"
    } else {
        "tidak ada"
    }
}

fn parse_resolution(raw: &str) -> Result<Resolution, String> {
    raw.parse::<Resolution>()
        .map_err(|err: ParseResolutionError| err.to_string())
}

fn is_user_fixable(err: &config::ConfigError) -> bool {
    matches!(
        err,
        config::ConfigError::Invalid(_)
            | config::ConfigError::Parse { .. }
            | config::ConfigError::VersionTooNew { .. }
    )
}
