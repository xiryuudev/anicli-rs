//! Binary bantu `sourcecheck`: cek satu source secara live.
//!
//! Alur: search → detail → daftar episode → stream, lalu cetak hasilnya. Dipakai
//! untuk verifikasi Tahap 5 dan debugging parser saat situs berubah.
//!
//! Contoh:
//! ```text
//! cargo run --bin sourcecheck -- --source samehadaku --query "one piece"
//! cargo run --bin sourcecheck -- --source otakudesu --episode <url-episode>
//! ```

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{bail, Context, Result};
use clap::Parser;

use anicli_rs::config;
use anicli_rs::model::StreamKind;
use anicli_rs::paths::Paths;
use anicli_rs::sources::SourceRegistry;

#[derive(Debug, Parser)]
#[command(
    name = "sourcecheck",
    version,
    about = "Cek live satu source anicli-rs"
)]
struct Cli {
    /// File config (default: lokasi platform).
    #[arg(long, env = "ANICLI_CONFIG")]
    config: Option<PathBuf>,

    /// Direktori data (untuk cache/log).
    #[arg(long, env = "ANICLI_DATA_DIR")]
    data_dir: Option<PathBuf>,

    /// Nama source; default = `general.default_source` di config.
    #[arg(long)]
    source: Option<String>,

    /// Kata kunci pencarian.
    #[arg(long, default_value = "one piece")]
    query: String,

    /// Langsung cek detail URL ini (lewati pencarian).
    #[arg(long)]
    detail: Option<String>,

    /// Langsung cek episode URL ini (lewati pemilihan episode).
    #[arg(long)]
    episode: Option<String>,

    /// Resolusi yang ingin dicek (hanya untuk tampilan).
    #[arg(long, default_value = "720p")]
    resolution: String,
}

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("sourcecheck: {err:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<()> {
    let cli = Cli::parse();
    let paths = Paths::resolve(cli.config.clone(), cli.data_dir.clone())
        .context("menentukan path config/data")?;
    let loaded = config::load(&paths).context("memuat config")?;
    let config = loaded.config;

    let build = SourceRegistry::from_config(&config);
    let registry = build.registry;
    if registry.is_empty() {
        bail!(
            "tidak ada source terpasang (missing: {})",
            build.missing.join(", ")
        );
    }

    let source_name = cli
        .source
        .clone()
        .unwrap_or_else(|| config.general.default_source.clone());
    let source = registry.get(&source_name).with_context(|| {
        format!(
            "source {source_name:?} tidak terpasang; tersedia: {}",
            registry.names().join(", ")
        )
    })?;

    println!("source   : {}", source.name());
    println!(
        "caps     : search={} streams={} episodes={}",
        source.capabilities().search,
        source.capabilities().streams,
        source.capabilities().episodes
    );
    println!("resolusi : {}", cli.resolution);
    println!();

    // 1) Detail (langsung atau lewat pencarian).
    let detail_url = match &cli.detail {
        Some(url) => url.clone(),
        None => {
            let results = source
                .search(&cli.query)
                .await
                .with_context(|| format!("search {:?}", cli.query))?;
            println!("search   : {} hasil", results.len());
            for (index, item) in results.iter().take(5).enumerate() {
                println!("  {}. {} — {}", index + 1, item.title, item.url);
            }
            let Some(first) = results.first() else {
                bail!("pencarian tidak menghasilkan apa pun");
            };
            first.url.to_string()
        }
    };
    println!("detail   : {detail_url}");
    let detail = source.detail(&detail_url).await.context("ambil detail")?;
    println!("judul    : {}", detail.title);
    println!(
        "status   : {:?} | tahun: {:?} | skor: {:?}",
        detail.status, detail.year, detail.score
    );
    println!("genre    : {}", detail.genres.join(", "));
    println!("episodes : {}", detail.episodes.len());
    for episode in detail.episodes.iter().take(3) {
        println!("  - {} ({})", episode.label_for_ui(), episode.url);
    }
    if detail.episodes.len() > 3 {
        println!("  ... {} episode lagi", detail.episodes.len() - 3);
    }
    if !detail.synopsis.is_empty() {
        let snippet: String = detail.synopsis.chars().take(120).collect();
        println!("sinopsis : {snippet}…");
    }

    if !source.capabilities().streams {
        println!();
        println!("streams  : tidak didukung source ini (download-only)");
        return Ok(());
    }

    // 2) Episode target.
    let episode_url = match &cli.episode {
        Some(url) => url.clone(),
        None => {
            let episodes = if detail.episodes.is_empty() {
                match source.episodes(detail.url.as_str()).await {
                    Ok(list) => list,
                    Err(err) if err.is_source_fatal() => {
                        println!();
                        println!("episode  : tidak tersedia ({})", err.user_message());
                        return Ok(());
                    }
                    Err(err) => return Err(err).context("ambil daftar episode"),
                }
            } else {
                detail.episodes.clone()
            };
            let Some(last) = episodes.last() else {
                println!();
                println!("episode  : tidak ada episode yang bisa dicek");
                return Ok(());
            };
            last.url.to_string()
        }
    };
    println!();
    println!("episode  : {episode_url}");

    // 3) Stream.
    match source.streams(&episode_url).await {
        Ok(links) => {
            println!("streams  : {} link", links.len());
            let playable = links
                .iter()
                .filter(|link| link.stream_kind == StreamKind::Direct)
                .count();
            println!("direct   : {playable} link siap mpv");
            for link in &links {
                println!(
                    "  [{}] {} | {} | {} | {}",
                    link.resolution_label(),
                    link.stream_kind_label(),
                    link.server,
                    link.label,
                    link.url
                );
            }
        }
        Err(err) => {
            // Bukan kegagalan sourcecheck: source download-only memang tidak punya stream.
            println!("streams  : tidak ada ({err})");
            println!("           pesan UI: {}", err.user_message());
        }
    }

    Ok(())
}
