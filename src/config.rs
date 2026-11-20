//! Pemuatan, validasi, migrasi, dan penyimpanan config TOML (DESIGN §14).
//!
//! Aturan penting:
//! - File yang hilang dibuat otomatis dari [`CONFIG_TEMPLATE`] (file contoh lengkap
//!   berkomentar), sehingga user baru tidak pernah melihat error "config tidak ada".
//! - Serialisasi ke disk bersifat atomik (tulis `.tmp` → `rename`) supaya config
//!   tidak korup kalau proses mati di tengah penulisan.
//! - Field tidak dikenal **diabaikan** (toleran versi), tetapi nilai yang salah tipe
//!   atau salah enum tetap error dengan path key TOML-nya.
//! - `config_version` lebih baru dari yang didukung → error, bukan ditebak.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::model::Resolution;
use crate::paths::Paths;

/// Template config yang dikirim di binary (isinya sama dengan `config.example.toml`).
pub const CONFIG_TEMPLATE: &str = include_str!("../config.example.toml");

/// Versi skema config yang dikenal binary ini.
pub const CURRENT_CONFIG_VERSION: u32 = 1;

/// Nama-nama level log yang diterima (selain sintaks `EnvFilter`).
pub const LOG_LEVELS: &[&str] = &["trace", "debug", "info", "warn", "error", "off"];

// =============================================================================
// Tipe config
// =============================================================================

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub config_version: u32,
    pub general: GeneralConfig,
    pub playback: PlaybackConfig,
    pub http: HttpConfig,
    pub storage: StorageConfig,
    pub ui: UiConfig,
    pub logging: LoggingConfig,
    pub sources: BTreeMap<String, SourceConfig>,
    pub update: UpdateConfig,
}

/// Tambahkan source baru dari default bila belum ada di config user.
///
/// Mengembalikan nama source yang ditambahkan (untuk warning + disimpan).
fn merge_new_sources(config: &mut Config) -> Vec<String> {
    // Kalau user sengaja menonaktifkan semuanya, jangan tambah source baru:
    // biarkan validasi yang memberi tahu (menghindari perilaku mengejutkan).
    if !config.sources.values().any(|source| source.enabled) {
        return Vec::new();
    }
    let defaults = Config::default();
    let mut added = Vec::new();
    for (name, default_source) in &defaults.sources {
        if !config.sources.contains_key(name) {
            config.sources.insert(name.clone(), default_source.clone());
            added.push(name.clone());
        }
    }
    if added.is_empty() {
        return added;
    }
    added.sort();
    for name in &added {
        if config
            .general
            .fallback_order
            .iter()
            .any(|existing| existing == name)
        {
            continue;
        }
        match config
            .general
            .fallback_order
            .iter()
            .position(|existing| existing == "kusonime")
        {
            Some(index) => config.general.fallback_order.insert(index, name.clone()),
            None => config.general.fallback_order.push(name.clone()),
        }
    }
    added
}

impl Default for Config {
    fn default() -> Self {
        Self {
            update: UpdateConfig::default(),
            config_version: CURRENT_CONFIG_VERSION,
            general: GeneralConfig::default(),
            playback: PlaybackConfig::default(),
            http: HttpConfig::default(),
            storage: StorageConfig::default(),
            ui: UiConfig::default(),
            logging: LoggingConfig::default(),
            sources: default_sources(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GeneralConfig {
    pub default_source: String,
    pub fallback_order: Vec<String>,
    pub language: String,
    /// Filter bahasa source: `"all"`, `"id"`, `"en"`, atau `"jp"`.
    #[serde(default = "default_language_filter")]
    pub language_filter: String,

    /// Cakupan pencarian (bisa diubah dengan tombol `S` di layar Cari).
    #[serde(default)]
    pub search_scope: SearchScope,
}

fn default_language_filter() -> String {
    "all".to_string()
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            search_scope: SearchScope::All,
            language_filter: default_language_filter(),
            default_source: "samehadaku".to_string(),
            fallback_order: vec![
                "samehadaku".to_string(),
                "otakudesu".to_string(),
                "oploverz".to_string(),
                "animeindo".to_string(),
                "anoboy".to_string(),
                "kotakanime".to_string(),
                "anichin".to_string(),
                "anipm".to_string(),
                "mkissa".to_string(),
                "animotvslash".to_string(),
                "kuramanime".to_string(),
                "kusonime".to_string(),
            ],
            language: "id".to_string(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Player {
    #[default]
    Mpv,
    Vlc,
}

impl fmt::Display for Player {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Mpv => f.write_str("mpv"),
            Self::Vlc => f.write_str("vlc"),
        }
    }
}

impl std::str::FromStr for Player {
    type Err = ConfigError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "mpv" => Ok(Self::Mpv),
            "vlc" => Ok(Self::Vlc),
            other => Err(ConfigError::Invalid(vec![format!(
                "playback.player: {other:?} tidak dikenal (pilih \"mpv\" atau \"vlc\")"
            )])),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ResolutionPolicy {
    #[default]
    NearestHigher,
    NearestLower,
    ExactOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum EmbedStrategy {
    #[default]
    Ytdl,
    Browser,
    Skip,
}

impl fmt::Display for ResolutionPolicy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            Self::NearestHigher => "nearest_higher",
            Self::NearestLower => "nearest_lower",
            Self::ExactOnly => "exact_only",
        };
        f.write_str(text)
    }
}

impl fmt::Display for EmbedStrategy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            Self::Ytdl => "ytdl",
            Self::Browser => "browser",
            Self::Skip => "skip",
        };
        f.write_str(text)
    }
}

