//! Adapter mkissa.to (English) — GraphQL API: search, detail, episode list.

pub mod client;
pub mod parse;

use std::sync::Arc;

use async_trait::async_trait;

use crate::config::{Config, SourceConfig};
use crate::error::{SourceError, SourceResult};
use crate::model::{AnimeDetail, AnimeSearchResult, Episode, Language, StreamLink};
use crate::sources::{AnimeSource, Capabilities, SourceFactory};

use client::Mkissa;
pub use client::NAME;

pub fn factory() -> SourceFactory {
    SourceFactory { name: NAME, build }
}

pub fn build(config: &Config, source_config: &SourceConfig) -> SourceResult<Arc<dyn AnimeSource>> {
    Ok(Arc::new(Mkissa::new(config, source_config)?))
}

#[async_trait]
impl AnimeSource for Mkissa {
    fn name(&self) -> &'static str {
        NAME
    }

    fn language(&self) -> Language {
        Language::English
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            search: true,
            ongoing: false,
            completed: false,
            genres: false,
            detail: true,
            episodes: true,
            // Stream asli butuh signature `aaReq`; kita buka halaman tonton resmi.
            streams: true,
            batch_download: false,
        }
    }

    async fn search(&self, query: &str) -> SourceResult<Vec<AnimeSearchResult>> {
        Mkissa::search(self, query).await
    }

    async fn ongoing(&self) -> SourceResult<Vec<AnimeSearchResult>> {
        Err(SourceError::unsupported(NAME, "ongoing"))
    }

    async fn completed(&self) -> SourceResult<Vec<AnimeSearchResult>> {
        Err(SourceError::unsupported(NAME, "completed"))
    }

    async fn detail(&self, url: &str) -> SourceResult<AnimeDetail> {
        Mkissa::detail(self, url).await
    }

    async fn episodes(&self, anime_url: &str) -> SourceResult<Vec<Episode>> {
        Mkissa::episodes(self, anime_url).await
    }

    async fn streams(&self, episode_url: &str) -> SourceResult<Vec<StreamLink>> {
        Mkissa::fetch_streams(self, episode_url).await
    }
}
