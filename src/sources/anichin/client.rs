//! Client Anichin: HTML Madara (`z1.anichin.blog`).

use std::sync::Arc;

use url::Url;

use crate::config::{Config, SourceConfig};
use crate::error::{SourceError, SourceResult};
use crate::http::{HttpClient, MirrorSet};
use crate::model::{
    AnimeDetail, AnimeSearchResult, Episode, RequestHeaders, SourceId, StreamKind, StreamLink,
};

pub const NAME: &str = "anichin";
pub const DEFAULT_BASE: &str = "https://z1.anichin.blog";
pub const MARKER: &str = "anichin";

#[derive(Debug)]
pub struct Anichin {
    source: SourceId,
    mirrors: MirrorSet,
    http: Arc<HttpClient>,
    user_agent: String,
}

impl Anichin {
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
            user_agent: config.http.user_agent.clone(),
        })
    }

    pub async fn base_url(&self) -> SourceResult<Url> {
        self.mirrors.resolve(&self.http, NAME).await
    }

    fn join(base: &Url, path: &str) -> SourceResult<Url> {
        base.join(path).map_err(|err| {
            SourceError::parse(
                NAME,
                "url",
                "join",
                format!("URL tidak valid {path:?}: {err}"),
            )
        })
    }

    pub async fn search(&self, query: &str) -> SourceResult<Vec<AnimeSearchResult>> {
        let base = self.base_url().await?;
        let mut url = Self::join(&base, "/")?;
        url.query_pairs_mut().append_pair("s", query);
        let body = self.http.get_text(NAME, &url, Some(&base)).await?;
        super::parse::parse_list(&body, &base, &self.source)
    }

    async fn browse(&self, status: &str) -> SourceResult<Vec<AnimeSearchResult>> {
        let base = self.base_url().await?;
        let mut url = Self::join(&base, "/series/")?;
        url.query_pairs_mut().append_pair("status", status);
        let body = self.http.get_text(NAME, &url, Some(&base)).await?;
        super::parse::parse_list(&body, &base, &self.source)
    }

    pub async fn ongoing(&self) -> SourceResult<Vec<AnimeSearchResult>> {
        self.browse("ongoing").await
    }

    pub async fn completed(&self) -> SourceResult<Vec<AnimeSearchResult>> {
        self.browse("completed").await
    }

    pub async fn detail(&self, anime_url: &str) -> SourceResult<AnimeDetail> {
        let base = self.base_url().await?;
        let url = Url::parse(anime_url).map_err(|err| {
            SourceError::parse(NAME, "detail", "url", format!("URL tidak valid: {err}"))
        })?;
        let body = self.http.get_text(NAME, &url, Some(&base)).await?;
        let mut detail = super::parse::parse_detail(&body, &base, &self.source)?;
        detail.url = url;
        Ok(detail)
    }

    pub async fn episodes(&self, anime_url: &str) -> SourceResult<Vec<Episode>> {
        self.detail(anime_url).await.map(|detail| detail.episodes)
    }

    /// Episode → link Embed (player `rmb.anichin.bio`) supaya bisa dibuka player/browser.
    pub async fn fetch_streams(&self, episode_url: &str) -> SourceResult<Vec<StreamLink>> {
        let page_url = Url::parse(episode_url).map_err(|err| {
            SourceError::parse(NAME, "episode", "url", format!("URL tidak valid: {err}"))
        })?;
        let base = self.base_url().await?;
        let body = self.http.get_text(NAME, &page_url, Some(&base)).await?;
        let (title, embed) = super::parse::parse_episode(&body, &base, &self.source)?;
        let Some(embed) = embed else {
            return Err(SourceError::parse(
                NAME,
                "episode",
                "iframe",
                "player tidak ditemukan di halaman episode",
            ));
        };
        Ok(vec![StreamLink {
            source: self.source.clone(),
            resolution: None,
            label: title,
            server: crate::embed::classify_server(&embed),
            container: None,
            url: embed.clone(),
            headers: RequestHeaders {
                referer: Some(page_url.clone()),
                user_agent: Some(self.user_agent.clone()),
            },
            stream_kind: StreamKind::Embed,
        }])
    }

    pub fn source_id(&self) -> &SourceId {
        &self.source
    }

    pub fn http(&self) -> &HttpClient {
        &self.http
    }
}
