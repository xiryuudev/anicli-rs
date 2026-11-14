//! Parser murni Kusonime (`kusonime.com`) — katalog metadata & batch download.
//!
//! Kusonime **tidak** menyediakan streaming per episode (DESIGN §22.2): satu halaman
//! = satu seri, berisi tabel unduhan `.rar`. Karena itu parser ini fokus metadata
//! dan daftar host, bukan `StreamLink` yang bisa diputar.

use scraper::{Html, Selector};
use url::Url;

use crate::error::{SourceError, SourceResult};
use crate::model::{
    AiringStatus, AnimeDetail, AnimeSearchResult, Container, MediaKind, Resolution, SourceId,
};
use crate::sources::util;
use crate::sources::util::{abs_url, attr, select, select_first, select_in, text};

pub const NAME: &str = "kusonime";

const PAGE_LIST: &str = "list";
const PAGE_DETAIL: &str = "detail";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostLink {
    pub host: String,
    pub url: Url,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadRow {
    pub label: String,
    pub resolution: Option<Resolution>,
    pub container: Option<Container>,
    pub links: Vec<HostLink>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BatchBlock {
    pub title: String,
    pub rows: Vec<DownloadRow>,
}

/// Hasil REST `/wp-json/wp/v2/posts?search=…`.
pub fn parse_rest(json_text: &str, source: &SourceId) -> SourceResult<Vec<AnimeSearchResult>> {
    let value: serde_json::Value = serde_json::from_str(json_text).map_err(|err| {
        SourceError::parse(
            NAME,
            PAGE_LIST,
            "search_rest",
            format!("JSON tidak sesuai bentuk yang diharapkan: {err}"),
        )
    })?;
    let Some(items) = value.as_array() else {
        return Err(SourceError::parse(
            NAME,
            PAGE_LIST,
            "search_rest",
            "respons REST bukan array",
        ));
    };

    let mut results = Vec::new();
    for item in items {
        let Some(link) = item.get("link").and_then(|link| link.as_str()) else {
            continue;
        };
        let Some(url) = Url::parse(link.trim()).ok() else {
            continue;
        };
        let title = item
            .get("title")
            .and_then(|title| title.get("rendered"))
            .and_then(|rendered| rendered.as_str())
            .map(clean_title)
            .unwrap_or_default();
        if title.is_empty() {
            continue;
        }
        results.push(AnimeSearchResult {
            source: source.clone(),
            title,
            url,
            thumbnail: None,
            kind: None,
            status: None,
            score: None,
        });
    }
    Ok(results)
}

/// Kartu listing (home/search/genre/category) — markup identik.
pub fn parse_cards(
    html_text: &str,
    base: &Url,
    source: &SourceId,
) -> SourceResult<Vec<AnimeSearchResult>> {
    let document = Html::parse_document(html_text);
    let cards = select(&document, "div.kover");
    let mut results = Vec::new();

    for card in &cards {
        let Some(anchor) = select_in(*card, "h2.episodeye a").first().copied() else {
            continue;
        };
        let Some(href) = attr(&anchor, "href") else {
            continue;
        };
        let Some(url) = abs_url(base, &href) else {
            continue;
        };
        let title = clean_title(&text(&anchor));
        if title.is_empty() {
            continue;
        }
        let thumbnail = select_in(*card, "img.wp-post-image")
            .first()
            .and_then(|image| attr(image, "src"))
            .and_then(|src| abs_url(base, &src));
        results.push(AnimeSearchResult {
            source: source.clone(),
            title,
            url,
            thumbnail,
            kind: None,
            status: None,
            score: None,
        });
    }

    if results.is_empty() && !html_text.contains("kover") {
        return Err(SourceError::parse(
            NAME,
            PAGE_LIST,
            "div.kover",
            "tidak ada kartu listing di halaman ini",
        ));
    }
    Ok(results)
}

/// Detail seri + tabel unduhan.
pub fn parse_detail(html_text: &str, base: &Url, source: &SourceId) -> SourceResult<AnimeDetail> {
    let document = Html::parse_document(html_text);

    let title = select_first(&document, "h1.jdlz")
        .map(|element| text(&element))
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            SourceError::parse(
                NAME,
                PAGE_DETAIL,
                "h1.jdlz",
                "judul tidak ditemukan — kemungkinan soft-404 (slug tidak ada)",
            )
        })?;

    let poster = select_first(&document, ".post-thumb img.wp-post-image")
        .and_then(|image| attr(&image, "src"))
        .and_then(|src| abs_url(base, &src));

    let mut meta = std::collections::BTreeMap::new();
    for paragraph in select(&document, ".lexot > .info > p") {
        let label_elements = select_in(paragraph, "b");
        let Some(label_element) = label_elements.first().copied() else {
            continue;
        };
        let label = text(&label_element)
            .trim_end_matches(':')
            .trim()
            .to_string();
        let full = text(&paragraph);
        let value = full
            .split_once(':')
            .map(|(_, value)| value.trim().to_string())
            .unwrap_or_default();
        meta.insert(label, value);
    }

    let status = meta
        .get("Status")
        .map(|value| util::airing_status(value))
        .unwrap_or(AiringStatus::Unknown);
    let score = meta.get("Score").and_then(|value| util::parse_score(value));
    let year = meta
        .get("Released on")
        .and_then(|value| util::parse_year(value));
    let total_episodes = meta
        .get("Total Episode")
        .and_then(|value| util::parse_episode_number(value));
    let kind = meta.get("Type").map(|value| util::media_kind(value));

    let mut genres = Vec::new();
    if let Some(paragraph) = select(&document, ".lexot > .info > p")
        .into_iter()
        .find(|paragraph| {
            select_in(*paragraph, "b")
                .first()
                .map(|label| text(label).to_ascii_lowercase().starts_with("genre"))
                .unwrap_or(false)
        })
    {
        for anchor in select_in(paragraph, "a[rel=\"tag\"], a[href*=\"/genres/\"]") {
            let genre = text(&anchor);
            if !genre.is_empty() {
                genres.push(genre);
            }
        }
    }
    genres.dedup();

    let synopsis = select(&document, ".lexot > p")
        .iter()
        .map(|paragraph| text(paragraph))
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");

    let alt_titles = meta
        .get("Japanese")
        .filter(|value| !value.is_empty())
        .cloned()
        .into_iter()
        .collect::<Vec<_>>();

    let blocks = parse_batches(&document, base);
    let episodes = Vec::new(); // Kusonime tidak punya daftar episode.

    Ok(AnimeDetail {
        source: source.clone(),
        url: base.clone(),
        title: clean_title(&title),
        alt_titles,
        synopsis,
        genres,
        kind,
        status,
        year,
        score,
        total_episodes,
        poster,
        episodes,
    })
    .map(|mut detail| {
        // Simpan ringkasan batch di `total_episodes`-friendly way? Tidak: tipe domain
        // tidak punya field batch; hook download (v1.1) akan memakai parser terpisah.
        let _ = blocks;
        detail.url = base.clone();
        detail
    })
}

