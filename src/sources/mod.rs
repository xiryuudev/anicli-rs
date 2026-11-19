//! Kontrak source anime + registry (DESIGN §6–§7).
//!
//! Setiap penyedia diimplementasikan sebagai modul terpisah di bawah `sources/` dan
//! hanya boleh berisi: URL builder/endpoint khusus situs (`client.rs`) + fungsi parse
//! murni (`parse.rs`) + orkestrasi implementasi trait (`mod.rs`).

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use async_trait::async_trait;

use crate::config::{Config, SourceConfig};
use crate::error::SourceResult;
use crate::model::{AnimeDetail, AnimeSearchResult, Episode, Language, StreamLink};

pub mod anichin;
pub mod animeindo;
pub mod anipm;
pub mod anoboy;
pub mod kotakanime;
pub mod kusonime;
pub mod mock;
pub mod oploverz;
pub mod otakudesu;
pub mod samehadaku;
pub mod util;

/// Kontrak wajib setiap source.
///
/// Method yang tidak didukung **wajib** mengembalikan
/// [`SourceError::Unsupported`](crate::error::SourceError::Unsupported), bukan
/// `Ok(vec![])`: "source tidak punya fitur ini" dan "hasilnya kosong" adalah dua UX
/// yang berbeda (DESIGN §6).
#[async_trait]
pub trait AnimeSource: Send + Sync {
    /// Nama stabil untuk config/log/UI, mis. `"samehadaku"`.
    fn name(&self) -> &'static str;

    /// Bahasa konten source. Default: Indonesia (mayoritas source existing),
    /// sehingga source lama tidak perlu diubah.
    fn language(&self) -> Language {
        Language::Indonesian
    }

    /// Fitur yang benar-benar didukung; UI menyembunyikan tombol yang tidak ada.
    fn capabilities(&self) -> Capabilities;

    async fn search(&self, query: &str) -> SourceResult<Vec<AnimeSearchResult>>;
    async fn ongoing(&self) -> SourceResult<Vec<AnimeSearchResult>>;
    async fn completed(&self) -> SourceResult<Vec<AnimeSearchResult>>;
    async fn detail(&self, url: &str) -> SourceResult<AnimeDetail>;
    async fn episodes(&self, anime_url: &str) -> SourceResult<Vec<Episode>>;
    async fn streams(&self, episode_url: &str) -> SourceResult<Vec<StreamLink>>;
}

/// Fitur yang didukung sebuah source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capabilities {
    pub search: bool,
    pub ongoing: bool,
    pub completed: bool,
    pub genres: bool,
    pub detail: bool,
    pub episodes: bool,
    /// `false` = download-only (mis. Kusonime). Orchestrator melewati source ini
    /// di jalur stream tanpa menunggu error.
    pub streams: bool,
    /// Hanya untuk hook download masa depan (DESIGN §12.4).
    pub batch_download: bool,
}

impl Capabilities {
    /// Source lengkap (semua fitur).
    pub const fn all() -> Self {
        Self {
            search: true,
            ongoing: true,
            completed: true,
            genres: true,
            detail: true,
            episodes: true,
            streams: true,
            batch_download: false,
        }
    }

    /// Katalog metadata + batch download, tanpa streaming per-episode (Kusonime).
    pub const fn metadata_only() -> Self {
        Self {
            search: true,
            ongoing: false,
            completed: false,
            genres: true,
            detail: true,
            episodes: false,
            streams: false,
            batch_download: true,
        }
    }

    pub const fn with_streams(mut self, value: bool) -> Self {
        self.streams = value;
        self
    }

    pub const fn with_episodes(mut self, value: bool) -> Self {
        self.episodes = value;
        self
    }
}

impl Default for Capabilities {
    fn default() -> Self {
        Self::all()
    }
}

/// Konstruktor source, didaftarkan di [`factories`].
pub struct SourceFactory {
    pub name: &'static str,
    /// `build` menerima config global + blok `[sources.<name>]` agar adapter bisa
    /// membaca `base_url`, `mirrors`, dan setelan HTTP.
    pub build: fn(&Config, &SourceConfig) -> SourceResult<Arc<dyn AnimeSource>>,
}

