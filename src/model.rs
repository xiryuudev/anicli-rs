//! Model domain yang dipakai lintas layer (DESIGN §5).
//!
//! Semua tipe di sini **murni data**: tidak ada I/O, tidak ada HTTP, tidak ada UI.
//! Itulah yang membuat parser source bisa dites offline: parser menghasilkan tipe ini
//! dari fixture HTML, dan test memeriksanya tanpa jaringan.

use std::fmt;

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use url::Url;

// =============================================================================
// Language
// =============================================================================

/// Bahasa konten sebuah source.
///
/// Sengaja `Copy` + `Eq` + `Hash` supaya bisa jadi key filter UI dan disimpan di
/// config. Menambah bahasa baru cukup menambah varian di sini — tidak ada kode tab
/// yang perlu diubah.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Language {
    /// Subtitle Indonesia (mayoritas source).
    #[serde(rename = "id")]
    Indonesian,
    /// English sub/dub.
    #[serde(rename = "en")]
    English,
    /// Japanese (placeholder untuk membuktikan extensibility).
    #[serde(rename = "jp")]
    Japanese,
}

impl Language {
    /// Semua bahasa yang dikenal, urut untuk tampilan.
    pub const ALL: &'static [Language] =
        &[Language::Indonesian, Language::English, Language::Japanese];

    /// Kode pendek stabil (`id`, `en`, `jp`) untuk config/log.
    pub fn code(self) -> &'static str {
        match self {
            Self::Indonesian => "id",
            Self::English => "en",
            Self::Japanese => "jp",
        }
    }

    /// Tag pendek untuk label UI, mis. `[ID]`.
    pub fn tag(self) -> &'static str {
        match self {
            Self::Indonesian => "[ID]",
            Self::English => "[EN]",
            Self::Japanese => "[JP]",
        }
    }

    /// Nama panjang untuk tab Bahasa.
    pub fn label(self) -> &'static str {
        match self {
            Self::Indonesian => "Indonesia",
            Self::English => "English",
            Self::Japanese => "Japanese",
        }
    }

    /// Parse dari kode (`id`/`id-ID`/`indonesian`), case-insensitive.
    pub fn from_code(raw: &str) -> Option<Self> {
        let value = raw.trim().to_ascii_lowercase();
        match value.as_str() {
            "id" | "id-id" | "indonesian" | "indonesia" | "indo" => Some(Self::Indonesian),
            "en" | "en-us" | "english" | "eng" => Some(Self::English),
            "jp" | "ja" | "japanese" | "jpn" => Some(Self::Japanese),
            _ => None,
        }
    }
}

impl fmt::Display for Language {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.label())
    }
}

// =============================================================================
// Resolution
// =============================================================================

/// Resolusi video sebagai tinggi dalam piksel (240, 360, 480, 720, 1080, ...).
///
/// Disimpan sebagai angka supaya pemilihan "terdekat" (DESIGN §11) bisa dihitung
/// dengan `Ord`, bukan perbandingan string. Ditampilkan/di-serialize sebagai
/// `"720p"` agar config tetap enak dibaca manusia.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Resolution(pub u16);

impl Resolution {
    pub const P240: Self = Self(240);
    pub const P360: Self = Self(360);
    pub const P480: Self = Self(480);
    pub const P720: Self = Self(720);
    pub const P1080: Self = Self(1080);

    pub const fn height(self) -> u16 {
        self.0
    }
}

impl fmt::Display for Resolution {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}p", self.0)
    }
}

/// Error parsing resolusi, mis. `"720"` tanpa `p` tetap diterima tetapi `"hd"` tidak.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("resolusi tidak dikenal: {0:?} (contoh yang valid: 360p, 480p, 720p, 1080p)")]
pub struct ParseResolutionError(pub String);

impl std::str::FromStr for Resolution {
    type Err = ParseResolutionError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let trimmed = s.trim();
        let digits: String = trimmed
            .trim_end_matches(['p', 'P', 'i', 'I'])
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        let height: u16 = digits
            .parse()
            .map_err(|_| ParseResolutionError(s.to_string()))?;
        // Rentang wajar untuk video; menolak "0p" atau angka absurd.
        if (144..=4320).contains(&height) {
            Ok(Self(height))
        } else {
            Err(ParseResolutionError(s.to_string()))
        }
    }
}

impl Serialize for Resolution {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Resolution {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        raw.parse().map_err(serde::de::Error::custom)
    }
}

// =============================================================================
// Identitas & enumerasi kecil
// =============================================================================

/// Nama source yang dipakai di config, log, dan hasil pencarian.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SourceId(pub String);

