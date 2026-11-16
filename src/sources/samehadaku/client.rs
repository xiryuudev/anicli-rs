//! Client Samehadaku: URL builder, endpoint AJAX/REST, dan pemanggilan HTTP.
//!
//! Pemisahan dari `parse.rs` membuat parser bisa dites offline; modul ini yang
//! menyentuh jaringan dan memetakan kegagalan HTTP ke [`SourceError`].

use std::sync::Arc;

use url::Url;

use super::parse::{self, DownloadRow, EpisodePage, ServerButton};
use crate::config::{Config, SourceConfig};
use crate::error::{SourceError, SourceResult};
use crate::http::{HttpClient, MirrorSet};
use crate::model::{
    AnimeSearchResult, Container, RequestHeaders, ServerKind, SourceId, StreamKind, StreamLink,
};

/// Nama source (dipakai config, log, dan UI).
pub const NAME: &str = "samehadaku";
/// Marker yang harus muncul di halaman home untuk memvalidasi domain aktif.
pub const MARKER: &str = "Samehadaku";
/// Domain utama (bisa dioverride lewat config).
pub const DEFAULT_BASE: &str = "https://v2.samehadaku.how";

/// Data yang diambil dari satu halaman episode (HTML + hasil parse).
#[derive(Debug, Clone)]
pub struct EpisodeFetch {
    pub page: EpisodePage,
    pub downloads: Vec<DownloadRow>,
}

/// Adapter Samehadaku.
#[derive(Debug)]
pub struct Samehadaku {
    source: SourceId,
    mirrors: MirrorSet,
    http: Arc<HttpClient>,
    user_agent: String,
}

impl Samehadaku {
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

    pub async fn search(&self, query: &str) -> SourceResult<Vec<AnimeSearchResult>> {
        let base = self.base_url().await?;
        let url = Self::with_query(
            &base,
            "/wp-json/wp/v2/anime",
            &[
                ("search", query),
                ("per_page", "20"),
                ("_fields", "id,slug,link,title"),
            ],
        )?;
        let body = self.http.get_text(NAME, &url, Some(&base)).await?;
        parse::parse_search_rest(&body, &self.source)
    }

    pub async fn ongoing(&self) -> SourceResult<Vec<AnimeSearchResult>> {
        let base = self.base_url().await?;
        let url = Self::join(&base, "/anime-terbaru/")?;
        let body = self.http.get_text(NAME, &url, Some(&base)).await?;
        parse::parse_ongoing(&body, &base, &self.source)
    }

    pub async fn completed(&self) -> SourceResult<Vec<AnimeSearchResult>> {
        let base = self.base_url().await?;
        let url = Self::with_query(
            &base,
            "/daftar-anime-2/",
            &[("status", "Finished Airing"), ("order", "latest")],
        )?;
        let body = self.http.get_text(NAME, &url, Some(&base)).await?;
        parse::parse_grid(
            &body,
            &base,
            &self.source,
            Some(crate::model::AiringStatus::Completed),
        )
    }

    pub async fn detail(&self, anime_url: &str) -> SourceResult<crate::model::AnimeDetail> {
        let base = self.base_url().await?;
        let url = Url::parse(anime_url).map_err(|err| {
            SourceError::parse(NAME, "detail", "url", format!("URL tidak valid: {err}"))
        })?;
        let body = self.http.get_text(NAME, &url, Some(&base)).await?;
        parse::parse_detail(&body, &base, &self.source)
    }

    pub async fn episode(&self, episode_url: &str) -> SourceResult<EpisodeFetch> {
        let base = self.base_url().await?;
        let url = Url::parse(episode_url).map_err(|err| {
            SourceError::parse(NAME, "episode", "url", format!("URL tidak valid: {err}"))
        })?;
        let body = self.http.get_text(NAME, &url, Some(&base)).await?;
        let page = parse::parse_episode(&body, &base, &self.source)?;
        let downloads = parse::parse_downloads(&body, &base);
        Ok(EpisodeFetch { page, downloads })
    }

