//! Adapter Animeindo.

pub mod client;
pub mod parse;

use std::sync::Arc;

use async_trait::async_trait;

use crate::config::{Config, SourceConfig};
use crate::error::{SourceError, SourceResult};
use crate::model::{AnimeDetail, AnimeSearchResult, Episode, StreamLink};
use crate::sources::{AnimeSource, Capabilities, SourceFactory};

use client::Animeindo;
pub use client::NAME;

pub fn factory() -> SourceFactory {
    SourceFactory { name: NAME, build }
}

pub fn build(config: &Config, source_config: &SourceConfig) -> SourceResult<Arc<dyn AnimeSource>> {
    Ok(Arc::new(Animeindo::new(config, source_config)?))
}

#[async_trait]
impl AnimeSource for Animeindo {
    fn name(&self) -> &'static str {
        NAME
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            // Situs tidak punya kategori "completed" terpisah.
            completed: false,
            ..Capabilities::all()
        }
    }

    async fn search(&self, query: &str) -> SourceResult<Vec<AnimeSearchResult>> {
        Animeindo::search(self, query).await
    }

    async fn ongoing(&self) -> SourceResult<Vec<AnimeSearchResult>> {
        Animeindo::ongoing(self).await
    }

    async fn completed(&self) -> SourceResult<Vec<AnimeSearchResult>> {
        Err(SourceError::unsupported(NAME, "completed"))
    }

    async fn detail(&self, url: &str) -> SourceResult<AnimeDetail> {
        Animeindo::detail(self, url).await
    }

    async fn episodes(&self, anime_url: &str) -> SourceResult<Vec<Episode>> {
        Animeindo::episodes(self, anime_url).await
    }

    async fn streams(&self, episode_url: &str) -> SourceResult<Vec<StreamLink>> {
        Animeindo::fetch_streams(self, episode_url).await
    }
}
