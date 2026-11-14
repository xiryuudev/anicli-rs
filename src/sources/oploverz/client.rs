//! Client Oploverz — JSON API `backapi.oploverz.ac` untuk katalog, HTML hanya untuk
//! stream jika diperlukan.

use std::sync::Arc;

use url::Url;

use crate::config::{Config, SourceConfig};
use crate::error::{SourceError, SourceResult};
use crate::http::{HttpClient, MirrorSet};
use crate::model::{
    Container, Episode, MediaKind, RequestHeaders, Resolution, ServerKind, SourceId, StreamKind,
    StreamLink,
};

pub const NAME: &str = "oploverz";
pub const DEFAULT_BASE: &str = "https://oploverz.diy";
pub const API_BASE: &str = "https://backapi.oploverz.ac";
pub const MARKER: &str = "Oploverz";

/// Batas halaman yang diambil untuk episode/stream (10 episode per halaman).
const MAX_EPISODE_PAGES: u32 = 20;
const MAX_STREAM_PAGES: u32 = 30;

#[derive(Debug)]
pub struct Oploverz {
    source: SourceId,
    mirrors: MirrorSet,
    http: Arc<HttpClient>,
}

impl Oploverz {
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

    fn api(path: &str) -> SourceResult<Url> {
        Url::parse(&format!("{API_BASE}{path}")).map_err(|err| {
            SourceError::parse(NAME, "api", "url", format!("URL API tidak valid: {err}"))
        })
    }

    async fn fetch_api(&self, path: &str) -> SourceResult<String> {
        let base = self.base_url().await?;
        let url = Self::api(path)?;
        self.http.get_text(NAME, &url, Some(&base)).await
    }

    fn slug_from_url(anime_url: &str) -> SourceResult<String> {
        let url = Url::parse(anime_url).map_err(|err| {
            SourceError::parse(NAME, "url", "url", format!("URL tidak valid: {err}"))
        })?;
        let segments: Vec<&str> = url.path_segments().map(|s| s.collect()).unwrap_or_default();
        if let Some(index) = segments.iter().position(|segment| *segment == "series") {
            if let Some(slug) = segments.get(index + 1) {
                return Ok((*slug).to_string());
            }
        }
        segments
            .last()
            .map(|slug| (*slug).to_string())
            .ok_or_else(|| SourceError::parse(NAME, "url", "slug", "slug tidak ditemukan"))
    }

    pub async fn search(&self, query: &str) -> SourceResult<Vec<crate::model::AnimeSearchResult>> {
        let path = format!("/api/series?page=1&q={}", urlencode(query));
        let body = self.fetch_api(&path).await?;
        super::parse::parse_series(&body, &self.source).map(|(results, _)| results)
    }

    async fn browse(&self, ongoing: bool) -> SourceResult<Vec<crate::model::AnimeSearchResult>> {
        let mut results = Vec::new();
        for page in 1..=3 {
            let body = self.fetch_api(&format!("/api/series?page={page}")).await?;
            let (items, meta) = super::parse::parse_series(&body, &self.source)?;
            for item in items {
                let is_ongoing = item.status == Some(crate::model::AiringStatus::Ongoing);
                if is_ongoing == ongoing {
                    results.push(item);
                }
            }
            if meta.last_page > 0 && page >= meta.last_page {
                break;
            }
        }
        Ok(results)
    }

    pub async fn ongoing(&self) -> SourceResult<Vec<crate::model::AnimeSearchResult>> {
        self.browse(true).await
    }

    pub async fn completed(&self) -> SourceResult<Vec<crate::model::AnimeSearchResult>> {
        self.browse(false).await
    }

    pub async fn detail(&self, anime_url: &str) -> SourceResult<crate::model::AnimeDetail> {
        let slug = Self::slug_from_url(anime_url)?;
        let body = self.fetch_api(&format!("/api/series/{slug}")).await?;
        let mut detail = super::parse::parse_series_detail(&body, &self.source)?;
        detail.url = Url::parse(anime_url).unwrap_or_else(|_| detail.url.clone());
        Ok(detail)
    }