/// Daftar source yang dikompilasi ke binary ini.
///
/// Tahap 4 masih kosong; Tahap 5 mengisi Samehadaku/Otakudesu/Kusonime dan tahap
/// berikutnya menambah sisanya. Source yang diaktifkan di config tetapi tidak ada di
/// sini dilaporkan sebagai `missing` + warning (bukan error fatal).
pub fn factories() -> &'static [SourceFactory] {
    static FACTORIES: &[SourceFactory] = &[
        SourceFactory {
            name: anichin::NAME,
            build: anichin::build,
        },
        SourceFactory {
            name: anipm::NAME,
            build: anipm::build,
        },
        SourceFactory {
            name: anoboy::NAME,
            build: anoboy::build,
        },
        SourceFactory {
            name: animeindo::NAME,
            build: animeindo::build,
        },
        SourceFactory {
            name: samehadaku::NAME,
            build: samehadaku::build,
        },
        SourceFactory {
            name: otakudesu::NAME,
            build: otakudesu::build,
        },
        SourceFactory {
            name: kusonime::NAME,
            build: kusonime::build,
        },
        SourceFactory {
            name: oploverz::NAME,
            build: oploverz::build,
        },
        SourceFactory {
            name: kotakanime::NAME,
            build: kotakanime::build,
        },
    ];
    FACTORIES
}

/// Hasil pembangunan registry: siap pakai + catatan untuk log/UI.
#[derive(Debug, Clone)]
pub struct RegistryBuild {
    pub registry: SourceRegistry,
    /// Masalah non-fatal, mis. source enabled tetapi belum diimplementasikan.
    pub warnings: Vec<String>,
    /// Nama source enabled yang tidak punya implementasi.
    pub missing: Vec<String>,
}

/// Kumpulan source yang sudah diinstansiasi, dapat dicari berdasarkan nama.
#[derive(Clone, Default)]
pub struct SourceRegistry {
    sources: Vec<Arc<dyn AnimeSource>>,
    index: HashMap<String, usize>,
}

impl fmt::Debug for SourceRegistry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SourceRegistry")
            .field("sources", &self.names())
            .finish()
    }
}

impl SourceRegistry {
    /// Bangun dari daftar source; nama duplikat diabaikan (yang pertama menang).
    pub fn new(sources: Vec<Arc<dyn AnimeSource>>) -> Self {
        let mut registry = Self {
            sources: Vec::with_capacity(sources.len()),
            index: HashMap::new(),
        };
        for source in sources {
            if registry.index.contains_key(source.name()) {
                continue;
            }
            registry
                .index
                .insert(source.name().to_string(), registry.sources.len());
            registry.sources.push(source);
        }
        registry
    }

    /// Bahasa unik dari semua source terpasang (urut `Language::ALL`).
    pub fn languages(&self) -> Vec<crate::model::Language> {
        let mut languages: Vec<crate::model::Language> = self
            .sources
            .iter()
            .map(|source| source.language())
            .collect();
        languages.sort();
        languages.dedup();
        languages
    }

    /// Instansiasi semua source `enabled = true` di config memakai [`factories`].
    pub fn from_config(config: &Config) -> RegistryBuild {
        Self::build(config, factories())
    }

    /// Versi yang bisa menyuntik daftar factory (dipakai test).
    pub fn build(config: &Config, factories: &[SourceFactory]) -> RegistryBuild {
        let mut built: Vec<Arc<dyn AnimeSource>> = Vec::new();
        let mut warnings = Vec::new();
        let mut missing = Vec::new();

        for (name, source_config) in &config.sources {
            if !source_config.enabled {
                continue;
            }
            match factories.iter().find(|factory| factory.name == name) {
                Some(factory) => match (factory.build)(config, source_config) {
                    Ok(source) => built.push(source),
                    Err(err) => warnings.push(format!(
                        "sources.{name}: gagal di-inisialisasi: {}",
                        err.user_message()
                    )),
                },
                None => {
                    missing.push(name.clone());
                    warnings.push(format!(
                        "sources.{name}: diaktifkan di config tetapi belum diimplementasikan \
                         (diabaikan; implementasi menyusul)"
                    ));
                }
            }
        }

        RegistryBuild {
            registry: Self::new(built),
            warnings,
            missing,
        }
    }

    pub fn len(&self) -> usize {
        self.sources.len()
    }

    pub fn is_empty(&self) -> bool {
        self.sources.is_empty()
    }

    pub fn names(&self) -> Vec<&'static str> {
        self.sources.iter().map(|source| source.name()).collect()
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn AnimeSource>> {
        self.index
            .get(name)
            .and_then(|index| self.sources.get(*index))
            .cloned()
    }

    pub fn sources(&self) -> &[Arc<dyn AnimeSource>] {
        &self.sources
    }

