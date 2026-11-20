//! Parser murni Anichin (donghua sub Indo, `anichin.blog`).
//!
//! Tema Madara: kartu `div.bsx > a.tip`, detail `/series/<slug>/`, daftar episode
//! `div.eplister ul li a` dengan `.epl-num`/`.epl-title`/`.epl-date`, dan player
//! berupa iframe di halaman episode.

use scraper::Html;
use url::Url;

use crate::error::{SourceError, SourceResult};
use crate::model::{AiringStatus, AnimeDetail, AnimeSearchResult, Episode, MediaKind, SourceId};
use crate::sources::util;
use crate::sources::util::{abs_url, attr, select, select_first, select_in, text};

pub const NAME: &str = "anichin";
const PAGE_LIST: &str = "list";
const PAGE_DETAIL: &str = "detail";
const PAGE_EPISODE: &str = "episode";

/// Hasil halaman pencarian/listing (`div.bsx`).
pub fn parse_list(
    html_text: &str,
    base: &Url,
    source: &SourceId,
) -> SourceResult<Vec<AnimeSearchResult>> {
    let document = Html::parse_document(html_text);
    let mut results = Vec::new();

    for card in select(
        &document,
        "div.bsx a.tip[href], article.bs div.bsx a.tip[href]",
    ) {
        let Some(href) = attr(&card, "href") else {
            continue;
        };
        let Some(url) = abs_url(base, &href) else {
            continue;
        };
        let title = attr(&card, "title")
            .filter(|value| !value.trim().is_empty())
            .or_else(|| {
                select_in(card, ".tt")
                    .first()
                    .map(|element| text(element))
                    .filter(|value| !value.is_empty())
            })
            .unwrap_or_default();
        if title.is_empty() {
            continue;
        }
        let thumbnail = select_in(card, "img")
            .first()
            .and_then(|image| attr(image, "src"))
            .and_then(|src| abs_url(base, &src));
        let status = select_in(card, ".epx")
            .first()
            .map(|element| text(element))
            .map(|value| util::airing_status(&value));
        results.push(AnimeSearchResult {
            source: source.clone(),
            title,
            url,
            thumbnail,
            kind: Some(MediaKind::Tv),
            status,
            score: None,
        });
    }

    if results.is_empty() {
        // Halaman tanpa hasil memakai layout lain ("Tidak ditemukan"), bukan error.
        let lowered = html_text.to_lowercase();
        let no_results = lowered.contains("tidak ditemukan")
            || lowered.contains("nothing found")
            || html_text.contains("listupd");
        if !no_results {
            return Err(SourceError::parse(
                NAME,
                PAGE_LIST,
                "div.bsx a.tip",
                "tidak ada kartu donghua di halaman ini",
            ));
        }
    }
    Ok(results)
}

/// Detail `/series/<slug>/`.
pub fn parse_detail(html_text: &str, base: &Url, source: &SourceId) -> SourceResult<AnimeDetail> {
    let document = Html::parse_document(html_text);

    let title = select_first(&document, "h1.entry-title")
        .or_else(|| select_first(&document, "h1"))
        .or_else(|| select_first(&document, "div.infox h1"))
        .map(|element| text(&element))
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            SourceError::parse(NAME, PAGE_DETAIL, "h1.entry-title", "judul tidak ditemukan")
        })?;

    let poster = select_first(&document, "div.thumb img")
        .and_then(|image| attr(&image, "src"))
        .and_then(|src| abs_url(base, &src));

    let genres: Vec<String> = select(&document, "div.genxed a")
        .iter()
        .map(|anchor| text(anchor))
        .filter(|value| !value.is_empty())
        .collect();

    let synopsis = select_first(&document, "div.desc")
        .map(|element| text(&element))
        .filter(|value| !value.is_empty())
        .unwrap_or_default();

    let mut episodes: Vec<Episode> = select(&document, "div.eplister ul li a[href]")
        .iter()
        .filter_map(|anchor| {
            let href = attr(anchor, "href")?;
            let url = abs_url(base, &href)?;
            let number = select_in(*anchor, ".epl-num")
                .first()
                .map(|element| text(element))
                .and_then(|raw| util::parse_episode_number(&raw));
            let label = select_in(*anchor, ".epl-title")
                .first()
                .map(|element| text(element))
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| {
                    number
                        .map(|number| format!("Episode {number}"))
                        .unwrap_or_else(|| "Episode".to_string())
                });
            Some(Episode {
                number,
                label,
                title: None,
                url,
                released: None,
            })
        })
        .collect();
    episodes.reverse(); // sumber menampilkan terbaru-dulu

    let status = select_first(&document, "div.anime-status")
        .map(|element| text(&element))
        .map(|value| util::airing_status(&value))
        .unwrap_or(AiringStatus::Unknown);

    Ok(AnimeDetail {
        source: source.clone(),
        url: base.clone(),
        title,
        alt_titles: Vec::new(),
        synopsis,
        genres,
        kind: Some(MediaKind::Tv),
        status,
        year: None,
        score: None,
        total_episodes: u32::try_from(episodes.len()).ok(),
        poster,
        episodes,
    })
}

