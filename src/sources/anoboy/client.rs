//! Client Anoboy: URL builder + pengambilan halaman.

use std::sync::Arc;

use url::Url;

use crate::config::{Config, SourceConfig};
use crate::error::{SourceError, SourceResult};
use crate::http::{HttpClient, MirrorSet};
use crate::model::{
    AiringStatus, AnimeDetail, AnimeSearchResult, RequestHeaders, ServerKind, SourceId, StreamKind,
    StreamLink,
};

pub const NAME: &str = "anoboy";
pub const DEFAULT_BASE: &str = "https://anoboy.id";
/// Domain aktif memakai "anoboy" di URL aset/canonical (case-sensitive aman).
pub const MARKER: &str = "anoboy";

#[derive(Debug)]
pub struct Anoboy {
    source: SourceId,
    mirrors: MirrorSet,
    http: Arc<HttpClient>,
    user_agent: String,
}

impl Anoboy {
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

    pub async fn fetch_text(&self, url: &Url) -> SourceResult<String> {
        let base = self.base_url().await?;
        self.http.get_text(NAME, url, Some(&base)).await
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
        let mut url = Self::join(&base, "/")?;
        url.query_pairs_mut().append_pair("s", query);
        let body = self.http.get_text(NAME, &url, Some(&base)).await?;
        super::parse::parse_list(&body, &base, &self.source, None)
    }

    pub async fn ongoing(&self) -> SourceResult<Vec<AnimeSearchResult>> {
        self.list_with_status("ongoing", Some(AiringStatus::Ongoing))
            .await
    }

    pub async fn completed(&self) -> SourceResult<Vec<AnimeSearchResult>> {
        self.list_with_status("completed", Some(AiringStatus::Completed))
            .await
    }

    async fn list_with_status(
        &self,
        status: &str,
        airing: Option<AiringStatus>,
    ) -> SourceResult<Vec<AnimeSearchResult>> {
        let base = self.base_url().await?;
        let mut url = Self::join(&base, "/series/")?;
        {
            let mut query = url.query_pairs_mut();
            query.append_pair("status", status);
            // WAJIB `type=anime`; tanpa itu situs mengembalikan item rusak.
            query.append_pair("type", "anime");
            query.append_pair("order", "update");
        }
        let body = self.http.get_text(NAME, &url, Some(&base)).await?;
        super::parse::parse_list(&body, &base, &self.source, airing)
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

    /// Resolusi embed Blogger → URL MP4 googlevideo (IP-bound, resolve saat play).
    pub async fn resolve_blogger(&self, embed: &Url) -> SourceResult<Option<StreamLink>> {
        let body = self.http.get_text(NAME, embed, None).await?;
        let Some(media) = crate::embed::blogspot_media(&body) else {
            return Ok(None);
        };
        let resolution = crate::embed::googlevideo_resolution(&media).map(crate::model::Resolution);
        Ok(Some(StreamLink {
            source: self.source.clone(),
            resolution,
            label: format!(
                "Blogspot {}",
                resolution.map(|r| r.to_string()).unwrap_or_default()
            )
            .trim()
            .to_string(),
            server: ServerKind::Blogspot,
            container: Some(crate::model::Container::Mp4),
            headers: RequestHeaders {
                referer: None,
                user_agent: Some(self.user_agent.clone()),
            },
            url: media,
            stream_kind: StreamKind::Direct,
        }))
    }

    pub fn source_id(&self) -> &SourceId {
        &self.source
    }

    pub fn http(&self) -> &HttpClient {
        &self.http
    }
}