/// Setelan fitur cek update.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UpdateConfig {
    /// Cek versi terbaru saat aplikasi mulai (non-blocking).
    pub check_on_start: bool,
    /// Cache hasil cek (jam) supaya tidak memanggil API setiap kali start.
    pub check_interval_hours: u64,
}

impl Default for UpdateConfig {
    fn default() -> Self {
        Self {
            check_on_start: true,
            check_interval_hours: 12,
        }
    }
}

/// Cakupan pencarian: semua source aktif, atau hanya source yang dipilih.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SearchScope {
    /// Cari paralel di semua source aktif (fallback order).
    #[default]
    All,
    /// Hanya source yang sedang aktif (tab atas).
    Selected,
}

impl std::fmt::Display for SearchScope {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::All => write!(formatter, "semua source"),
            Self::Selected => write!(formatter, "source aktif"),
        }
    }
}

/// Aplikasi player Android yang dipakai saat berjalan di Termux.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AndroidPlayer {
    /// Intent VIEW tanpa filter paket: aplikasi default Android yang memutar.
    #[default]
    Auto,
    /// mpv-android (`is.xyz.mpv`).
    Mpv,
    /// VLC Android (`org.videolan.vlc`).
    Vlc,
}

impl std::fmt::Display for AndroidPlayer {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Auto => write!(formatter, "auto"),
            Self::Mpv => write!(formatter, "mpv-android"),
            Self::Vlc => write!(formatter, "vlc-android"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PlaybackConfig {
    pub player: Player,
    pub mpv_path: String,
    pub vlc_path: String,
    pub default_resolution: Resolution,
    pub resolution_policy: ResolutionPolicy,
    pub prefer_mkv: bool,
    pub resume_last_episode: bool,
    pub embed_strategy: EmbedStrategy,
    pub foreground: bool,
    pub extra_args: Vec<String>,
    /// Hanya dipakai di Android/Termux (lihat `player::android`).
    #[serde(default)]
    pub android_player: AndroidPlayer,
}

impl Default for PlaybackConfig {
    fn default() -> Self {
        Self {
            player: Player::Mpv,
            mpv_path: String::new(),
            vlc_path: String::new(),
            default_resolution: Resolution::P720,
            resolution_policy: ResolutionPolicy::NearestHigher,
            prefer_mkv: false,
            resume_last_episode: true,
            embed_strategy: EmbedStrategy::Ytdl,
            foreground: false,
            extra_args: Vec::new(),
            android_player: AndroidPlayer::Auto,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HttpConfig {
    pub request_delay_ms: u64,
    pub jitter_ms: u64,
    pub max_concurrent: usize,
    pub connect_timeout_secs: u64,
    pub timeout_secs: u64,
    pub retries: u32,
    pub user_agent: String,
    pub mirror_cache_ttl_secs: u64,
    /// Opsional; dikosongkan secara default dan tidak pernah ditulis ke log.
    pub cookie_header: Option<String>,
}

impl Default for HttpConfig {
    fn default() -> Self {
        Self {
            request_delay_ms: 500,
            jitter_ms: 300,
            max_concurrent: 6,
            connect_timeout_secs: 8,
            timeout_secs: 20,
            retries: 2,
            user_agent: "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) \
                         Chrome/126.0.0.0 Safari/537.36"
                .to_string(),
            mirror_cache_ttl_secs: 3600,
            cookie_header: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StorageConfig {
    pub history_enabled: bool,
    pub max_history: usize,
    pub max_bookmarks: usize,
    pub data_dir: String,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            history_enabled: true,
            max_history: 500,
            max_bookmarks: 200,
            data_dir: String::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UiConfig {
    pub theme: String,
    pub mouse: bool,
    pub merge_browse: bool,
    pub image_preview: bool,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            theme: "default".to_string(),
            mouse: false,
            merge_browse: false,
            image_preview: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LoggingConfig {
    pub level: String,
    pub max_days: u32,
    pub to_stderr: bool,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: "info".to_string(),
            max_days: 7,
            to_stderr: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SourceConfig {
    pub enabled: bool,
    pub base_url: String,
    pub mirrors: Vec<String>,
}

impl Default for SourceConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            base_url: String::new(),
            mirrors: Vec::new(),
        }
    }
}

fn default_sources() -> BTreeMap<String, SourceConfig> {
    let mut sources = BTreeMap::new();
    sources.insert(
        "samehadaku".to_string(),
        SourceConfig {
            enabled: true,
            base_url: "https://v2.samehadaku.how".to_string(),
            mirrors: Vec::new(),
        },
    );
    sources.insert(
        "otakudesu".to_string(),
        SourceConfig {
            enabled: true,
            base_url: "https://otakudesu.blog".to_string(),
            mirrors: Vec::new(),
        },
    );
    sources.insert(
        "kusonime".to_string(),
        SourceConfig {
            enabled: true,
            base_url: "https://kusonime.com".to_string(),
            mirrors: Vec::new(),
        },
    );
    sources.insert(
        "kotakanime".to_string(),
        SourceConfig {
            enabled: true,
            base_url: "https://s13.nontonanimeid.boats".to_string(),
            mirrors: Vec::new(),
        },
    );
    sources.insert(
        "anichin".to_string(),
        SourceConfig {
            enabled: true,
            base_url: "https://anichin.moe".to_string(),
            mirrors: vec![
                "https://www.anichin.moe".to_string(),
                "https://anichin.care".to_string(),
            ],
        },
    );
    sources.insert(
        "anipm".to_string(),
        SourceConfig {
            enabled: true,
            base_url: "https://ani.pm".to_string(),
            mirrors: Vec::new(),
        },
    );
    sources.insert(
        "mkissa".to_string(),
        SourceConfig {
            enabled: true,
            base_url: "https://mkissa.to".to_string(),
            mirrors: Vec::new(),
        },
    );
    sources.insert(
        "animotvslash".to_string(),
        SourceConfig {
            enabled: true,
            base_url: "https://animotvslash.org".to_string(),
            mirrors: Vec::new(),
        },
    );
    sources.insert(
        "kuramanime".to_string(),
        SourceConfig {
            enabled: true,
            base_url: "https://v20.kuramanime.ing".to_string(),
            mirrors: Vec::new(),
        },
    );
    sources.insert(
        "anoboy".to_string(),
        SourceConfig {
            enabled: true,
            base_url: "https://anoboy.id".to_string(),
            mirrors: vec!["https://anoboy.nl".to_string()],
        },
    );
    sources.insert(
        "animeindo".to_string(),
        SourceConfig {
            enabled: true,
            base_url: "https://anime-indo.lol".to_string(),
            mirrors: Vec::new(),
        },
    );
    sources.insert(
        "oploverz".to_string(),
        SourceConfig {
            enabled: true,
            base_url: "https://oploverz.diy".to_string(),
            mirrors: vec!["https://oploverz.site".to_string()],
        },
    );
    sources
}

// =============================================================================
// Error & hasil load
// =============================================================================

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("gagal membaca {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("gagal menulis {path}: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("TOML tidak valid di {path}:\n{source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },

    #[error("gagal men-serialisasi config ke TOML: {0}")]
    Serialize(String),

    #[error("konfigurasi tidak valid:\n{}", format_issues(.0))]
    Invalid(Vec<String>),

    #[error(
        "versi config {found} lebih baru dari yang didukung binary ini ({supported}). \
         Perbarui anicli-rs (`cargo install --path .` / unduh rilis terbaru)."
    )]
    VersionTooNew { found: u32, supported: u32 },
}

fn format_issues(issues: &[String]) -> String {
    issues
        .iter()
        .map(|issue| format!("  - {issue}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Hasil [`load`]: config yang sudah divalidasi + catatan untuk ditampilkan/di-log.
#[derive(Debug, Clone)]
pub struct LoadedConfig {
    pub config: Config,
    /// `true` kalau file tidak ada dan dibuat dari template.
    pub created: bool,
    /// Masalah non-fatal (key diabaikan, nilai mencurigakan, dll.).
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InitOutcome {
    Created(PathBuf),
    AlreadyExists(PathBuf),
    Overwritten(PathBuf),
}

impl InitOutcome {
    pub fn path(&self) -> &Path {
        match self {
            Self::Created(path) | Self::AlreadyExists(path) | Self::Overwritten(path) => path,
        }
    }
}

/// Override dari CLI (menang atas file config).
#[derive(Debug, Clone, Default)]
pub struct Overrides {
    pub source: Option<String>,
    pub resolution: Option<Resolution>,
    pub player: Option<Player>,
}

// =============================================================================
// Load / init / save
// =============================================================================

/// Muat config dari `paths.config_file`. Membuat file dari template bila belum ada.
pub fn load(paths: &Paths) -> Result<LoadedConfig, ConfigError> {
    let path = &paths.config_file;
    let mut created = false;

    if !path.exists() {
        paths.ensure().map_err(|err| ConfigError::Write {
            path: paths.config_dir(),
            source: std::io::Error::other(err.to_string()),
        })?;
        write_atomic(path, CONFIG_TEMPLATE)?;
        created = true;
    }

    let raw = std::fs::read_to_string(path).map_err(|source| ConfigError::Read {
        path: path.clone(),
        source,
    })?;

    let mut config: Config = toml::from_str(&raw).map_err(|source| ConfigError::Parse {
        path: path.clone(),
        source,
    })?;

    let mut warnings = migrate(&mut config)?;
    let added = merge_new_sources(&mut config);
    if !added.is_empty() {
        warnings.push(format!(
            "source baru ditambahkan ke config: {} (hapus blok [sources.*] untuk menonaktifkan)",
            added.join(", ")
        ));
        // Simpan sekali supaya migrasi tidak berulang tiap start.
        config.save(path)?;
    }
    warnings.extend(validate(&config)?);

    Ok(LoadedConfig {
        config,
        created,
        warnings,
    })
}

/// Tulis template ke lokasi config tanpa menyentuh file yang sudah ada (kecuali `force`).
pub fn init(paths: &Paths, force: bool) -> Result<InitOutcome, ConfigError> {
    let path = &paths.config_file;
    paths.ensure().map_err(|err| ConfigError::Write {
        path: paths.config_dir(),
        source: std::io::Error::other(err.to_string()),
    })?;

    if path.exists() {
        if !force {
            return Ok(InitOutcome::AlreadyExists(path.clone()));
        }
        write_atomic(path, CONFIG_TEMPLATE)?;
        return Ok(InitOutcome::Overwritten(path.clone()));
    }

    write_atomic(path, CONFIG_TEMPLATE)?;
    Ok(InitOutcome::Created(path.clone()))
}

impl Config {
    /// Serialisasi config ke TOML (komentar template tidak ikut, tentu saja).
    pub fn to_toml(&self) -> Result<String, ConfigError> {
        toml::to_string_pretty(self).map_err(|err| ConfigError::Serialize(err.to_string()))
    }

    /// Simpan atomik ke `path`.
    pub fn save(&self, path: &Path) -> Result<(), ConfigError> {
        let text = self.to_toml()?;
        write_atomic(path, &text)
    }

    /// Terapkan override CLI; mengembalikan warning untuk nilai yang diabaikan.
    pub fn apply_overrides(&mut self, overrides: &Overrides) -> Result<Vec<String>, ConfigError> {
        let mut warnings = Vec::new();

        if let Some(source) = &overrides.source {
            match self.sources.get(source) {
                Some(cfg) if cfg.enabled => {
                    self.general.default_source = source.clone();
                    if !self.general.fallback_order.iter().any(|s| s == source) {
                        self.general.fallback_order.insert(0, source.clone());
                    }
                }
                Some(_) => {
                    return Err(ConfigError::Invalid(vec![format!(
                        "--source {source:?}: source ada di config tetapi `enabled = false`"
                    )]));
                }
                None => {
                    return Err(ConfigError::Invalid(vec![format!(
                        "--source {source:?} tidak dikenal. Source yang tersedia: {}",
                        self.enabled_source_names().join(", ")
                    )]));
                }
            }
        }

        if let Some(resolution) = overrides.resolution {
            self.playback.default_resolution = resolution;
        }

        if let Some(player) = overrides.player {
            self.playback.player = player;
        }

        if self.sources.is_empty() {
            warnings.push("config tidak punya blok [sources.*]".to_string());
        }

        Ok(warnings)
    }

    pub fn enabled_source_names(&self) -> Vec<&str> {
        self.sources
            .iter()
            .filter(|(_, cfg)| cfg.enabled)
            .map(|(name, _)| name.as_str())
            .collect()
    }

    /// Urutan source yang benar-benar dipakai: default dulu, lalu fallback yang enabled.
    pub fn playback_order(&self) -> Vec<&str> {
        let mut order: Vec<&str> = Vec::new();
        let candidates =
            std::iter::once(&self.general.default_source).chain(self.general.fallback_order.iter());
        for name in candidates {
            let name = name.as_str();
            if order.contains(&name) {
                continue;
            }
            if self.sources.get(name).is_some_and(|cfg| cfg.enabled) {
                order.push(name);
            }
        }
        order
    }
}

// =============================================================================
// Migrasi & validasi
// =============================================================================

fn migrate(config: &mut Config) -> Result<Vec<String>, ConfigError> {
    let mut warnings = Vec::new();

    if config.config_version > CURRENT_CONFIG_VERSION {
        return Err(ConfigError::VersionTooNew {
            found: config.config_version,
            supported: CURRENT_CONFIG_VERSION,
        });
    }

    if config.config_version == 0 {
        warnings.push("config_version tidak ada; dianggap versi 1 (format lama)".to_string());
        config.config_version = 1;
    }

    // Tempat migrasi versi berikutnya:
    // if config.config_version < 2 { ...; config.config_version = 2; }

    Ok(warnings)
}

/// Validasi semantik. Mengembalikan warning; error dikumpulkan sekaligus supaya
/// user bisa membetulkan beberapa key dalam satu kali edit.
pub fn validate(config: &Config) -> Result<Vec<String>, ConfigError> {
    let mut errors: Vec<String> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    // --- sources ---
    if config.sources.is_empty() {
        errors.push("sections.sources: tidak ada satupun `[sources.<nama>]`".to_string());
    }
    let enabled = config.enabled_source_names();
    if enabled.is_empty() && !config.sources.is_empty() {
        errors.push(
            "sources: tidak ada source dengan `enabled = true` (minimal satu harus aktif)"
                .to_string(),
        );
    }

    for (name, source) in &config.sources {
        if source.base_url.trim().is_empty() {
            warnings.push(format!("sources.{name}.base_url kosong"));
        } else if url::Url::parse(source.base_url.trim()).is_err() {
            errors.push(format!(
                "sources.{name}.base_url bukan URL yang valid: {:?}",
                source.base_url
            ));
        } else if source.base_url.trim().starts_with("http://") {
            warnings.push(format!(
                "sources.{name}.base_url memakai http:// (tidak terenkripsi): {:?}",
                source.base_url
            ));
        }
        for (index, mirror) in source.mirrors.iter().enumerate() {
            if url::Url::parse(mirror).is_err() {
                errors.push(format!(
                    "sources.{name}.mirrors[{index}] bukan URL yang valid: {mirror:?}"
                ));
            }
        }
    }

    // --- general ---
    if !config.sources.is_empty() && !config.sources.contains_key(&config.general.default_source) {
        errors.push(format!(
            "general.default_source: {:?} tidak ada di [sources.*] (tersedia: {})",
            config.general.default_source,
            config
                .sources
                .keys()
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if !config.general.default_source.trim().is_empty()
        && config
            .sources
            .get(&config.general.default_source)
            .is_some_and(|source| !source.enabled)
    {
        errors.push(format!(
            "general.default_source: {:?} ada tetapi `enabled = false`",
            config.general.default_source
        ));
    }

    let mut seen: Vec<&str> = Vec::new();
    for name in &config.general.fallback_order {
        if !config.sources.contains_key(name) {
            warnings.push(format!(
                "general.fallback_order: {name:?} tidak dikenal, diabaikan"
            ));
        } else if config
            .sources
            .get(name)
            .is_some_and(|source| !source.enabled)
        {
            warnings.push(format!(
                "general.fallback_order: {name:?} ada tetapi `enabled = false`, diabaikan"
            ));
        } else if seen.contains(&name.as_str()) {
            warnings.push(format!(
                "general.fallback_order: {name:?} duplikat, diabaikan"
            ));
        } else {
            seen.push(name);
        }
    }
    if !config.general.fallback_order.is_empty()
        && !seen.contains(&config.general.default_source.as_str())
    {
        warnings.push(format!(
            "general.fallback_order tidak memuat default_source {:?}; source itu tetap dicoba lebih dulu",
            config.general.default_source
        ));
    }

    if config.general.language != "id" {
        warnings.push(format!(
            "general.language: {:?} belum didukung, memakai \"id\"",
            config.general.language
        ));
    }

    // --- http ---
    if config.http.max_concurrent == 0 {
        errors.push("http.max_concurrent harus >= 1".to_string());
    }
    if config.http.timeout_secs == 0 {
        errors.push("http.timeout_secs harus > 0".to_string());
    }
    if config.http.connect_timeout_secs == 0 {
        errors.push("http.connect_timeout_secs harus > 0".to_string());
    }
    if config.http.request_delay_ms < 100 {
        warnings.push(format!(
            "http.request_delay_ms = {} terlalu kecil; situs bisa membalas 429/diblokir",
            config.http.request_delay_ms
        ));
    }
    if config.http.user_agent.trim().is_empty() {
        errors.push(
            "http.user_agent kosong; beberapa source (Samehadaku) membalas 403 untuk UA kosong"
                .to_string(),
        );
    }

    // --- storage ---
    if config.storage.max_history == 0 {
        errors.push("storage.max_history harus > 0".to_string());
    }
    if config.storage.max_bookmarks == 0 {
        warnings.push("storage.max_bookmarks = 0: bookmark tidak bisa disimpan".to_string());
    }

    // --- ui ---
    if !matches!(config.ui.theme.as_str(), "default" | "dark") {
        warnings.push(format!(
            "ui.theme: {:?} tidak dikenal (dipakai \"default\")",
            config.ui.theme
        ));
    }
    if config.ui.image_preview && !cfg!(feature = "image-preview") {
        warnings.push(
            "ui.image_preview = true tetapi binary ini dibuild tanpa `--features image-preview`"
                .to_string(),
        );
    }

    // --- logging ---
    if !LOG_LEVELS.contains(&config.logging.level.trim().to_ascii_lowercase().as_str()) {
        warnings.push(format!(
            "logging.level: {:?} bukan level standar (trace/debug/info/warn/error); \
             akan dicoba sebagai direktif EnvFilter",
            config.logging.level
        ));
    }

    if errors.is_empty() {
        Ok(warnings)
    } else {
        Err(ConfigError::Invalid(errors))
    }
}

// =============================================================================
// Penulisan atomik
// =============================================================================

pub(crate) fn write_atomic(path: &Path, contents: &str) -> Result<(), ConfigError> {
    let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    else {
        return Err(ConfigError::Write {
            path: path.to_path_buf(),
            source: std::io::Error::new(std::io::ErrorKind::InvalidInput, "path tanpa induk"),
        });
    };
    std::fs::create_dir_all(parent).map_err(|source| ConfigError::Write {
        path: parent.to_path_buf(),
        source,
    })?;

    replace_contents(path, contents).map_err(|source| ConfigError::Write {
        path: path.to_path_buf(),
        source,
    })
}

/// POSIX: tulis ke `.tmp` lalu rename (atomik).
#[cfg(not(windows))]
fn replace_contents(path: &Path, contents: &str) -> std::io::Result<()> {
    let tmp = path.with_extension("toml.tmp");
    let write = || -> std::io::Result<()> {
        std::fs::write(&tmp, contents)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))?;
        }
        let file = std::fs::File::open(&tmp)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&tmp, path)
    };
    write().inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

/// Windows: temp+rename dan copy terus gagal `Access is denied` karena handle
/// antivirus/Defender pada file baru. Tulis langsung ke tujuan (truncate + fsync);
/// trade-off atomicity diterima agar config/bookmark/riwayat selalu bisa disimpan.
#[cfg(windows)]
fn replace_contents(path: &Path, contents: &str) -> std::io::Result<()> {
    use std::io::Write as _;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)?;
    file.write_all(contents.as_bytes())?;
    file.sync_all()?;
    Ok(())
}

// =============================================================================
// Test
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_paths() -> (tempfile::TempDir, Paths) {
        let tmp = tempfile::TempDir::new().unwrap();
        let paths = Paths::resolve(
            Some(tmp.path().join("config").join("config.toml")),
            Some(tmp.path().join("data")),
        )
        .unwrap();
        (tmp, paths)
    }

    fn write_config(paths: &Paths, text: &str) {
        std::fs::create_dir_all(paths.config_dir()).unwrap();
        std::fs::write(&paths.config_file, text).unwrap();
    }

    #[test]
    fn template_parses() {
        let parsed: Config = toml::from_str(CONFIG_TEMPLATE).expect("template harus valid TOML");
        assert_eq!(parsed.config_version, CURRENT_CONFIG_VERSION);
        assert_eq!(parsed.sources.len(), 12);
        assert!(parsed.sources.keys().any(|name| name == "otakudesu"));
    }

    #[test]
    fn template_matches_defaults() {
        // Mencegah drift: kalau default di kode berubah, template wajib ikut berubah.
        let parsed: Config = toml::from_str(CONFIG_TEMPLATE).unwrap();
        assert_eq!(parsed, Config::default());
    }

    #[test]
    fn template_is_semantically_valid() {
        let parsed: Config = toml::from_str(CONFIG_TEMPLATE).unwrap();
        let warnings = validate(&parsed).expect("template harus lolos validasi");
        assert!(
            warnings.is_empty(),
            "template menghasilkan warning: {warnings:?}"
        );
    }

    #[test]
    fn missing_file_is_created_from_template() {
        let (_tmp, paths) = temp_paths();
        let loaded = load(&paths).unwrap();
        assert!(loaded.created);
        assert!(paths.config_file.exists());
        assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
        assert_eq!(loaded.config, Config::default());

        // Muat kedua kali: tidak dibuat ulang.
        let again = load(&paths).unwrap();
        assert!(!again.created);
    }

    #[test]
    fn init_respects_existing_file_without_force() {
        let (_tmp, paths) = temp_paths();
        assert_eq!(
            init(&paths, false).unwrap(),
            InitOutcome::Created(paths.config_file.clone())
        );
        assert_eq!(
            init(&paths, false).unwrap(),
            InitOutcome::AlreadyExists(paths.config_file.clone())
        );
        assert_eq!(
            init(&paths, true).unwrap(),
            InitOutcome::Overwritten(paths.config_file.clone())
        );
    }

    #[test]
    fn save_then_reload_is_stable() {
        let (_tmp, paths) = temp_paths();
        let mut config = Config::default();
        config.playback.default_resolution = Resolution::P1080;
        config.general.default_source = "otakudesu".to_string();
        config.sources.get_mut("kusonime").unwrap().enabled = false;

        config.save(&paths.config_file).unwrap();
        let reloaded = load(&paths).unwrap().config;
        assert_eq!(reloaded, config);
    }

    #[test]
    fn atomic_write_replaces_existing_file_and_leaves_no_tmp() {
        let (_tmp, paths) = temp_paths();
        paths.ensure().unwrap();
        std::fs::write(&paths.config_file, "config_version = 1\n").unwrap();
        Config::default().save(&paths.config_file).unwrap();

        let raw = std::fs::read_to_string(&paths.config_file).unwrap();
        assert!(raw.contains("[playback]"));
        assert!(!paths.config_file.with_extension("toml.tmp").exists());
    }

    #[test]
    fn version_newer_than_supported_is_rejected() {
        let (_tmp, paths) = temp_paths();
        write_config(
            &paths,
            &format!("config_version = {}\n", CURRENT_CONFIG_VERSION + 1),
        );
        let err = load(&paths).unwrap_err();
        assert!(matches!(err, ConfigError::VersionTooNew { .. }), "{err}");
    }

    #[test]
    fn outdated_version_is_migrated_with_warning() {
        let (_tmp, paths) = temp_paths();
        write_config(&paths, "config_version = 0\n");
        let loaded = load(&paths).unwrap();
        assert_eq!(loaded.config.config_version, CURRENT_CONFIG_VERSION);
        assert!(loaded.warnings.iter().any(|w| w.contains("config_version")));
    }

    #[test]
    fn unknown_default_source_is_an_error_naming_the_key() {
        let (_tmp, paths) = temp_paths();
        write_config(
            &paths,
            "config_version = 1\n[general]\ndefault_source = \"nope\"\n",
        );
        let err = load(&paths).unwrap_err().to_string();
        assert!(err.contains("general.default_source"), "{err}");
        assert!(err.contains("nope"), "{err}");
    }

    #[test]
    fn all_sources_disabled_is_an_error() {
        let (_tmp, paths) = temp_paths();
        write_config(
            &paths,
            "config_version = 1\n\
             [sources.samehadaku]\nenabled = false\nbase_url = \"https://example.com\"\n",
        );
        let err = load(&paths).unwrap_err().to_string();
        assert!(err.contains("enabled = true"), "{err}");
    }

    #[test]
    fn unknown_fallback_name_is_filtered_with_warning() {
        let (_tmp, paths) = temp_paths();
        write_config(
            &paths,
            "config_version = 1\n\
             [general]\nfallback_order = [\"samehadaku\", \"tidak-ada\"]\n",
        );
        let loaded = load(&paths).unwrap();
        assert!(loaded.warnings.iter().any(|w| w.contains("tidak-ada")));
        // default 3 source tetap ada karena struct-level default mengisi sisanya.
        assert_eq!(loaded.config.playback_order()[0], "samehadaku");
    }

    #[test]
    fn invalid_url_and_empty_user_agent_are_errors() {
        let (_tmp, paths) = temp_paths();
        write_config(
            &paths,
            "config_version = 1\n\
             [http]\nuser_agent = \"\"\n\
             [sources.samehadaku]\nbase_url = \"bukan url\"\n",
        );
        let err = load(&paths).unwrap_err().to_string();
        assert!(err.contains("http.user_agent"), "{err}");
        assert!(err.contains("sources.samehadaku.base_url"), "{err}");
    }

    #[test]
    fn invalid_resolution_is_a_parse_error_mentioning_value() {
        let (_tmp, paths) = temp_paths();
        write_config(
            &paths,
            "config_version = 1\n[playback]\ndefault_resolution = \"hd\"\n",
        );
        let err = load(&paths).unwrap_err().to_string();
        assert!(err.contains("hd"), "{err}");
        assert!(
            err.contains("720p"),
            "pesan harus memberi contoh valid: {err}"
        );
    }

    #[test]
    fn overrides_change_effective_settings() {
        let mut config = Config::default();
        let warnings = config
            .apply_overrides(&Overrides {
                source: Some("otakudesu".to_string()),
                resolution: Some(Resolution::P1080),
                player: Some(Player::Vlc),
            })
            .unwrap();
        assert!(warnings.is_empty());
        assert_eq!(config.general.default_source, "otakudesu");
        assert_eq!(config.playback.default_resolution, Resolution::P1080);
        assert_eq!(config.playback.player, Player::Vlc);
        assert_eq!(config.playback_order()[0], "otakudesu");
    }

    #[test]
    fn override_with_unknown_source_fails_helpfully() {
        let mut config = Config::default();
        let err = config
            .apply_overrides(&Overrides {
                source: Some("tidak-ada-source".to_string()),
                ..Default::default()
            })
            .unwrap_err()
            .to_string();
        assert!(err.contains("tidak dikenal"), "{err}");
        assert!(
            err.contains("samehadaku"),
            "harus menyebut yang tersedia: {err}"
        );
    }

    #[test]
    fn playback_order_skips_disabled_and_unknown() {
        let mut config = Config::default();
        config.sources.get_mut("otakudesu").unwrap().enabled = false;
        config.general.fallback_order = vec![
            "otakudesu".to_string(),
            "tidak-ada".to_string(),
            "kusonime".to_string(),
        ];
        assert_eq!(config.playback_order(), vec!["samehadaku", "kusonime"]);
    }

    #[test]
    fn config_error_messages_are_multi_line_for_multiple_issues() {
        let mut config = Config::default();
        config.http.max_concurrent = 0;
        config.http.timeout_secs = 0;
        let err = validate(&config).unwrap_err().to_string();
        assert!(err.contains("- http.max_concurrent"));
        assert!(err.contains("- http.timeout_secs"));
    }
}
