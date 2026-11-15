//! Adapter Kusonime: metadata + katalog batch (tanpa streaming per episode).

pub mod client;
pub mod parse;

use std::sync::Arc;

use async_trait::async_trait;

use crate::config::{Config, SourceConfig};
use crate::error::{SourceError, SourceResult};
use crate::model::{AnimeDetail, AnimeSearchResult, Episode, StreamLink};
use crate::sources::{AnimeSource, Capabilities, SourceFactory};
use url::Url;

use client::Kusonime;
pub use client::NAME;

pub fn factory() -> SourceFactory {
    SourceFactory { name: NAME, build }
}

pub fn build(config: &Config, source_config: &SourceConfig) -> SourceResult<Arc<dyn AnimeSource>> {
    Ok(Arc::new(Kusonime::new(config, source_config)?))
}

#[async_trait]
impl AnimeSource for Kusonime {
    fn name(&self) -> &'static str {
        NAME
    }

    fn capabilities(&self) -> Capabilities {
        // Katalog batch: detail memuat tabel unduhan; tidak ada streaming.
        Capabilities {
            search: true,
            ongoing: false,
            completed: false,
            genres: true,
            detail: true,
            episodes: true,
            streams: true,
            batch_download: true,
        }
    }

    async fn search(&self, query: &str) -> SourceResult<Vec<AnimeSearchResult>> {
        Kusonime::search(self, query).await
    }

    async fn ongoing(&self) -> SourceResult<Vec<AnimeSearchResult>> {
        // Kusonime tidak membedakan ongoing/completed; semua halaman adalah batch.
        Err(SourceError::unsupported(NAME, "ongoing"))
    }

    async fn completed(&self) -> SourceResult<Vec<AnimeSearchResult>> {
        Err(SourceError::unsupported(NAME, "completed"))
    }

    async fn detail(&self, url: &str) -> SourceResult<AnimeDetail> {
        Kusonime::detail(self, url).await
    }

    async fn episodes(&self, anime_url: &str) -> SourceResult<Vec<Episode>> {
        // Satu halaman = satu seri; tampilkan satu baris "Batch".
        let url = Url::parse(anime_url).map_err(|err| {
            SourceError::parse(NAME, "episodes", "url", format!("URL tidak valid: {err}"))
        })?;
        Ok(vec![Episode {
            number: None,
            label: "Batch (semua episode)".to_string(),
            title: None,
            url,
            released: None,
        }])
    }

    async fn streams(&self, episode_url: &str) -> SourceResult<Vec<StreamLink>> {
        Kusonime::batch_streams(self, episode_url).await
    }
}