impl SourceId {
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SourceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum MediaKind {
    Tv,
    Movie,
    Ova,
    Special,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum AiringStatus {
    Ongoing,
    Completed,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Container {
    Mp4,
    Mkv,
    #[default]
    Unknown,
}

/// Bagaimana link itu bisa dipakai.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StreamKind {
    /// URL media langsung (mp4/mkv/m3u8) — mpv bisa memutar dengan header yang ada.
    Direct,
    /// Halaman player pihak ketiga yang mungkin butuh JS (lihat DESIGN §12.6).
    Embed,
    /// Halaman file-hosting yang tidak bisa diputar langsung di v1.
    DownloadOnly,
}

/// Server/host penyedia stream, dipakai untuk badge & preferensi.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServerKind {
    Mirror,
    Zippy,
    Pixeldrain,
    Gofile,
    Mega,
    Filedon,
    Acefile,
    Mediafire,
    Icefile,
    Desustream,
    Odcloud,
    Blogspot,
    Other(String),
}

impl fmt::Display for ServerKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Other(name) => f.write_str(name),
            Self::Mirror => f.write_str("Mirror"),
            Self::Zippy => f.write_str("ZippyShare"),
            Self::Pixeldrain => f.write_str("Pixeldrain"),
            Self::Gofile => f.write_str("Gofile"),
            Self::Mega => f.write_str("Mega"),
            Self::Filedon => f.write_str("Filedon"),
            Self::Acefile => f.write_str("Acefile"),
            Self::Mediafire => f.write_str("Mediafire"),
            Self::Icefile => f.write_str("IceFile"),
            Self::Desustream => f.write_str("Desustream"),
            Self::Odcloud => f.write_str("Odcloud"),
            Self::Blogspot => f.write_str("Blogspot"),
        }
    }
}

/// Header yang **wajib** dikirim saat memutar URL media (banyak host menolak tanpa
/// Referer/UA). Selalu diisi oleh adapter source, bukan ditebak player.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestHeaders {
    pub referer: Option<Url>,
    pub user_agent: Option<String>,
}

