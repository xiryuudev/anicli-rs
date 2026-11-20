//! Test registry melalui API publik (tanpa menyentuh internals).
//!
//! Fokus: dispatch `dyn AnimeSource`, urutan source, filter capability, dan
//! pelaporan source yang diaktifkan tetapi belum diimplementasikan.

use std::sync::Arc;

use anicli_rs::config::{Config, SourceConfig};
use anicli_rs::error::{SourceError, SourceResult};
use anicli_rs::model::{
    AnimeSearchResult, Episode, MediaKind, Resolution, ServerKind, SourceId, StreamKind, StreamLink,
};
use anicli_rs::sources::mock::{MockData, MockSource};
use anicli_rs::sources::{AnimeSource, Capabilities, SourceFactory, SourceRegistry};

fn url(raw: &str) -> url::Url {
    url::Url::parse(raw).unwrap()
}

fn sample_result(source: &str, title: &str) -> AnimeSearchResult {
    AnimeSearchResult {
        source: SourceId::new(source),
        title: title.to_string(),
        url: url("https://example.test/anime/x/"),
        thumbnail: None,
        kind: Some(MediaKind::Tv),
        status: None,
        score: None,
    }
}

fn mock_with_stream(source: &'static str) -> Arc<dyn AnimeSource> {
    Arc::new(MockSource::new(source).with_data(MockData {
        search: vec![sample_result(source, "One Piece")],
        streams: vec![StreamLink {
            source: SourceId::new(source),
            resolution: Some(Resolution::P720),
            label: "720p".into(),
            server: ServerKind::Icefile,
            container: None,
            url: url("https://cdn.example.test/720.m3u8"),
            headers: Default::default(),
            stream_kind: StreamKind::Direct,
        }],
        episodes: vec![Episode {
            number: Some(1),
            label: "Ep 1".into(),
            title: None,
            url: url("https://example.test/ep/1"),
            released: None,
        }],
        ..Default::default()
    }))
}

#[tokio::test]
async fn dyn_dispatch_through_registry_works() {
    let registry = SourceRegistry::new(vec![mock_with_stream("samehadaku")]);
    let source = registry.get("samehadaku").expect("source ada");

    assert_eq!(source.name(), "samehadaku");
    assert!(source.capabilities().search);

    let results = source.search("one piece").await.unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].source.as_str(), "samehadaku");

    let episodes = source
        .episodes("https://example.test/anime/x/")
        .await
        .unwrap();
    assert_eq!(episodes.len(), 1);

    let streams = source.streams("https://example.test/ep/1").await.unwrap();
    assert_eq!(streams.len(), 1);
    assert_eq!(streams[0].resolution_label(), "720p");
}

#[tokio::test]
async fn ordered_follows_config_order_and_keeps_unknown_out() {
    let registry = SourceRegistry::new(vec![
        mock_with_stream("samehadaku"),
        mock_with_stream("otakudesu"),
        mock_with_stream("kusonime"),
    ]);

    let ordered = registry.ordered(&["kusonime", "samehadaku", "tidak-ada"]);
    let names: Vec<&str> = ordered.iter().map(|source| source.name()).collect();
    assert_eq!(names, vec!["kusonime", "samehadaku", "otakudesu"]);
}

#[tokio::test]
async fn metadata_only_source_is_excluded_from_stream_path() {
    let metadata_only =
        Arc::new(MockSource::new("kusonime").with_capabilities(Capabilities::metadata_only()));
    let registry = SourceRegistry::new(vec![mock_with_stream("samehadaku"), metadata_only]);

    let stream_sources = registry.with_streams();
    assert_eq!(stream_sources.len(), 1);
    assert_eq!(stream_sources[0].name(), "samehadaku");

    let kusonime = registry.get("kusonime").unwrap();
    let err = kusonime
        .streams("https://example.test/ep/1")
        .await
        .unwrap_err();
    assert!(matches!(err, SourceError::Unsupported { .. }));
}

#[tokio::test]
async fn failing_source_propagates_typed_error() {
    let failing = Arc::new(MockSource::new("samehadaku").failing(SourceError::Http {
        site: "samehadaku",
        url: "https://v2.samehadaku.how/".into(),
        status: 523,
    }));
    let registry = SourceRegistry::new(vec![failing]);
    let source = registry.get("samehadaku").unwrap();

    let err = source.search("x").await.unwrap_err();
    match &err {
        SourceError::Http { status, .. } => assert_eq!(*status, 523),
        other => panic!("harus Http 523, dapat {other:?}"),
    }
    assert!(err.is_retryable());
}

fn build_mock(_: &Config, source: &SourceConfig) -> SourceResult<Arc<dyn AnimeSource>> {
    Ok(Arc::new(
        MockSource::new("samehadaku").with_base_url(source.base_url.clone()),
    ))
}

static FACTORIES: &[SourceFactory] = &[SourceFactory {
    name: "samehadaku",
    build: build_mock,
}];

#[test]
fn build_uses_factories_and_reports_missing_sources() {
    let config = Config::default();
    let build = SourceRegistry::build(&config, FACTORIES);

    assert_eq!(build.registry.names(), vec!["samehadaku"]);
    assert_eq!(build.missing.len(), 11);
    assert!(build
        .warnings
        .iter()
        .any(|warning| warning.contains("belum diimplementasikan")));
}

#[test]
fn from_config_instantiates_builtin_sources() {
    // Tahap 5: ketiga source prioritas sudah terpasang, tanpa menyentuh jaringan
    // (konstruktor hanya membuat client + MirrorSet).
    let config = Config::default();
    let build = SourceRegistry::from_config(&config);
    let mut names = build.registry.names();
    names.sort_unstable();
    assert_eq!(
        names,
        vec![
            "anichin",
            "animeindo",
            "animotvslash",
            "anipm",
            "anoboy",
            "kotakanime",
            "kuramanime",
            "kusonime",
            "mkissa",
            "oploverz",
            "otakudesu",
            "samehadaku"
        ]
    );
    assert!(build.missing.is_empty(), "missing: {:?}", build.missing);
    assert!(build.warnings.is_empty(), "warnings: {:?}", build.warnings);
}

#[test]
fn registry_reports_unique_languages() {
    let config = Config::default();
    let build = SourceRegistry::from_config(&config);
    let languages = build.registry.languages();
    assert!(
        languages.contains(&anicli_rs::model::Language::Indonesian),
        "source ID harus ada: {languages:?}"
    );
    assert!(
        languages.contains(&anicli_rs::model::Language::English),
        "ani.pm harus terdaftar sebagai EN: {languages:?}"
    );
    // unik
    let mut deduped = languages.clone();
    deduped.dedup();
    assert_eq!(deduped.len(), languages.len());
}
