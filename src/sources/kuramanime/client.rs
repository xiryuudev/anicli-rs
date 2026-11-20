//! Client Kuramanime (Indonesia) — `v20.kuramanime.ing`.

use std::sync::Arc;

use url::Url;

use crate::config::{Config, SourceConfig};
use crate::error::{SourceError, SourceResult};
use crate::http::{HttpClient, MirrorSet};
use crate::model::{
    AnimeDetail, AnimeSearchResult, Episode, RequestHeaders, SourceId, StreamKind, StreamLink,
};

pub const NAME: &str = "kuramanime";
pub const DEFAULT_BASE: &str = "https://v20.kuramanime.ing";
pub const MARKER: &str = "kuramanime";

#[derive(Debug)]
pub struct Kuramanime {
    source: SourceId,
    mirrors: MirrorSet,
    http: Arc<HttpClient>,
    user_agent: String,
}

impl Kuramanime {
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

    pub async fn search(&self, query: &str) -> SourceResult<Vec<AnimeSearchResult>> {
        let base = self.base_url().await?;
        let mut url = base.join("/anime").map_err(|err| {
            SourceError::parse(NAME, "search", "url", format!("URL tidak valid: {err}"))
        })?;
        url.query_pairs_mut().append_pair("search", query);
        let body = self.http.get_text(NAME, &url, Some(&base)).await?;
        super::parse::parse_list(&body, &base, &self.source)
    }

    pub async fn detail(&self, anime_url: &str) -> SourceResult<AnimeDetail> {
        let base = self.base_url().await?;
        let url = Url::parse(anime_url).map_err(|err| {
            SourceError::parse(NAME, "detail", "url", format!("URL tidak valid: {err}"))
        })?;
        let body = self.http.get_text(NAME, &url, Some(&base)).await?;
        super::parse::parse_detail(&body, &url, &self.source)
    }

    pub async fn episodes(&self, anime_url: &str) -> SourceResult<Vec<Episode>> {
        self.detail(anime_url).await.map(|detail| detail.episodes)
    }

    /// Player Kuramanime butuh parameter bertanda tangan + JS; buka halaman episode
    /// di browser (menampilkan pemilih server resmi).
    pub async fn fetch_streams(&self, episode_url: &str) -> SourceResult<Vec<StreamLink>> {
        let base = self.base_url().await?;
        let page = Url::parse(episode_url).map_err(|err| {
            SourceError::parse(NAME, "episode", "url", format!("URL tidak valid: {err}"))
        })?;
        let body = self.http.get_text(NAME, &page, Some(&base)).await?;
        let (title, servers) = super::parse::parse_episode(&body, &page, &self.source)?;
        let label = if servers.is_empty() {
            format!("Browser — {title}")
        } else {
            format!("Browser — {title} ({} server)", servers.len())
        };
        Ok(vec![StreamLink {
            source: self.source.clone(),
            resolution: None,
            label,
            server: crate::embed::classify_server(&page),
            container: None,
            url: page,
            headers: RequestHeaders {
                referer: Some(base),
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