impl RequestHeaders {
    pub fn with_referer(referer: Url) -> Self {
        Self {
            referer: Some(referer),
            user_agent: None,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.referer.is_none() && self.user_agent.is_none()
    }
}

// =============================================================================
// Hasil tiap tahap
// =============================================================================

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnimeSearchResult {
    pub source: SourceId,
    pub title: String,
    pub url: Url,
    pub thumbnail: Option<Url>,
    pub kind: Option<MediaKind>,
    pub status: Option<AiringStatus>,
    pub score: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Episode {
    /// `None` untuk entri non-nomor (mis. "Batch", "Movie").
    pub number: Option<u32>,
    pub label: String,
    pub title: Option<String>,
    pub url: Url,
    pub released: Option<NaiveDate>,
}

impl Episode {
    pub fn label_for_ui(&self) -> String {
        match self.number {
            Some(number) if self.label.trim().is_empty() => format!("Episode {number}"),
            _ => self.label.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnimeDetail {
    pub source: SourceId,
    pub url: Url,
    pub title: String,
    /// Judul alternatif (Jepang/English) untuk pencocokan lintas source (DESIGN §10.3).
    pub alt_titles: Vec<String>,
    pub synopsis: String,
    pub genres: Vec<String>,
    /// Tipe media (TV/Movie/OVA) bila situs mencantumkannya.
    pub kind: Option<MediaKind>,
    pub status: AiringStatus,
    pub year: Option<u16>,
    pub score: Option<f32>,
    pub total_episodes: Option<u32>,
    pub poster: Option<Url>,
    pub episodes: Vec<Episode>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamLink {
    pub source: SourceId,
    pub resolution: Option<Resolution>,
    /// Label asli dari situs, mis. "Mp4 720p (ZippyShare)".
    pub label: String,
    pub server: ServerKind,
    pub container: Option<Container>,
    pub url: Url,
    pub headers: RequestHeaders,
    pub stream_kind: StreamKind,
}

impl StreamLink {
    /// Resolusi untuk ditampilkan, atau "?" kalau tidak diketahui.
    pub fn resolution_label(&self) -> String {
        self.resolution
            .map(|resolution| resolution.to_string())
            .unwrap_or_else(|| "?".to_string())
    }

    pub fn is_playable_directly(&self) -> bool {
        self.stream_kind == StreamKind::Direct
    }

    /// Label pendek untuk UI/log.
    pub fn stream_kind_label(&self) -> &'static str {
        match self.stream_kind {
            StreamKind::Direct => "direct",
            StreamKind::Embed => "embed",
            StreamKind::DownloadOnly => "download",
        }
    }
}

/// Identitas lintas-source: apa yang mau diputar, tanpa terikat URL satu situs.
///
/// Ini yang membuat fallback bisa "pindah source" (DESIGN §10.1): URL anime di
/// Samehadaku dan Otakudesu berbeda, tetapi judul + nomor episode sama.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlaybackTarget {
    pub anime_title: String,
    pub alt_titles: Vec<String>,
    pub episode_number: Option<u32>,
    pub episode_label: String,
    pub origin: SourceId,
    pub anime_url: Url,
    pub episode_url: Url,
}

impl PlaybackTarget {
    /// Judul yang dipakai untuk mencocokkan di source lain (judul utama dulu).
    pub fn candidate_titles(&self) -> Vec<&str> {
        let mut titles: Vec<&str> = vec![self.anime_title.as_str()];
        titles.extend(self.alt_titles.iter().map(String::as_str));
        titles
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(raw: &str) -> Url {
        Url::parse(raw).unwrap()
    }

    #[test]
    fn parses_common_resolution_forms() {
        assert_eq!("720p".parse::<Resolution>().unwrap(), Resolution::P720);
        assert_eq!("720P".parse::<Resolution>().unwrap(), Resolution::P720);
        assert_eq!(" 1080p ".parse::<Resolution>().unwrap(), Resolution::P1080);
        assert_eq!("360".parse::<Resolution>().unwrap(), Resolution::P360);
        assert_eq!("480i".parse::<Resolution>().unwrap(), Resolution::P480);
    }

    #[test]
    fn rejects_garbage_and_absurd_values() {
        for bad in ["", "hd", "p", "0p", "12p", "99999p", "seven20p"] {
            assert!(
                bad.parse::<Resolution>().is_err(),
                "{bad:?} seharusnya ditolak"
            );
        }
    }

    #[test]
    fn ordering_is_by_height() {
        assert!(Resolution::P360 < Resolution::P720);
        assert!(Resolution::P1080 > Resolution::P480);
        assert_eq!(Resolution::P720.to_string(), "720p");
    }

    #[test]
    fn serde_roundtrip_uses_string_form() {
        let json = serde_json::to_string(&Resolution::P1080).unwrap();
        assert_eq!(json, "\"1080p\"");
        let back: Resolution = serde_json::from_str("\"480p\"").unwrap();
        assert_eq!(back, Resolution::P480);
    }

    #[test]
    fn deserialize_error_mentions_valid_examples() {
        let err = serde_json::from_str::<Resolution>("\"hd\"")
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("720p"),
            "pesan error harus memberi contoh: {err}"
        );
    }

    #[test]
    fn playback_target_lists_alt_titles() {
        let target = PlaybackTarget {
            anime_title: "Kusuriya no Hitorigoto".into(),
            alt_titles: vec!["The Apothecary Diaries".into()],
            episode_number: Some(5),
            episode_label: "Ep 5".into(),
            origin: SourceId::new("samehadaku"),
            anime_url: url("https://v2.samehadaku.how/anime/x/"),
            episode_url: url("https://v2.samehadaku.how/x-episode-5/"),
        };
        assert_eq!(
            target.candidate_titles(),
            vec!["Kusuriya no Hitorigoto", "The Apothecary Diaries"]
        );
    }

    #[test]
    fn episode_ui_label_falls_back_to_number() {
        let episode = Episode {
            number: Some(12),
            label: String::new(),
            title: None,
            url: url("https://example.test/ep12"),
            released: None,
        };
        assert_eq!(episode.label_for_ui(), "Episode 12");
    }

    #[test]
    fn stream_label_and_playability_helpers() {
        let link = StreamLink {
            source: SourceId::new("samehadaku"),
            resolution: Some(Resolution::P720),
            label: "Mp4 720p".into(),
            server: ServerKind::Icefile,
            container: Some(Container::Mp4),
            url: url("https://example.test/file.m3u8"),
            headers: RequestHeaders::default(),
            stream_kind: StreamKind::Direct,
        };
        assert_eq!(link.resolution_label(), "720p");
        assert!(link.is_playable_directly());
    }

    #[test]
    fn request_headers_constructors() {
        assert!(RequestHeaders::default().is_empty());
        let headers = RequestHeaders::with_referer(url("https://example.test/ep/"));
        assert!(!headers.is_empty());
        assert_eq!(headers.referer.unwrap().host_str(), Some("example.test"));
    }
}
