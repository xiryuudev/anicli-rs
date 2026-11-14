//! Client Otakudesu: AJAX nonce/mirror + pemanggilan halaman.

use std::sync::{Arc, Mutex};

use serde_json::Value;
use url::Url;

use crate::config::{Config, SourceConfig};
use crate::embed;
use crate::error::{SourceError, SourceResult};
use crate::http::{HttpClient, MirrorSet};
use crate::model::{
    AiringStatus, AnimeDetail, AnimeSearchResult, RequestHeaders, ServerKind, SourceId, StreamKind,
    StreamLink,
};

use super::parse::{self, DownloadRow, EpisodePage, MirrorEntry};

pub const NAME: &str = "otakudesu";
pub const DEFAULT_BASE: &str = "https://otakudesu.blog";
/// Marker ringan yang muncul di semua halaman asli.
pub const MARKER: &str = "venser";

const AJAX_NONCE_ACTION: &str = "aa1208d27f29ca340c92c66d1926f13f";
const AJAX_MIRROR_ACTION: &str = "2a3505c93b0035d3f455df82bf976b84";

#[derive(Debug, Clone)]
pub struct EpisodeFetch {
    pub page: EpisodePage,
    pub downloads: Vec<DownloadRow>,
}

#[derive(Debug)]
pub struct Otakudesu {
    source: SourceId,
    mirrors: MirrorSet,
    http: Arc<HttpClient>,
    user_agent: String,
    nonce: Mutex<Option<String>>,
}

impl Otakudesu {
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
            nonce: Mutex::new(None),
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

    fn with_query(base: &Url, path: &str, params: &[(&str, &str)]) -> SourceResult<Url> {
        let mut url = Self::join(base, path)?;
        {
            let mut query = url.query_pairs_mut();
            for (key, value) in params {
                query.append_pair(key, value);
            }
        }
        Ok(url)
    }

    fn ajax_url(base: &Url) -> SourceResult<Url> {
        Self::join(base, "/wp-admin/admin-ajax.php")
    }

    pub async fn search(&self, query: &str) -> SourceResult<Vec<AnimeSearchResult>> {
        let base = self.base_url().await?;
        let url = Self::with_query(&base, "/", &[("s", query), ("post_type", "anime")])?;
        let body = self.http.get_text(NAME, &url, Some(&base)).await?;
        parse::parse_search(&body, &base, &self.source)
    }

    pub async fn ongoing(&self) -> SourceResult<Vec<AnimeSearchResult>> {
        let base = self.base_url().await?;
        let url = Self::join(&base, "/ongoing-anime/")?;
        let body = self.http.get_text(NAME, &url, Some(&base)).await?;
        parse::parse_list(&body, &base, &self.source, parse::ListPage::Ongoing)
    }

    pub async fn completed(&self) -> SourceResult<Vec<AnimeSearchResult>> {
        let base = self.base_url().await?;
        let url = Self::join(&base, "/complete-anime/")?;
        let body = self.http.get_text(NAME, &url, Some(&base)).await?;
        parse::parse_list(&body, &base, &self.source, parse::ListPage::Completed)
    }

    pub async fn detail(&self, anime_url: &str) -> SourceResult<AnimeDetail> {
        let base = self.base_url().await?;
        let url = Url::parse(anime_url).map_err(|err| {
            SourceError::parse(NAME, "detail", "url", format!("URL tidak valid: {err}"))
        })?;
        let body = self.http.get_text(NAME, &url, Some(&base)).await?;
        let mut detail = parse::parse_detail(&body, &base, &self.source)?;
        // URL asli anime (bukan base) supaya fallback/riwayat konsisten.
        detail.url = url;
        Ok(detail)
    }

    pub async fn episode(&self, episode_url: &str) -> SourceResult<EpisodeFetch> {
        let base = self.base_url().await?;
        let url = Url::parse(episode_url).map_err(|err| {
            SourceError::parse(NAME, "episode", "url", format!("URL tidak valid: {err}"))
        })?;
        let body = self.http.get_text(NAME, &url, Some(&base)).await?;
        let page = parse::parse_episode(&body, &base, &self.source)?;
        let document = scraper::Html::parse_document(&body);
        let downloads = parse::parse_downloads(&document, &base);
        Ok(EpisodeFetch { page, downloads })
    }

