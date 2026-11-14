//! Helper parsing defensif yang dipakai semua adapter source (DESIGN §9).
//!
//! Aturan modul ini:
//! - **Tidak pernah panic.** Selector yang tidak valid mengembalikan `None`/kosong.
//! - Membedakan "field opsional kosong" (→ `None`) dari "field kunci hilang"
//!   (→ [`SourceError::Parse`]) lewat fungsi `require_*`.
//! - Normalisasi teks (whitespace, entity, judul, nomor episode) dilakukan di sini
//!   supaya semua adapter berperilaku sama.

use scraper::{ElementRef, Html, Selector};
use url::Url;

use crate::error::{SourceError, SourceResult};
use crate::model::{AiringStatus, MediaKind, Resolution};

/// Parse selector CSS; `None` kalau sintaksnya tidak valid.
pub fn selector(css: &str) -> Option<Selector> {
    Selector::parse(css).ok()
}

/// Semua elemen yang cocok (kosong kalau selector tidak valid).
pub fn select<'a>(doc: &'a Html, css: &str) -> Vec<ElementRef<'a>> {
    match selector(css) {
        Some(selector) => doc.select(&selector).collect(),
        None => Vec::new(),
    }
}

/// Semua elemen turunan yang cocok.
pub fn select_in<'a>(element: ElementRef<'a>, css: &str) -> Vec<ElementRef<'a>> {
    match selector(css) {
        Some(selector) => element.select(&selector).collect(),
        None => Vec::new(),
    }
}

/// Elemen pertama yang cocok.
pub fn select_first<'a>(doc: &'a Html, css: &str) -> Option<ElementRef<'a>> {
    selector(css).and_then(|selector| doc.select(&selector).next())
}

/// Teks elemen pertama yang cocok, sudah dinormalisasi.
pub fn sel_text(doc: &Html, css: &str) -> Option<String> {
    select_first(doc, css).map(|element| text(&element))
}

/// Teks lengkap sebuah elemen (termasuk turunannya), whitespace dirapikan.
pub fn text(element: &ElementRef<'_>) -> String {
    collapse_whitespace(&element.text().collect::<String>())
}

