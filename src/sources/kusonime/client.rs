//! Client Kusonime: REST WordPress untuk discovery + HTML untuk tabel unduhan.

use std::sync::Arc;

use url::Url;

use crate::config::{Config, SourceConfig};
use crate::error::{SourceError, SourceResult};
use crate::http::{HttpClient, MirrorSet};
use crate::model::{AnimeDetail, AnimeSearchResult, SourceId};

pub const NAME: &str = "kusonime";
pub const DEFAULT_BASE: &str = "https://kusonime.com";
/// Marker domain: hanya dipakai untuk membedakan domain parkir/clone.
pub const MARKER: &str = "kusonime";

#[derive(Debug)]
pub struct Kusonime {
    source: SourceId,
    mirrors: MirrorSet,
    http: Arc<HttpClient>,
}

impl Kusonime {
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

    /// Pencarian lewat REST (stabil); kalau REST gagal, fallback ke HTML `?s=`.
    pub async fn search(&self, query: &str) -> SourceResult<Vec<AnimeSearchResult>> {
        let base = self.base_url().await?;
        let url = Self::with_query(
            &base,
            "/wp-json/wp/v2/posts",
            &[
                ("search", query),
                ("per_page", "20"),
                ("_fields", "id,link,title,excerpt"),
            ],
        )?;

        match self.http.get_text(NAME, &url, Some(&base)).await {
            Ok(body) => super::parse::parse_rest(&body, &self.source),
            Err(err) => {
                tracing::warn!(
                    target: "anicli_rs::kusonime",
                    %err,
                    "REST search gagal, fallback ke HTML"
                );
                let html_url = Self::with_query(&base, "/", &[("s", query)])?;
                let body = self.http.get_text(NAME, &html_url, Some(&base)).await?;
                super::parse::parse_cards(&body, &base, &self.source)
            }
        }
    }

    /// Detail seri. Memakai client **tanpa redirect** untuk menangkap soft-404
    /// (slug salah → 404 + `Location: /` → homepage bila redirect diikuti).
    pub async fn detail(&self, detail_url: &str) -> SourceResult<AnimeDetail> {
        let base = self.base_url().await?;
        let url = Url::parse(detail_url).map_err(|err| {
            SourceError::parse(NAME, "detail", "url", format!("URL tidak valid: {err}"))
        })?;
        let body = self
            .http
            .get_text_no_redirect(NAME, &url, Some(&base))
            .await?;
        let mut detail = super::parse::parse_detail(&body, &base, &self.source)?;
        detail.url = url;
        Ok(detail)
    }

    /// Daftar post terbaru (untuk browse "latest" di UI), bukan bagian trait.
    pub async fn latest(&self) -> SourceResult<Vec<AnimeSearchResult>> {
        let base = self.base_url().await?;
        let url = Self::with_query(
            &base,
            "/wp-json/wp/v2/posts",
            &[
                ("per_page", "20"),
                ("orderby", "date"),
                ("order", "desc"),
                ("_fields", "id,link,title,excerpt"),
            ],
        )?;
        let body = self.http.get_text(NAME, &url, Some(&base)).await?;
        super::parse::parse_rest(&body, &self.source)
    }

    pub fn source_id(&self) -> &SourceId {
        &self.source
    }

    pub fn http(&self) -> &HttpClient {
        &self.http
    }
}
