//! Client ani.pm (English) — pencarian lewat JSON API, detail/stream lewat
//! `playback-bootstrap` (halaman HTML-nya SPA yang tidak selalu di-prerender).

use std::sync::Arc;

use url::Url;

use crate::config::{Config, SourceConfig};
use crate::error::{SourceError, SourceResult};
use crate::http::{HttpClient, MirrorSet};
use crate::model::{
    AnimeDetail, AnimeSearchResult, Episode, RequestHeaders, SourceId, StreamKind, StreamLink,
};

pub const NAME: &str = "anipm";
pub const DEFAULT_BASE: &str = "https://ani.pm";
pub const MARKER: &str = "ani.pm";

#[derive(Debug)]
pub struct AniPm {
    source: SourceId,
    mirrors: MirrorSet,
    http: Arc<HttpClient>,
    user_agent: String,
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
            user_agent: config.http.user_agent.clone(),
        })
    }

    pub async fn base_url(&self) -> SourceResult<Url> {
        self.mirrors.resolve(&self.http, NAME).await
    }

    async fn bootstrap(&self, base: &Url, route: &str, episode: u32) -> SourceResult<String> {
        let path = format!("/api/anime/playback-bootstrap/settlar/{route}?ep={episode}&lang=sub");
        let url = base.join(&path).map_err(|err| {
            SourceError::parse(NAME, "detail", "url", format!("URL API tidak valid: {err}"))
        })?;
        self.http.get_text(NAME, &url, Some(base)).await
    }

    /// Ambil metadata: coba `ep=1`, lalu `ep=0` (judul tanpa episode bernomor 1).
    async fn bootstrap_detail(&self, base: &Url, route: &str) -> SourceResult<String> {
        match self.bootstrap(base, route, 1).await {
            Ok(body) if body.contains("\"core\"") => Ok(body),
            other => match self.bootstrap(base, route, 0).await {
                Ok(body) if body.contains("\"core\"") => Ok(body),
                Ok(_) => other,
                Err(_) => other,
            },
        }
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
        let route = super::parse::route_id(&page_url).ok_or_else(|| {
            SourceError::parse(NAME, "detail", "routeId", "tidak ada routeId di URL")
        })?;
        let body = self.bootstrap_detail(&base, &route).await?;
        super::parse::parse_detail(&body, &page_url, &base, &self.source)
    }

    pub async fn episodes(&self, anime_url: &str) -> SourceResult<Vec<Episode>> {
        self.detail(anime_url).await.map(|detail| detail.episodes)
    }

    /// Stream: halaman tonton resmi (player situs butuh konteks browser).
    ///
    /// Token `settlar` dari `playback-bootstrap` hanya valid di dalam iframe halaman
    /// ani.pm (dibuka langsung → `invalid-or-expired-embed-session`), jadi yang
    /// disediakan adalah halaman tonton resminya; player dibuka di browser.
    pub async fn fetch_streams(&self, episode_url: &str) -> SourceResult<Vec<StreamLink>> {
        let base = self.base_url().await?;
        let page = Url::parse(episode_url).map_err(|err| {
            SourceError::parse(NAME, "episode", "url", format!("URL tidak valid: {err}"))
        })?;
        let episode = super::parse::episode_number(&page).unwrap_or(1);
        Ok(vec![StreamLink {
            source: self.source.clone(),
            resolution: None,
            label: format!("Browser — Episode {episode}"),
            server: crate::embed::classify_server(&page),
            container: None,
            url: page,
            headers: RequestHeaders {
                referer: Some(base.clone()),
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
