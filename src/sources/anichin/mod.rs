//! Adapter Anichin (donghua sub Indonesia).

pub mod client;
pub mod parse;

use std::sync::Arc;

use async_trait::async_trait;

use crate::config::{Config, SourceConfig};
use crate::error::SourceResult;
use crate::model::{AnimeDetail, AnimeSearchResult, Episode, Language, StreamLink};
use crate::sources::{AnimeSource, Capabilities, SourceFactory};

use client::Anichin;
pub use client::NAME;

pub fn factory() -> SourceFactory {
    SourceFactory { name: NAME, build }
}

pub fn build(config: &Config, source_config: &SourceConfig) -> SourceResult<Arc<dyn AnimeSource>> {
    Ok(Arc::new(Anichin::new(config, source_config)?))
}

#[async_trait]
impl AnimeSource for Anichin {
    fn name(&self) -> &'static str {
        NAME
    }

    fn language(&self) -> Language {
        Language::Indonesian
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities::all()
    }

    async fn search(&self, query: &str) -> SourceResult<Vec<AnimeSearchResult>> {
        Anichin::search(self, query).await
    }

    async fn ongoing(&self) -> SourceResult<Vec<AnimeSearchResult>> {
        Anichin::ongoing(self).await
    }

    async fn completed(&self) -> SourceResult<Vec<AnimeSearchResult>> {
        Anichin::completed(self).await
    }

    async fn detail(&self, url: &str) -> SourceResult<AnimeDetail> {
        Anichin::detail(self, url).await
    }

    async fn episodes(&self, anime_url: &str) -> SourceResult<Vec<Episode>> {
        Anichin::episodes(self, anime_url).await
    }

    async fn streams(&self, episode_url: &str) -> SourceResult<Vec<StreamLink>> {
        Anichin::fetch_streams(self, episode_url).await
    }
}
