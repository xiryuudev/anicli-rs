//! Adapter Kusonime: metadata + katalog batch (tanpa streaming per episode).

pub mod client;
pub mod parse;

use std::sync::Arc;

use async_trait::async_trait;

use crate::config::{Config, SourceConfig};
use crate::error::{SourceError, SourceResult};
use crate::model::{AnimeDetail, AnimeSearchResult, Episode, StreamLink};
use crate::sources::{AnimeSource, Capabilities, SourceFactory};

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
        // Download-only: berguna untuk pencarian, metadata, dan katalog batch.
        Capabilities::metadata_only()
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

    async fn episodes(&self, _anime_url: &str) -> SourceResult<Vec<Episode>> {
        Err(SourceError::unsupported(NAME, "episodes"))
    }

    async fn streams(&self, _episode_url: &str) -> SourceResult<Vec<StreamLink>> {
        Err(SourceError::unsupported(NAME, "streams"))
    }
}