    /// Ambil nonce AJAX (disimpan di memori; nonce bertahan dalam satu run).
    async fn nonce(&self) -> SourceResult<String> {
        if let Ok(guard) = self.nonce.lock() {
            if let Some(nonce) = guard.clone() {
                return Ok(nonce);
            }
        }
        let base = self.base_url().await?;
        let url = Self::ajax_url(&base)?;
        let form = vec![("action", AJAX_NONCE_ACTION.to_string())];
        let body = self
            .http
            .post_form_text(NAME, &url, &form, Some(&base))
            .await?;
        let value: Value = serde_json::from_str(&body).map_err(|err| {
            SourceError::parse(NAME, "ajax", "nonce", format!("respons bukan JSON: {err}"))
        })?;
        let nonce = value
            .get("data")
            .and_then(Value::as_str)
            .map(ToString::to_string)
            .ok_or_else(|| {
                SourceError::parse(NAME, "ajax", "nonce", "respons tidak memuat `data`")
            })?;
        if let Ok(mut guard) = self.nonce.lock() {
            *guard = Some(nonce.clone());
        }
        Ok(nonce)
    }

    /// Resolve satu mirror (`data-content` → AJAX → iframe embed URL).
    pub async fn resolve_mirror(&self, mirror: &MirrorEntry) -> SourceResult<Option<Url>> {
        let base = self.base_url().await?;
        let url = Self::ajax_url(&base)?;
        let nonce = self.nonce().await?;
        let form = vec![
            ("action", AJAX_MIRROR_ACTION.to_string()),
            ("id", mirror.id.to_string()),
            ("i", mirror.index.to_string()),
            (
                "q",
                mirror.resolution.map(|r| r.to_string()).unwrap_or_default(),
            ),
            ("nonce", nonce),
        ];
        let body = self
            .http
            .post_form_text(NAME, &url, &form, Some(&base))
            .await?;
        let value: Value = serde_json::from_str(&body).map_err(|err| {
            SourceError::parse(NAME, "ajax", "mirror", format!("respons bukan JSON: {err}"))
        })?;

        if let Some(message) = value.get("message").and_then(Value::as_str) {
            tracing::debug!(target: "anicli_rs::otakudesu", mirror = %mirror.label, message, "mirror tidak tersedia");
            return Ok(None);
        }

        let Some(encoded) = value.get("data").and_then(Value::as_str) else {
            return Ok(None);
        };
        let Some(html) = crate::sources::util::b64decode_text(encoded) else {
            return Ok(None);
        };
        Ok(crate::embed::iframe_src(&html).and_then(|src| Url::parse(&src).ok()))
    }

    /// Resolusi embed desustream → URL media (butuh User-Agent saat diputar).
    pub async fn desustream_media(&self, embed_url: &Url) -> SourceResult<Option<StreamLink>> {
        let body = self.http.get_text(NAME, embed_url, None).await?;
        let Some(media) = embed::desustream_media(&body) else {
            return Ok(None);
        };
        Ok(Some(StreamLink {
            source: self.source.clone(),
            resolution: None,
            label: "Default".to_string(),
            server: ServerKind::Desustream,
            container: embed::container_for("", &media),
            headers: RequestHeaders {
                referer: None,
                user_agent: Some(self.user_agent.clone()),
            },
            url: media,
            stream_kind: StreamKind::Direct,
        }))
    }

    pub fn embed_link(
        &self,
        url: &Url,
        resolution: Option<crate::model::Resolution>,
        label: impl Into<String>,
    ) -> StreamLink {
        let server = crate::embed::classify_server(url);
        StreamLink {
            source: self.source.clone(),
            resolution,
            label: label.into(),
            server,
            container: None,
            url: url.clone(),
            headers: RequestHeaders::default(),
            stream_kind: StreamKind::Embed,
        }
    }

    pub fn source_id(&self) -> &SourceId {
        &self.source
    }

    pub fn http(&self) -> &HttpClient {
        &self.http
    }

    /// Status completed bila `AiringStatus::Completed`, untuk tes cepat.
    pub fn completion_label(status: AiringStatus) -> &'static str {
        match status {
            AiringStatus::Completed => "Completed",
            AiringStatus::Ongoing => "Ongoing",
            AiringStatus::Unknown => "Unknown",
        }
    }
}
