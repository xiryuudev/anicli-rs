//! Client ani.pm (English). Pencarian lewat JSON API; detail lewat LD+JSON.

use std::sync::Arc;

use url::Url;

use crate::config::{Config, SourceConfig};
use crate::error::{SourceError, SourceResult};
use crate::http::{HttpClient, MirrorSet};
use crate::model::{AnimeDetail, AnimeSearchResult, Episode, SourceId};

pub const NAME: &str = "anipm";
pub const DEFAULT_BASE: &str = "https://ani.pm";
pub const MARKER: &str = "ani.pm";

#[derive(Debug)]
pub struct AniPm {
    source: SourceId,
    mirrors: MirrorSet,
    http: Arc<HttpClient>,
}

impl AniPm {
    pub fn new(config: &Config, source_config: &SourceConfig) -> SourceResult<Self> {
        let http = Arc::new(HttpClient::new(&config.http)?);
        let base = if source_config.base_url.trim().is_empty() {
            DEFAULT_BASE
        } else {
            source_config.base_url.trim()
        };
        let mirrors = MirrorSet::new(
            base,
            &source_config.mirrors,
            MARKER,
            config.http.mirror_cache_ttl_secs,
        )?;
        Ok(Self {
            source: SourceId::new(NAME),
            mirrors,
            http,
        })
    }

    pub async fn base_url(&self) -> SourceResult<Url> {
        self.mirrors.resolve(&self.http, NAME).await
    }

    pub async fn search(&self, query: &str) -> SourceResult<Vec<AnimeSearchResult>> {
        let base = self.base_url().await?;
        let mut url = base.join("/api/anime/search").map_err(|err| {
            SourceError::parse(NAME, "search", "url", format!("URL tidak valid: {err}"))
        })?;
        url.query_pairs_mut().append_pair("q", query);
        let body = self.http.get_text(NAME, &url, Some(&base)).await?;
        super::parse::parse_search(&body, &base, &self.source)
    }

    pub async fn detail(&self, anime_url: &str) -> SourceResult<AnimeDetail> {
        let base = self.base_url().await?;
        let page_url = Url::parse(anime_url).map_err(|err| {
            SourceError::parse(NAME, "detail", "url", format!("URL tidak valid: {err}"))
        })?;
        let body = self.http.get_text(NAME, &page_url, Some(&base)).await?;
        super::parse::parse_detail(&body, &page_url, &base, &self.source)
    }

    pub async fn episodes(&self, anime_url: &str) -> SourceResult<Vec<Episode>> {
        self.detail(anime_url).await.map(|detail| detail.episodes)
    }

    pub fn source_id(&self) -> &SourceId {
        &self.source
    }

    pub fn http(&self) -> &HttpClient {
        &self.http
    }
}
