//! Client Animeindo (`anime-indo.lol`).

use std::sync::Arc;

use url::Url;

use crate::config::{Config, SourceConfig};
use crate::error::{SourceError, SourceResult};
use crate::http::{HttpClient, MirrorSet};
use crate::model::{
    AnimeDetail, AnimeSearchResult, RequestHeaders, ServerKind, SourceId, StreamKind, StreamLink,
};

pub const NAME: &str = "animeindo";
pub const DEFAULT_BASE: &str = "https://anime-indo.lol";
/// Marker konten situs asli (bukan status 200, karena banyak domain clone).
pub const MARKER: &str = "tontonin";

#[derive(Debug)]
pub struct Animeindo {
    source: SourceId,
    mirrors: MirrorSet,
    http: Arc<HttpClient>,
    user_agent: String,
}

impl Animeindo {
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
                format!("gagal membentuk URL {path:?}: {err}"),
            )
        })
    }

    pub async fn search(&self, query: &str) -> SourceResult<Vec<AnimeSearchResult>> {
        let base = self.base_url().await?;
        let mut url = Self::join(&base, "/search.php")?;
        url.query_pairs_mut().append_pair("q", query);
        let body = self.http.get_text(NAME, &url, Some(&base)).await?;
        super::parse::parse_search(&body, &base, &self.source)
    }

    /// Home = rilisan terbaru (dipakai sebagai "ongoing"; situs tidak punya kategori).
    pub async fn ongoing(&self) -> SourceResult<Vec<AnimeSearchResult>> {
        let base = self.base_url().await?;
        let url = Self::join(&base, "/")?;
        let body = self.http.get_text(NAME, &url, Some(&base)).await?;
        super::parse::parse_list(&body, &base, &self.source)
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

    pub async fn episodes(&self, anime_url: &str) -> SourceResult<Vec<crate::model::Episode>> {
        self.detail(anime_url).await.map(|detail| detail.episodes)
    }

    /// Resolusi satu server: host `xtwap` punya `<source src>` langsung.
    pub async fn resolve_server(
        &self,
        page_url: &Url,
        server: &super::parse::ServerButton,
    ) -> SourceResult<Option<StreamLink>> {
        let host = server
            .url
            .host_str()
            .unwrap_or_default()
            .to_ascii_lowercase();
        let is_xtwap = host.contains("xtwap");
        if is_xtwap {
            let body = self.http.get_text(NAME, &server.url, None).await?;
            if let Some(media) = super::parse::player_media(&body) {
                let resolution =
                    crate::embed::googlevideo_resolution(&media).map(crate::model::Resolution);
                return Ok(Some(StreamLink {
                    source: self.source.clone(),
                    resolution,
                    label: format!(
                        "{} {}",
                        server.label,
                        resolution.map(|r| r.to_string()).unwrap_or_default()
                    )
                    .trim()
                    .to_string(),
                    server: ServerKind::Other("Xtwap".to_string()),
                    container: Some(crate::model::Container::Mp4),
                    headers: RequestHeaders {
                        referer: Some(page_url.clone()),
                        user_agent: Some(self.user_agent.clone()),
                    },
                    url: media,
                    stream_kind: StreamKind::Direct,
                }));
            }
        }

        Ok(Some(StreamLink {
            source: self.source.clone(),
            resolution: crate::sources::util::parse_resolution(&server.label),
            label: server.label.clone(),
            server: crate::embed::classify_server(&server.url),
            container: None,
            url: server.url.clone(),
            headers: RequestHeaders {
                referer: Some(page_url.clone()),
                user_agent: None,
            },
            stream_kind: StreamKind::Embed,
        }))
    }

    pub async fn fetch_streams(&self, episode_url: &str) -> SourceResult<Vec<StreamLink>> {
        let page_url = Url::parse(episode_url).map_err(|err| {
            SourceError::parse(NAME, "episode", "url", format!("URL tidak valid: {err}"))
        })?;
        let base = self.base_url().await?;
        let body = self.http.get_text(NAME, &page_url, Some(&base)).await?;
        let (_title, _number, servers) = super::parse::parse_episode(&body, &base, &self.source)?;

        let mut links = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for server in &servers {
            if let Ok(Some(link)) = self.resolve_server(&page_url, server).await {
                if seen.insert(link.url.to_string()) {
                    links.push(link);
                }
            }
        }
        if links.is_empty() {
            return Err(SourceError::parse(
                NAME,
                "episode",
                "streams",
                "tidak ada server yang bisa dipakai",
            ));
        }
        Ok(links)
    }

    pub fn source_id(&self) -> &SourceId {
        &self.source
    }

    pub fn http(&self) -> &HttpClient {
        &self.http
    }
}
