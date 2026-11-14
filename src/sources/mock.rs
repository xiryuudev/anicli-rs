//! Source palsu untuk test registry, orchestrator fallback, dan mode `--mock`.
//!
//! `MockSource` **tidak** melakukan I/O: semua hasil sudah ditentukan saat dibuat.
//! Dipakai untuk membuktikan urutan fallback (Tahap 7) tanpa menyentuh jaringan,
//! termasuk skenario "source error/timeout/kosong langsung".

use std::sync::Arc;

use async_trait::async_trait;

use crate::error::{SourceError, SourceResult};
use crate::model::{AnimeDetail, AnimeSearchResult, Episode, SourceId, StreamLink};

use super::{AnimeSource, Capabilities};

/// Data yang "dikembalikan" MockSource per method.
#[derive(Debug, Clone, Default)]
pub struct MockData {
    pub search: Vec<AnimeSearchResult>,
    pub ongoing: Vec<AnimeSearchResult>,
    pub completed: Vec<AnimeSearchResult>,
    pub detail: Option<AnimeDetail>,
    pub episodes: Vec<Episode>,
    pub streams: Vec<StreamLink>,
}

/// Perilaku source palsu.
#[derive(Debug, Clone)]
pub enum Behavior {
    /// Di-box supaya ukuran enum kecil (clippy::large_enum_variant).
    Ok(Box<MockData>),
    /// Semua method langsung mengembalikan error ini (simulasi source down).
    Fail(SourceError),
}

#[derive(Debug, Clone)]
pub struct MockSource {
    name: &'static str,
    base_url: String,
    capabilities: Capabilities,
    behavior: Behavior,
}

impl MockSource {
    pub fn new(name: &'static str) -> Self {
        Self {
            name,
            base_url: format!("https://{name}.test"),
            capabilities: Capabilities::all(),
            behavior: Behavior::Ok(Box::default()),
        }
    }

    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub fn with_capabilities(mut self, capabilities: Capabilities) -> Self {
        self.capabilities = capabilities;
        self
    }

    pub fn with_data(mut self, data: MockData) -> Self {
        self.behavior = Behavior::Ok(Box::new(data));
        self
    }

    pub fn failing(self, error: SourceError) -> Self {
        Self {
            behavior: Behavior::Fail(error),
            ..self
        }
    }

    fn data(&self) -> SourceResult<&MockData> {
        match &self.behavior {
            Behavior::Ok(data) => Ok(data.as_ref()),
            Behavior::Fail(error) => Err(error.clone()),
        }
    }
}

#[async_trait]
impl AnimeSource for MockSource {
    fn name(&self) -> &'static str {
        self.name
    }

    fn capabilities(&self) -> Capabilities {
        self.capabilities
    }

    async fn search(&self, _query: &str) -> SourceResult<Vec<AnimeSearchResult>> {
        if !self.capabilities.search {
            return Err(SourceError::unsupported(self.name, "search"));
        }
        Ok(self.data()?.search.clone())
    }

    async fn ongoing(&self) -> SourceResult<Vec<AnimeSearchResult>> {
        if !self.capabilities.ongoing {
            return Err(SourceError::unsupported(self.name, "ongoing"));
        }
        Ok(self.data()?.ongoing.clone())
    }

    async fn completed(&self) -> SourceResult<Vec<AnimeSearchResult>> {
        if !self.capabilities.completed {
            return Err(SourceError::unsupported(self.name, "completed"));
        }
        Ok(self.data()?.completed.clone())
    }

    async fn detail(&self, _url: &str) -> SourceResult<AnimeDetail> {
        if !self.capabilities.detail {
            return Err(SourceError::unsupported(self.name, "detail"));
        }
        self.data()?
            .detail
            .clone()
            .ok_or_else(|| SourceError::not_found(self.name, "detail anime"))
    }

    async fn episodes(&self, _anime_url: &str) -> SourceResult<Vec<Episode>> {
        if !self.capabilities.episodes {
            return Err(SourceError::unsupported(self.name, "episodes"));
        }
        Ok(self.data()?.episodes.clone())
    }

    async fn streams(&self, _episode_url: &str) -> SourceResult<Vec<StreamLink>> {
        if !self.capabilities.streams {
            return Err(SourceError::unsupported(self.name, "streams"));
        }
        Ok(self.data()?.streams.clone())
    }
}

impl MockSource {
    /// Bungkus jadi `Arc<dyn AnimeSource>` (mempermudah test registry).
    pub fn shared(self) -> Arc<dyn AnimeSource> {
        Arc::new(self)
    }

    /// SourceId untuk data yang diasosiasikan dengan mock ini.
    pub fn source_id(&self) -> SourceId {
        SourceId::new(self.name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{MediaKind, Resolution, StreamKind};

    fn url(raw: &str) -> url::Url {
        url::Url::parse(raw).unwrap()
    }

    fn sample_search() -> AnimeSearchResult {
        AnimeSearchResult {
            source: SourceId::new("mock"),
            title: "One Piece".into(),
            url: url("https://mock.test/anime/one-piece/"),
            thumbnail: None,
            kind: Some(MediaKind::Tv),
            status: None,
            score: Some(8.7),
        }
    }

    #[tokio::test]
    async fn returns_configured_data() {
        let source = MockSource::new("mock").with_data(MockData {
            search: vec![sample_search()],
            ..Default::default()
        });
        let results = source.search("one piece").await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "One Piece");
    }

    #[tokio::test]
    async fn fail_behavior_applies_to_every_method() {
        let source = MockSource::new("mock").failing(SourceError::Network {
            site: "mock",
            url: "https://mock.test".into(),
            detail: "connection refused".into(),
        });
        assert!(source.search("x").await.is_err());
        assert!(source.ongoing().await.is_err());
        assert!(source.detail("https://mock.test/a").await.is_err());
    }

    #[tokio::test]
    async fn missing_detail_is_not_found() {
        let source = MockSource::new("mock");
        let err = source.detail("https://mock.test/a").await.unwrap_err();
        assert!(matches!(err, SourceError::NotFound { .. }));
    }

    #[tokio::test]
    async fn capabilities_gate_methods() {
        let source = MockSource::new("mock").with_capabilities(Capabilities {
            streams: false,
            ..Capabilities::all()
        });
        assert!(matches!(
            source.streams("https://mock.test/ep").await.unwrap_err(),
            SourceError::Unsupported { .. }
        ));
    }

    #[tokio::test]
    async fn returns_stream_links() {
        let link = StreamLink {
            source: SourceId::new("mock"),
            resolution: Some(Resolution::P720),
            label: "720p".into(),
            server: crate::model::ServerKind::Icefile,
            container: None,
            url: url("https://mock.test/media.m3u8"),
            headers: Default::default(),
            stream_kind: StreamKind::Direct,
        };
        let source = MockSource::new("mock").with_data(MockData {
            streams: vec![link],
            ..Default::default()
        });
        let links = source.streams("https://mock.test/ep").await.unwrap();
        assert_eq!(links.len(), 1);
        assert!(links[0].is_playable_directly());
    }
}