/// Tabel unduhan: `#dl > .smokeddlrh > .smokeurlrh`.
pub fn parse_batches(document: &Html, base: &Url) -> Vec<BatchBlock> {
    let mut blocks = Vec::new();
    for block in select(document, "#dl > div.smokeddlrh") {
        let title = select_in(block, ".smokettlrh")
            .first()
            .map(|element| text(element))
            .unwrap_or_default();
        let mut rows = Vec::new();
        for row in select_in(block, ".smokeurlrh") {
            let label = select_in(row, "strong")
                .first()
                .map(|element| text(element))
                .unwrap_or_default();
            let resolution = util::parse_resolution(&label);
            let container = if label.to_ascii_lowercase().contains("mkv") {
                Some(Container::Mkv)
            } else if label.to_ascii_lowercase().contains("mp4") || resolution.is_some() {
                Some(Container::Mp4)
            } else {
                None
            };
            let links = select_in(row, "a[href]")
                .iter()
                .filter_map(|anchor| {
                    let host = text(anchor);
                    let href = attr(anchor, "href")?;
                    let url = abs_url(base, &href)?;
                    Some(HostLink { host, url })
                })
                .collect::<Vec<_>>();
            if !links.is_empty() {
                rows.push(DownloadRow {
                    label,
                    resolution,
                    container,
                    links,
                });
            }
        }
        if !rows.is_empty() {
            blocks.push(BatchBlock { title, rows });
        }
    }
    blocks
}