    /// Urutkan sesuai `order` (nama yang tidak ada/duplikat dilewati), lalu tambahkan
    /// sisa source di akhir supaya tidak ada source yang "hilang" dari UI.
    pub fn ordered(&self, order: &[&str]) -> Vec<Arc<dyn AnimeSource>> {
        let mut result: Vec<Arc<dyn AnimeSource>> = Vec::new();
        for name in order {
            if result.iter().any(|source| source.name() == *name) {
                continue;
            }
            if let Some(source) = self.get(name) {
                result.push(source);
            }
        }
        for source in &self.sources {
            if !result
                .iter()
                .any(|existing| existing.name() == source.name())
            {
                result.push(source.clone());
            }
        }
        result
    }

    /// Nama source yang punya fitur tertentu (mis. hanya yang punya `streams`).
    pub fn with_streams(&self) -> Vec<Arc<dyn AnimeSource>> {
        self.sources
            .iter()
            .filter(|source| source.capabilities().streams)
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::SourceError;
    use crate::model::SourceId;

    fn mock(name: &'static str) -> Arc<dyn AnimeSource> {
        Arc::new(mock::MockSource::new(name))
    }

    #[test]
    fn registry_indexes_by_name_and_skips_duplicates() {
        let registry = SourceRegistry::new(vec![
            mock("samehadaku"),
            mock("samehadaku"),
            mock("otakudesu"),
        ]);
        assert_eq!(registry.len(), 2);
        assert_eq!(registry.names(), vec!["samehadaku", "otakudesu"]);
        assert!(registry.get("samehadaku").is_some());
        assert!(registry.get("nope").is_none());
    }

    #[test]
    fn ordered_respects_wanted_order_then_appends_rest() {
        let registry = SourceRegistry::new(vec![
            mock("samehadaku"),
            mock("otakudesu"),
            mock("kusonime"),
        ]);
        let ordered = registry.ordered(&["kusonime", "tidak-ada", "kusonime", "samehadaku"]);
        let names: Vec<&str> = ordered.iter().map(|source| source.name()).collect();
        assert_eq!(names, vec!["kusonime", "samehadaku", "otakudesu"]);
    }

    #[test]
    fn with_streams_filters_metadata_only_sources() {
        let metadata_only = Arc::new(
            mock::MockSource::new("kusonime").with_capabilities(Capabilities::metadata_only()),
        );
        let registry = SourceRegistry::new(vec![mock("samehadaku"), metadata_only]);
        let names: Vec<&str> = registry
            .with_streams()
            .iter()
            .map(|source| source.name())
            .collect();
        assert_eq!(names, vec!["samehadaku"]);
    }

    #[test]
    fn build_reports_enabled_sources_without_implementation() {
        let config = Config::default();
        let build = SourceRegistry::build(&config, &[]);
        assert!(build.registry.is_empty());
        assert_eq!(build.missing.len(), 9);
        assert!(build
            .warnings
            .iter()
            .all(|warning| warning.contains("belum diimplementasikan")));
    }

    #[test]
    fn build_instantiates_from_a_factory() {
        fn build_mock(_: &Config, source: &SourceConfig) -> SourceResult<Arc<dyn AnimeSource>> {
            Ok(Arc::new(
                mock::MockSource::new("samehadaku").with_base_url(source.base_url.clone()),
            ))
        }

        static FACTORIES: &[SourceFactory] = &[SourceFactory {
            name: "samehadaku",
            build: build_mock,
        }];

        let config = Config::default();
        let build = SourceRegistry::build(&config, FACTORIES);
        assert_eq!(build.registry.len(), 1);
        assert_eq!(build.registry.names(), vec!["samehadaku"]);
        assert_eq!(build.missing.len(), 8);
    }

    #[test]
    fn build_skips_disabled_sources() {
        let mut config = Config::default();
        config.sources.get_mut("samehadaku").unwrap().enabled = false;
        let build = SourceRegistry::build(&config, &[]);
        assert!(!build.missing.contains(&"samehadaku".to_string()));
        assert_eq!(build.missing.len(), 8);
    }

    #[tokio::test]
    async fn capabilities_gate_unsupported_methods() {
        let source =
            mock::MockSource::new("kusonime").with_capabilities(Capabilities::metadata_only());
        let err = source.streams("https://example.test/ep").await.unwrap_err();
        assert!(matches!(err, SourceError::Unsupported { .. }));
    }

    #[test]
    fn source_id_is_used_in_results() {
        let id = SourceId::new("samehadaku");
        assert_eq!(id.as_str(), "samehadaku");
        assert_eq!(id.to_string(), "samehadaku");
    }
}