/// Halaman episode: iframe player (sering `rmb.anichin.bio`).
pub fn parse_episode(
    html_text: &str,
    base: &Url,
    _source: &SourceId,
) -> SourceResult<(String, Option<Url>)> {
    let document = Html::parse_document(html_text);
    let title = select_first(&document, "h1.entry-title")
        .or_else(|| select_first(&document, "h1"))
        .or_else(|| select_first(&document, "div.infox h1"))
        .map(|element| text(&element))
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            SourceError::parse(
                NAME,
                PAGE_EPISODE,
                "h1.entry-title",
                "judul episode tidak ada",
            )
        })?;

    Ok((title, player_url(&document, base)))
}

/// Ambil URL player dari halaman episode.
///
/// Tema Anichin menyimpan mirror di `<select.mirror>` sebagai **base64** berisi
/// potongan HTML `<iframe src="…">`, beberapa episode juga punya iframe langsung.
fn player_url(document: &Html, base: &Url) -> Option<Url> {
    use base64::Engine as _;

    if let Some(src) = select_first(document, "iframe[src]")
        .and_then(|frame| attr(&frame, "src"))
        .and_then(|src| abs_url(base, &src))
    {
        return Some(src);
    }
    for option in &select(document, "select.mirror option[value]") {
        let Some(encoded) = attr(option, "value") else {
            continue;
        };
        if encoded.trim().is_empty() {
            continue;
        }
        let Ok(decoded) = base64::engine::general_purpose::STANDARD.decode(encoded.trim()) else {
            continue;
        };
        let Ok(fragment) = String::from_utf8(decoded) else {
            continue;
        };
        let inner = Html::parse_fragment(&fragment);
        if let Some(src) = select_first(&inner, "iframe[src]")
            .and_then(|frame| attr(&frame, "src"))
            .and_then(|src| abs_url(base, &src))
        {
            return Some(src);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(path: &str) -> String {
        let full = format!("{}/tests/fixtures/{path}", env!("CARGO_MANIFEST_DIR"));
        std::fs::read_to_string(&full).unwrap_or_else(|err| panic!("baca {full}: {err}"))
    }

    fn base() -> Url {
        Url::parse("https://z1.anichin.blog/").unwrap()
    }

    fn id() -> SourceId {
        SourceId::new(NAME)
    }

    #[test]
    fn search_results_are_parsed() {
        let items = parse_list(&fixture("anichin/search.html"), &base(), &id()).unwrap();
        assert!(items.len() >= 5, "hasil: {}", items.len());
        assert!(items
            .iter()
            .any(|item| item.title.to_lowercase().contains("martial")));
        assert!(items
            .iter()
            .all(|item| item.url.as_str().contains("/series/")));
        assert!(items[0].thumbnail.is_some());
    }

    #[test]
    fn empty_search_is_not_a_parse_error() {
        let items = parse_list(&fixture("anichin/search_empty.html"), &base(), &id()).unwrap();
        assert!(items.is_empty());
    }

    #[test]
    fn detail_parses_episodes_and_genres() {
        let detail = parse_detail(&fixture("anichin/series.html"), &base(), &id()).unwrap();
        assert_eq!(detail.title, "Oriental Martial Academy");
        assert!(detail.genres.iter().any(|genre| genre == "Action"));
        assert!(!detail.synopsis.is_empty());
        assert!(detail.episodes.len() >= 5);
        // Episode terakhir (terbaru) bernomor 19 pada fixture.
        assert_eq!(detail.episodes.last().unwrap().number, Some(19));
        assert!(detail.poster.is_some());
    }

    #[test]
    fn episode_page_exposes_embed_iframe() {
        let (_title, embed) =
            parse_episode(&fixture("anichin/episode.html"), &base(), &id()).unwrap();
        let embed = embed.expect("iframe player harus ada");
        assert!(embed.as_str().contains("anichin"), "{embed}");
    }
}