    pub async fn episodes(&self, anime_url: &str) -> SourceResult<Vec<Episode>> {
        let slug = Self::slug_from_url(anime_url)?;
        let mut episodes = Vec::new();
        for page in 1..=MAX_EPISODE_PAGES {
            let body = self
                .fetch_api(&format!("/api/series/{slug}/episodes?page={page}"))
                .await?;
            let (items, meta) = super::parse::parse_episodes(&body, &slug, &self.source)?;
            episodes.extend(items);
            if meta.last_page > 0 && page >= meta.last_page {
                break;
            }
        }
        episodes.sort_by_key(|episode| episode.number);
        Ok(episodes)
    }

    /// Stream untuk `/series/{slug}/episode/{n}`.
    pub async fn streams(&self, episode_url: &str) -> SourceResult<Vec<StreamLink>> {
        let url = Url::parse(episode_url).map_err(|err| {
            SourceError::parse(NAME, "episode", "url", format!("URL tidak valid: {err}"))
        })?;
        let slug = Self::slug_from_url(episode_url)?;
        let number = crate::sources::util::parse_episode_number(
            url.path_segments()
                .and_then(|mut segments| segments.next_back().map(ToString::to_string))
                .unwrap_or_default()
                .as_str(),
        )
        .ok_or_else(|| {
            SourceError::parse(
                NAME,
                "episode",
                "episode_number",
                "nomor episode tidak ada di URL",
            )
        })?;

        for page in 1..=MAX_STREAM_PAGES {
            let body = self
                .fetch_api(&format!("/api/series/{slug}/episodes?page={page}"))
                .await?;
            let batch = super::parse::parse_episode_batch(&body)?;
            if let Some(entry) = batch
                .items
                .into_iter()
                .find(|entry| entry.episode_number.as_deref() == Some(number.to_string().as_str()))
            {
                return self.links_from_entries(&entry.stream_url, &url).await;
            }
            if batch.meta.last_page > 0 && page >= batch.meta.last_page {
                break;
            }
        }

        Err(SourceError::not_found(
            NAME,
            format!("episode {number} tidak ditemukan di katalog API"),
        ))
    }

    async fn links_from_entries(
        &self,
        entries: &[super::parse::StreamEntry],
        episode_url: &Url,
    ) -> SourceResult<Vec<StreamLink>> {
        let mut links = super::parse::parse_stream_entries(entries, episode_url, &self.source);
        for link in &mut links {
            let host = link.url.host_str().unwrap_or_default().to_ascii_lowercase();
            if host.contains("filedon") {
                // Filedon menyimpan MP4 presigned (kedaluwarsa ~1 jam).
                let embed = link.url.as_str().replacen("/view/", "/embed/", 1);
                if let Ok(embed_url) = Url::parse(&embed) {
                    if let Ok(body) = self.http.get_text(NAME, &embed_url, None).await {
                        if let Some(media) = crate::embed::presigned_mp4(&body) {
                            link.url = media;
                            link.stream_kind = StreamKind::Direct;
                            link.container = Some(Container::Mp4);
                            link.server = ServerKind::Other("Filedon".to_string());
                            link.headers = RequestHeaders::default();
                            continue;
                        }
                    }
                }
            } else if host.contains("blogger") {
                if let Ok(body) = self.http.get_text(NAME, &link.url, None).await {
                    if let Some(media) = crate::embed::blogspot_media(&body) {
                        link.url = media;
                        link.stream_kind = StreamKind::Direct;
                        link.container = Some(Container::Mp4);
                        link.server = ServerKind::Blogspot;
                        link.resolution =
                            crate::embed::googlevideo_resolution(&link.url).map(Resolution);
                        link.headers = RequestHeaders::default();
                    }
                }
            }
        }
        if links.is_empty() {
            return Err(SourceError::parse(
                NAME,
                "episode",
                "streamUrl",
                "API tidak memuat streamUrl untuk episode ini",
            ));
        }
        Ok(links)
    }

    pub fn http(&self) -> &HttpClient {
        &self.http
    }

    pub fn source_id(&self) -> &SourceId {
        &self.source
    }
}

fn urlencode(raw: &str) -> String {
    raw.bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (byte as char).to_string()
            }
            b' ' => "+".to_string(),
            other => format!("%{other:02X}"),
        })
        .collect()
}

/// MediaKind yang umum di Oploverz (dipakai tes/UI).
pub fn kind_from(label: &str) -> MediaKind {
    crate::sources::util::media_kind(label)
}
