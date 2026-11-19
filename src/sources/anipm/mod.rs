//! Adapter ani.pm — source berbahasa Inggris (search + daftar episode).

pub mod client;
pub mod parse;

use std::sync::Arc;

use async_trait::async_trait;

use crate::config::{Config, SourceConfig};
use crate::error::{SourceError, SourceResult};
use crate::model::{AnimeDetail, AnimeSearchResult, Episode, Language, StreamLink};
use crate::sources::{AnimeSource, Capabilities, SourceFactory};

use client::AniPm;
pub use client::NAME;

pub fn factory() -> SourceFactory {
    SourceFactory { name: NAME, build }
}

pub fn build(config: &Config, source_config: &SourceConfig) -> SourceResult<Arc<dyn AnimeSource>> {
    Ok(Arc::new(AniPm::new(config, source_config)?))
}

#[async_trait]
impl AnimeSource for AniPm {
    fn name(&self) -> &'static str {
        NAME
    }

    fn language(&self) -> Language {
        Language::English
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            // Katalog + daftar episode; player SPA-nya belum diimplementasikan.
            search: true,
            ongoing: false,
            completed: false,
            genres: true,
            detail: true,
            episodes: true,
            streams: false,
            batch_download: false,
        }
    }

    async fn search(&self, query: &str) -> SourceResult<Vec<AnimeSearchResult>> {
        AniPm::search(self, query).await
    }

    async fn ongoing(&self) -> SourceResult<Vec<AnimeSearchResult>> {
        Err(SourceError::unsupported(NAME, "ongoing"))
    }

    async fn completed(&self) -> SourceResult<Vec<AnimeSearchResult>> {
        Err(SourceError::unsupported(NAME, "completed"))
    }

    async fn detail(&self, url: &str) -> SourceResult<AnimeDetail> {
        AniPm::detail(self, url).await
    }

    async fn episodes(&self, anime_url: &str) -> SourceResult<Vec<Episode>> {
        AniPm::episodes(self, anime_url).await
    }

    async fn streams(&self, _episode_url: &str) -> SourceResult<Vec<StreamLink>> {
        Err(SourceError::unsupported(NAME, "streams"))
    }
}