    /// URL embed untuk satu tombol server.
    ///
    /// Urutan: iframe di `data-embed` → `data-url` → AJAX `player_ajax` (hanya kalau
    /// tombol tidak punya embed statis, mis. shortcode lama).
    pub async fn embed_url(
        &self,
        page: &EpisodePage,
        button: &ServerButton,
        allow_ajax: bool,
    ) -> SourceResult<Option<Url>> {
        if let Some(embed) = &button.embed {
            if embed.contains("<iframe") {
                if let Some(src) = crate::embed::iframe_src(embed) {
                    if let Ok(url) = Url::parse(&src) {
                        return Ok(Some(url));
                    }
                }
            }
        }
        if let Some(data_url) = &button.data_url {
            if let Ok(url) = Url::parse(data_url.trim()) {
                return Ok(Some(url));
            }
        }
        if !allow_ajax {
            return Ok(None);
        }
        let Some(data) = page.samehada.as_ref() else {
            return Ok(None);
        };
        if data.nonce.is_empty() {
            return Ok(None);
        }
        let nonce = data.nonce.as_str();
        let (Some(post), Some(nume)) = (button.post.as_deref(), button.nume.as_deref()) else {
            return Ok(None);
        };
        let form = vec![
            ("action", "player_ajax".to_string()),
            ("nonce", nonce.to_string()),
            ("post", post.to_string()),
            ("nume", nume.to_string()),
            (
                "type",
                button
                    .data_type
                    .clone()
                    .unwrap_or_else(|| "schtml".to_string()),
            ),
        ];
        let body = self
            .http
            .post_form_text(NAME, &data.ajax_url, &form, None)
            .await?;
        Ok(crate::embed::iframe_src(&body).and_then(|src| Url::parse(&src).ok()))
    }

    /// Resolusi embed IceFile → varian HLS/MP4 dengan resolusi asli.
    pub async fn icefile_sources(&self, embed_url: &Url) -> SourceResult<Vec<StreamLink>> {
        let body = self.http.get_text(NAME, embed_url, None).await?;
        let sources = crate::embed::icefile_sources(&body);
        if sources.is_empty() {
            return Ok(Vec::new());
        }
        Ok(sources
            .into_iter()
            .map(|source| StreamLink {
                source: self.source.clone(),
                resolution: source.resolution.map(crate::model::Resolution),
                label: if source.label.is_empty() {
                    format!("IceFile {}", source.kind)
                } else {
                    source.label.clone()
                },
                server: ServerKind::Icefile,
                container: crate::embed::container_for(&source.kind, &source.link),
                headers: RequestHeaders::default(),
                url: source.link,
                stream_kind: StreamKind::Direct,
            })
            .collect())
    }

    /// Resolusi embed Blogspot → URL `googlevideo` (IP-bound, langsung diputar).
    pub async fn blogspot_media(&self, embed_url: &Url) -> SourceResult<Option<StreamLink>> {
        let body = self.http.get_text(NAME, embed_url, None).await?;
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
            container: Some(Container::Mp4),
            headers: RequestHeaders {
                // Blogger mewajibkan Referer; tanpa ini googlevideo membalas 403.
                referer: crate::embed::blogspot_referer(),
                user_agent: Some(self.user_agent.clone()),
            },
            url: media,
            stream_kind: StreamKind::Direct,
        }))
    }

    /// Link yang bisa diputar langsung dari halaman berbagi (pixeldrain).
    pub fn direct_from_share_url(
        &self,
        url: &Url,
        resolution: Option<crate::model::Resolution>,
        label: String,
    ) -> Option<StreamLink> {
        let direct = crate::embed::pixeldrain_direct(url)?;
        Some(StreamLink {
            source: self.source.clone(),
            resolution,
            label,
            server: ServerKind::Pixeldrain,
            container: None,
            headers: RequestHeaders::default(),
            url: direct,
            stream_kind: StreamKind::Direct,
        })
    }

    pub fn http(&self) -> &HttpClient {
        &self.http
    }

    pub fn source_id(&self) -> &SourceId {
        &self.source
    }

    /// Referer untuk permintaan media (halaman episode).
    pub fn referer_for(&self, page_url: &Url) -> RequestHeaders {
        RequestHeaders::with_referer(page_url.clone())
    }
}