/// Nilai atribut elemen (sudah di-trim).
pub fn attr(element: &ElementRef<'_>, name: &str) -> Option<String> {
    element
        .value()
        .attr(name)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

/// Resolusi URL relatif terhadap halaman. Menolak `#`, `javascript:`, `mailto:`,
/// dan string kosong (link semu seperti `href="#"` di Otakudesu).
pub fn abs_url(base: &Url, href: &str) -> Option<Url> {
    let raw = href.trim();
    if raw.is_empty()
        || raw.starts_with('#')
        || raw.starts_with("javascript:")
        || raw.starts_with("mailto:")
        || raw.starts_with("tel:")
    {
        return None;
    }
    base.join(raw).ok()
}

/// Field kunci: kalau selector tidak menemukan apa pun, hasilnya error parse yang
/// menyebut halaman dan nama field (bukan panic, bukan string kosong).
pub fn require_text(
    doc: &Html,
    css: &str,
    source: &'static str,
    page: &'static str,
    field: &'static str,
) -> SourceResult<String> {
    sel_text(doc, css).ok_or_else(|| {
        SourceError::parse(
            source,
            page,
            field,
            format!("selector {css:?} tidak menemukan elemen"),
        )
    })
}

/// Seperti [`require_text`], tetapi kosong dianggap hilang.
pub fn require_non_empty_text(
    doc: &Html,
    css: &str,
    source: &'static str,
    page: &'static str,
    field: &'static str,
) -> SourceResult<String> {
    let value = require_text(doc, css, source, page, field)?;
    if value.is_empty() {
        return Err(SourceError::parse(
            source,
            page,
            field,
            format!("selector {css:?} mengembalikan teks kosong"),
        ));
    }
    Ok(value)
}

/// Ambil `href` dari elemen pertama yang cocok lalu resolve jadi URL absolut.
pub fn require_url(
    doc: &Html,
    css: &str,
    base: &Url,
    source: &'static str,
    page: &'static str,
    field: &'static str,
) -> SourceResult<Url> {
    let element = select_first(doc, css).ok_or_else(|| {
        SourceError::parse(
            source,
            page,
            field,
            format!("selector {css:?} tidak menemukan elemen"),
        )
    })?;
    let href = attr(&element, "href").ok_or_else(|| {
        SourceError::parse(
            source,
            page,
            field,
            format!("elemen {css:?} tanpa atribut href"),
        )
    })?;
    abs_url(base, &href).ok_or_else(|| {
        SourceError::parse(
            source,
            page,
            field,
            format!("href {href:?} tidak bisa dijadikan URL absolut"),
        )
    })
}

/// Cari resolusi di label bebas, mis. `"Mp4 720p"`, `"MKV 1080p"`, `"Server 2 360p"`.
pub fn parse_resolution(label: &str) -> Option<Resolution> {
    let cleaned = label.replace(['(', ')', '[', ']', ',', ';'], " ");
    for token in cleaned.split_whitespace() {
        let token = token.trim_matches(|c: char| !c.is_ascii_alphanumeric());
        if token.len() < 4 || !token.ends_with(['p', 'P']) {
            continue;
        }
        // Token bisa berprefix huruf (mis. class `m480p`); ambil digit di depannya.
        let digits = token[..token.len() - 1].trim_start_matches(|c: char| !c.is_ascii_digit());
        let height: u16 = match digits.parse() {
            Ok(height) => height,
            Err(_) => continue,
        };
        if (144..=4320).contains(&height) {
            return Some(Resolution(height));
        }
    }
    // Kasus label murni angka: "720".
    label
        .trim()
        .parse::<u16>()
        .ok()
        .filter(|height| (144..=4320).contains(height))
        .map(Resolution)
}

/// Cari nomor episode di label bebas, mis. `"Ep 12"`, `"Episode 1085 Sub Indo"`,
/// `"S2 Episode 5"` → 5 (kata "episode" diutamakan daripada "S2").
pub fn parse_episode_number(label: &str) -> Option<u32> {
    let lowered = collapse_whitespace(label).to_lowercase();
    let words: Vec<&str> = lowered.split_whitespace().collect();

    for (index, word) in words.iter().enumerate() {
        let trimmed = word.trim_matches(|c: char| c == ':' || c == '.' || c == ',' || c == '-');
        let is_keyword = matches!(
            trimmed,
            "ep" | "eps" | "episode" | "ep." | "eps." | "episode:"
        );
        if is_keyword {
            if let Some(next) = words.get(index + 1) {
                if let Some(number) = numeric_token(next) {
                    return Some(number);
                }
            }
            continue;
        }
        // Bentuk menempel: "ep12", "episode12", "eps12".
        for prefix in ["episode", "eps", "ep"] {
            if let Some(rest) = trimmed.strip_prefix(prefix) {
                if let Some(number) = numeric_token(rest) {
                    return Some(number);
                }
            }
        }
    }

    // Fallback: token angka pertama yang bukan resolusi/tahun.
    words.iter().find_map(|word| numeric_token(word))
}

/// Tahun rilis 4 digit di dalam teks, mis. `"Nonton One Piece (2024)"`.
pub fn parse_year(label: &str) -> Option<u16> {
    for token in collapse_whitespace(label).split_whitespace() {
        let token = token.trim_matches(|c: char| !c.is_ascii_digit());
        if token.len() != 4 {
            continue;
        }
        if let Ok(year) = token.parse::<u16>() {
            if (1900..=2100).contains(&year) {
                return Some(year);
            }
        }
    }
    None
}

/// Skor di label bebas, mis. `"8.70"`, `"Skor 7,5"` → `Some(7.5)`.
pub fn parse_score(label: &str) -> Option<f32> {
    for token in collapse_whitespace(label).split_whitespace() {
        let token = token
            .trim_matches(|c: char| !c.is_ascii_digit() && c != '.' && c != ',')
            .replace(',', ".");
        if token.is_empty() || token.len() > 5 {
            continue;
        }
        // Hindari menangkap tahun (4 digit tanpa desimal).
        if token.len() == 4 && !token.contains('.') {
            continue;
        }
        if let Ok(score) = token.parse::<f32>() {
            if (0.0..=10.0).contains(&score) {
                return Some(score);
            }
        }
    }
    None
}

/// Normalisasi judul untuk pencocokan lintas-source (DESIGN §10.3):
/// lowercase, buang isi kurung `(2024)`/`[Batch]`, buang stopword khas situs,
/// sisakan alfanumerik + spasi tunggal.
pub fn normalize_title(raw: &str) -> String {
    const STOPWORDS: &[&str] = &[
        "nonton",
        "sub",
        "indo",
        "subtitle",
        "indonesia",
        "batch",
        "lengkap",
        "streaming",
        "download",
        "episode",
    ];

    let lowered = raw.to_lowercase();
    let mut without_brackets = String::with_capacity(lowered.len());
    let mut depth = 0_u32;
    for ch in lowered.chars() {
        match ch {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            _ if depth == 0 => without_brackets.push(ch),
            _ => {}
        }
    }

    without_brackets
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty() && !STOPWORDS.contains(word))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Rapikan whitespace (termasuk `&nbsp;`/tab/newline) jadi satu spasi.
pub fn collapse_whitespace(text: &str) -> String {
    text.replace('\u{00a0}', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Decode entity HTML (`&amp;`, `&#39;`, ...).
pub fn decode_entities(text: &str) -> String {
    html_escape::decode_html_entities(text).into_owned()
}

fn numeric_token(token: &str) -> Option<u32> {
    let token = token.trim_matches(|c: char| !c.is_ascii_alphanumeric());
    if token.is_empty() {
        return None;
    }
    // Resolusi ("720p") dan codec ("x265") bukan nomor episode.
    if token.ends_with(['p', 'P']) || token.starts_with('x') || token.starts_with('X') {
        return None;
    }
    let digits: String = token.chars().filter(char::is_ascii_digit).collect();
    if digits.is_empty() || digits.len() > 4 {
        return None;
    }
    let value: u32 = digits.parse().ok()?;
    // Tahun rilis bukan nomor episode.
    if digits.len() == 4 && (1900..=2100).contains(&value) {
        return None;
    }
    if value == 0 || value > 2999 {
        return None;
    }
    Some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(html: &str) -> Html {
        Html::parse_document(html)
    }

    fn base() -> Url {
        Url::parse("https://example.test/anime/one-piece/").unwrap()
    }

    #[test]
    fn extracts_and_normalizes_text() {
        let html = r#"<div class="title">  Nonton <b>One</b> Piece&nbsp;(2024)  </div>"#;
        let document = doc(html);
        assert_eq!(
            sel_text(&document, ".title").as_deref(),
            Some("Nonton One Piece (2024)")
        );
        assert_eq!(sel_text(&document, ".tidak-ada"), None);
    }

    #[test]
    fn attributes_and_urls_are_resolved() {
        let html = r#"<a class="ep" href="/episode/one-piece-episode-12/">Ep 12</a>"#;
        let document = doc(html);
        let element = select_first(&document, ".ep").unwrap();
        assert_eq!(
            attr(&element, "href").as_deref(),
            Some("/episode/one-piece-episode-12/")
        );
        assert_eq!(
            abs_url(&base(), "/episode/x").unwrap().as_str(),
            "https://example.test/episode/x"
        );
        assert_eq!(
            abs_url(&base(), "https://other.test/y").unwrap().as_str(),
            "https://other.test/y"
        );
    }

    #[test]
    fn url_rejects_fake_links() {
        for fake in [
            "#",
            "",
            "  ",
            "javascript:void(0)",
            "mailto:a@b.c",
            "tel:+62",
        ] {
            assert!(abs_url(&base(), fake).is_none(), "{fake:?} harus ditolak");
        }
    }

    #[test]
    fn require_helpers_produce_parse_errors() {
        let document = doc("<html><body></body></html>");
        let err = require_text(&document, ".judul", "samehadaku", "detail", "title").unwrap_err();
        assert!(matches!(err, SourceError::Parse { .. }));
        assert!(err.to_string().contains("title"));

        let err =
            require_url(&document, ".judul", &base(), "samehadaku", "detail", "url").unwrap_err();
        assert!(matches!(err, SourceError::Parse { .. }));
    }

    #[test]
    fn require_non_empty_text_rejects_blank() {
        let document = doc(r#"<div class="judul">   </div>"#);
        let err = require_non_empty_text(&document, ".judul", "s", "detail", "title").unwrap_err();
        assert!(matches!(err, SourceError::Parse { .. }));
    }

    #[test]
    fn parses_resolution_from_free_text_labels() {
        let cases = [
            ("Mp4 720p", 720),
            ("MKV 1080p", 1080),
            ("360p", 360),
            ("Server 2 480p", 480),
            ("(1080p)", 1080),
            ("VIP x265 720P", 720),
        ];
        for (label, expected) in cases {
            assert_eq!(
                parse_resolution(label).map(|resolution| resolution.height()),
                Some(expected),
                "label {label:?}"
            );
        }
        assert_eq!(parse_resolution("720").map(|r| r.height()), Some(720));
        assert!(parse_resolution("VIP x265").is_none());
        assert!(parse_resolution("Batch").is_none());
    }

    #[test]
    fn parses_episode_numbers() {
        let cases = [
            ("Ep 12", 12),
            ("Episode 1085 Sub Indo", 1085),
            ("One Piece Episode 1085", 1085),
            ("Ep. 7", 7),
            ("ep12", 12),
            ("S2 Episode 5", 5),
            ("12", 12),
        ];
        for (label, expected) in cases {
            assert_eq!(
                parse_episode_number(label),
                Some(expected),
                "label {label:?}"
            );
        }
        for label in ["Batch", "Movie", "One Piece", "720p"] {
            assert_eq!(parse_episode_number(label), None, "label {label:?}");
        }
    }

    #[test]
    fn parses_year_and_score() {
        assert_eq!(parse_year("Nonton One Piece (2024)"), Some(2024));
        assert_eq!(parse_year("Episode 12"), None);
        assert_eq!(parse_score("8.70"), Some(8.7));
        assert_eq!(parse_score("Skor 7,5"), Some(7.5));
        assert_eq!(parse_score("2024"), None);
        assert_eq!(parse_score("Episode 12"), None);
    }

    #[test]
    fn normalizes_titles_for_matching() {
        assert_eq!(
            normalize_title("Nonton One Piece (2024) Sub Indo"),
            "one piece"
        );
        assert_eq!(
            normalize_title("Kusuriya no Hitorigoto Season 3 [Batch]"),
            "kusuriya no hitorigoto season 3"
        );
        assert_eq!(
            normalize_title("  Shingeki no Kyojin: The Final Season  "),
            "shingeki no kyojin the final season"
        );
    }

    #[test]
    fn collapses_whitespace_and_decodes_entities() {
        assert_eq!(collapse_whitespace("  a\t b\n c  "), "a b c");
        assert_eq!(collapse_whitespace("a\u{00a0}b"), "a b");
        assert_eq!(decode_entities("a &amp; b &#39;c&#39;"), "a & b 'c'");
    }
}

/// Normalisasi teks tipe media situs (`TV`, `Movie`, `OVA`, `Special`, `BD`).
pub fn media_kind(label: &str) -> MediaKind {
    let lowered = label.trim().to_ascii_lowercase();
    if lowered.contains("movie") || lowered == "film" {
        MediaKind::Movie
    } else if lowered.contains("ova") {
        MediaKind::Ova
    } else if lowered.contains("special") {
        MediaKind::Special
    } else if lowered.contains("tv") || lowered.contains("series") || lowered == "bd" {
        MediaKind::Tv
    } else {
        MediaKind::Unknown
    }
}

/// Normalisasi status dari berbagai kosakata situs (Indonesia & AniList).
pub fn airing_status(label: &str) -> AiringStatus {
    let lowered = label.trim().to_ascii_lowercase();
    if lowered.contains("ongoing") || lowered.contains("airing") && !lowered.contains("finished") {
        AiringStatus::Ongoing
    } else if lowered.contains("completed")
        || lowered.contains("finished")
        || lowered.contains("tamat")
    {
        AiringStatus::Completed
    } else {
        AiringStatus::Unknown
    }
}

#[cfg(test)]
mod enum_tests {
    use super::*;

    #[test]
    fn media_kind_normalizes_site_vocabulary() {
        assert_eq!(media_kind("TV"), MediaKind::Tv);
        assert_eq!(media_kind("Movie"), MediaKind::Movie);
        assert_eq!(media_kind("OVA"), MediaKind::Ova);
        assert_eq!(media_kind("BD"), MediaKind::Tv);
        assert_eq!(media_kind("???"), MediaKind::Unknown);
    }

    #[test]
    fn airing_status_normalizes_two_vocabularies() {
        assert_eq!(airing_status("Ongoing"), AiringStatus::Ongoing);
        assert_eq!(airing_status("Finished Airing"), AiringStatus::Completed);
        assert_eq!(airing_status("Completed"), AiringStatus::Completed);
        assert_eq!(airing_status("Tamat"), AiringStatus::Completed);
        assert_eq!(airing_status(""), AiringStatus::Unknown);
    }
}

/// Decode base64 (payload `data-content` Otakudesu) menjadi teks UTF-8.
pub fn b64decode_text(encoded: &str) -> Option<String> {
    use base64::Engine as _;
    let engine = base64::engine::general_purpose::STANDARD;
    let bytes = engine.decode(encoded.trim()).ok()?;
    String::from_utf8(bytes).ok()
}

#[cfg(test)]
mod base64_tests {
    use super::*;

    #[test]
    fn decodes_ajax_payload() {
        // {"id":206798,"i":0,"q":"480p"}
        let encoded = "eyJpZCI6MjA2Nzk4LCJpIjowLCJxIjoiNDgwcCJ9";
        assert_eq!(
            b64decode_text(encoded).unwrap(),
            r#"{"id":206798,"i":0,"q":"480p"}"#
        );
        assert!(b64decode_text("bukan base64!!").is_none());
    }
}
