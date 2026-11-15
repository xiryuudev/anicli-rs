//! Client KotakAnime: halaman HTML + AJAX `player_ajax` (multipart) + decode embed.

use std::sync::Arc;

use url::Url;

use crate::config::{Config, SourceConfig};
use crate::error::{SourceError, SourceResult};
use crate::http::{HttpClient, MirrorSet};
use crate::model::{
    AnimeDetail, AnimeSearchResult, RequestHeaders, ServerKind, SourceId, StreamKind, StreamLink,
};

pub const NAME: &str = "kotakanime";
pub const DEFAULT_BASE: &str = "https://s13.nontonanimeid.boats";
/// Marker konten katalog (portal `kotakanimeid.link` tidak memilikinya).
pub const MARKER: &str = "wp-content";
const MAX_SERVER_PROBES: usize = 3;

#[derive(Debug)]
pub struct KotakAnime {
    source: SourceId,
    mirrors: MirrorSet,
    http: Arc<HttpClient>,
}

impl KotakAnime {
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
        super::parse::parse_archive(&body, &base, &self.source)
    }

    pub async fn ongoing(&self) -> SourceResult<Vec<AnimeSearchResult>> {
        let base = self.base_url().await?;
        let url = Self::join(&base, "/anime/")?;
        let body = self.http.get_text(NAME, &url, Some(&base)).await?;
        super::parse::parse_archive(&body, &base, &self.source)
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

    /// Ambil iframe player untuk satu server lewat AJAX multipart.
    async fn player_iframe(
        &self,
        page_url: &Url,
        server: &super::parse::PlayerServer,
        ajax: &super::parse::AjaxConfig,
    ) -> SourceResult<Option<Url>> {
        if server.server_name.eq_ignore_ascii_case("streamku") {
            return Ok(None); // server iklan, bukan media
        }
        let fields = vec![
            ("action", "player_ajax".to_string()),
            ("post", server.post.clone()),
            ("nume", server.nume.clone()),
            ("serverName", server.server_name.clone()),
            ("nonce", ajax.nonce.clone()),
        ];
        let body = self
            .http
            .post_multipart_text(NAME, &ajax.ajax_url, &fields, Some(page_url))
            .await?;
        Ok(crate::embed::iframe_src(&body).and_then(|src| Url::parse(src.trim()).ok()))
    }

    pub async fn fetch_streams(&self, episode_url: &str) -> SourceResult<Vec<StreamLink>> {
        let page_url = Url::parse(episode_url).map_err(|err| {
            SourceError::parse(NAME, "episode", "url", format!("URL tidak valid: {err}"))
        })?;
        let base = self.base_url().await?;
        let body = self.http.get_text(NAME, &page_url, Some(&base)).await?;
        let page = super::parse::parse_episode(&body, &base, &self.source)?;
        let Some(ajax) = page.ajax else {
            return Err(SourceError::parse(
                NAME,
                "episode",
                "kotakajax",
                "konfigurasi AJAX (nonce) tidak ditemukan",
            ));
        };

        let mut links = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for server in page.servers.iter().take(MAX_SERVER_PROBES) {
            let Some(iframe) = self.player_iframe(&page_url, server, &ajax).await? else {
                continue;
            };
            let embed_body = match self.http.get_text(NAME, &iframe, Some(&page_url)).await {
                Ok(body) => body,
                Err(err) => {
                    tracing::debug!(target: "anicli_rs::kotakanime", %err, server = %server.label, "gagal fetch embed");
                    continue;
                }
            };

            let resolution = None;
            let label = format!("{} (KotakAnime)", server.label);
            if let Some(hls) = super::parse::decode_embed(&embed_body) {
                if let Ok(url) = Url::parse(&hls) {
                    let link = StreamLink {
                        source: self.source.clone(),
                        resolution,
                        label,
                        server: ServerKind::Other("KotakCDN".to_string()),
                        container: None,
                        url,
                        headers: RequestHeaders::default(),
                        stream_kind: StreamKind::Direct,
                    };
                    if seen.insert(link.url.to_string()) {
                        links.push(link);
                    }
                    continue;
                }
            }
            let link = StreamLink {
                source: self.source.clone(),
                resolution,
                label,
                server: ServerKind::Other("KotakEmbed".to_string()),
                container: None,
                url: iframe.clone(),
                headers: RequestHeaders {
                    referer: Some(page_url.clone()),
                    user_agent: None,
                },
                stream_kind: StreamKind::Embed,
            };
            if seen.insert(link.url.to_string()) {
                links.push(link);
            }
        }

        if links.is_empty() {
            return Err(SourceError::parse(
                NAME,
                "episode",
                "streams",
                "tidak ada server yang bisa dipakai (AJAX/decode gagal)",
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