/// Kusonime menempelkan "Subtitle Indonesia"/"Batch Subtitle Indonesia" ke judul.
pub fn clean_title(title: &str) -> String {
    let mut value = util::collapse_whitespace(title);
    for suffix in [
        " Batch Subtitle Indonesia",
        " Subtitle Indonesia",
        " Sub Indo",
    ] {
        if value
            .to_ascii_lowercase()
            .ends_with(&suffix.to_ascii_lowercase())
        {
            let keep = value.len() - suffix.len();
            value.truncate(keep);
            value = value.trim().to_string();
        }
    }
    value
}

/// Tipe media dari label (dipakai bila perlu).
pub fn kind_from_label(label: &str) -> MediaKind {
    util::media_kind(label)
}

/// Selector helper untuk pemakaian lanjutan (hook download): `#dl`.
pub fn has_download_table(html_text: &str) -> bool {
    Html::parse_document(html_text)
        .select(&Selector::parse("#dl").expect("selector statis valid"))
        .next()
        .is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(path: &str) -> String {
        let full = format!("{}/tests/fixtures/{path}", env!("CARGO_MANIFEST_DIR"));
        std::fs::read_to_string(&full).unwrap_or_else(|err| panic!("baca {full}: {err}"))
    }

    fn base() -> Url {
        Url::parse("https://kusonime.com/").unwrap()
    }

    fn id() -> SourceId {
        SourceId::new(NAME)
    }

    #[test]
    fn rest_search_is_parsed_and_titles_cleaned() {
        let items = parse_rest(&fixture("kusonime/rest_search.json"), &id()).unwrap();
        assert!(!items.is_empty());
        assert!(items[0].title.starts_with("Ameku"));
        assert!(!items[0]
            .title
            .to_ascii_lowercase()
            .contains("subtitle indonesia"));
        assert!(items[0].url.as_str().starts_with("https://kusonime.com/"));
    }

    #[test]
    fn cards_listing_is_parsed() {
        let items = parse_cards(&fixture("kusonime/search.html"), &base(), &id()).unwrap();
        assert_eq!(items.len(), 10);
        assert!(items[0].title.contains("Boruto"));
        assert!(items[0].url.as_str().starts_with("https://kusonime.com/"));
    }

    #[test]
    fn detail_metadata_is_parsed() {
        let detail = parse_detail(&fixture("kusonime/detail.html"), &base(), &id()).unwrap();
        assert_eq!(detail.title, "Ameku Takao no Suiri Karte BD");
        assert_eq!(detail.status, AiringStatus::Completed);
        assert_eq!(detail.score, Some(7.14));
        assert_eq!(detail.year, Some(2025));
        assert_eq!(detail.genres, vec!["Mystery", "Slice of Life"]);
        assert!(detail
            .alt_titles
            .iter()
            .any(|title| title.contains("天久鷹央")));
        assert!(!detail.synopsis.is_empty());
        assert!(
            detail.episodes.is_empty(),
            "Kusonime tidak punya daftar episode"
        );
    }

    #[test]
    fn batch_download_table_is_parsed() {
        let document = Html::parse_document(&fixture("kusonime/detail.html"));
        let blocks = parse_batches(&document, &base());
        assert!(!blocks.is_empty());
        let rows = &blocks[0].rows;
        assert!(rows.len() >= 3);
        assert!(rows.iter().any(|row| row.resolution.is_some()));
        let all_links: Vec<&HostLink> = rows.iter().flat_map(|row| row.links.iter()).collect();
        assert!(all_links
            .iter()
            .any(|link| link.host.contains("Pixeldrain")));
        assert!(all_links
            .iter()
            .any(|link| link.url.host_str() == Some("acefile.co")));
    }

    #[test]
    fn detail_without_title_is_soft404_error() {
        let err = parse_detail("<html><body>homepage</body></html>", &base(), &id()).unwrap_err();
        assert!(matches!(err, SourceError::Parse { .. }));
        assert!(err.to_string().contains("soft-404"));
    }

    #[test]
    fn title_suffixes_are_removed() {
        assert_eq!(
            clean_title("Ameku Takao no Suiri Karte BD Batch Subtitle Indonesia"),
            "Ameku Takao no Suiri Karte BD"
        );
        assert_eq!(
            clean_title("Solo Leveling Subtitle Indonesia"),
            "Solo Leveling"
        );
        assert_eq!(clean_title("One Piece"), "One Piece");
    }

    #[test]
    fn download_table_detection() {
        assert!(has_download_table(&fixture("kusonime/detail.html")));
        assert!(!has_download_table("<html></html>"));
    }
}
