//! Parser murni Kuramanime (Indonesia) — `v20.kuramanime.ing`.

use scraper::Html;
use url::Url;

use crate::error::{SourceError, SourceResult};
use crate::model::{AnimeDetail, AnimeSearchResult, Episode, MediaKind, SourceId};
use crate::sources::util;
use crate::sources::util::{abs_url, attr, select, select_first, text};

pub const NAME: &str = "kuramanime";
const PAGE_LIST: &str = "list";
const PAGE_DETAIL: &str = "detail";

/// Hasil pencarian `/anime?search=` (`div.product__item`).
pub fn parse_list(
    html_text: &str,
    base: &Url,
    source: &SourceId,
) -> SourceResult<Vec<AnimeSearchResult>> {
    let document = Html::parse_document(html_text);
    let mut results = Vec::new();
    for item in select(&document, "div.product__item") {
        let Some(anchor) = select_first_in(item, "a[href]") else {
            continue;
        };
        let Some(href) = attr(&anchor, "href") else {
            continue;
        };
        let Some(url) = abs_url(base, &href) else {
            continue;
        };
        // Judul ada di `h5 a` (kartu) atau teks anchor.
        let title = select_first_in(item, "h5")
            .map(|element| text(&element))
            .map(|value| util::collapse_whitespace(&value))
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| text(&anchor));
        if title.is_empty() {
            continue;
        }
        let thumbnail = select_first_in(item, "[data-setbg]")
            .and_then(|element| attr(&element, "data-setbg"))
            .and_then(|src| abs_url(base, &src));
        let score = select_first_in(item, ".ep span")
            .map(|element| text(&element))
            .and_then(|value| util::parse_score(&value));
        results.push(AnimeSearchResult {
            source: source.clone(),
            title,
            url,
            thumbnail,
            kind: Some(MediaKind::Tv),
            status: None,
            score,
        });
    }
    if results.is_empty() {
        let lowered = html_text.to_lowercase();
        if !(lowered.contains("tidak ditemukan")
            || lowered.contains("tidak ada hasil")
            || lowered.contains("data tidak ditemukan")
            || lowered.contains("animeList"))
        {
            return Err(SourceError::parse(
                NAME,
                PAGE_LIST,
                "div.product__item",
                "tidak ada kartu anime di halaman ini",
            ));
        }
    }
    Ok(results)
}

fn select_first_in<'a>(
    element: scraper::ElementRef<'a>,
    css: &str,
) -> Option<scraper::ElementRef<'a>> {
    util::select_in(element, css).into_iter().next()
}

/// Detail `/anime/<id>/<slug>`.
///
/// Kuramanime tidak memuat daftar episode di HTML; yang tersedia adalah **jumlah**
/// episode (`/properties/episodes/<n>`) dan URL episode yang berpola
/// `/anime/<id>/<slug>/episode/<n>`, jadi daftar episode dibentuk dari jumlah itu.
pub fn parse_detail(
    html_text: &str,
    page_url: &Url,
    source: &SourceId,
) -> SourceResult<AnimeDetail> {
    let document = Html::parse_document(html_text);
    let title = select_first(&document, "div.anime__details__title h3")
        .or_else(|| select_first(&document, "h1"))
        .map(|element| text(&element))
        .map(|value| util::collapse_whitespace(&value))
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            SourceError::parse(
                NAME,
                PAGE_DETAIL,
                "div.anime__details__title h3",
                "judul tidak ditemukan",
            )
        })?;

    let poster = select_first(&document, "div.anime__details__pic[data-setbg]")
        .or_else(|| select_first(&document, "[data-setbg]"))
        .and_then(|element| attr(&element, "data-setbg"))
        .and_then(|src| abs_url(page_url, &src));
    let synopsis = select_first(&document, "p#synopsisField")
        .map(|element| text(&element))
        .filter(|value| !value.is_empty())
        .unwrap_or_default();
    let genres: Vec<String> = select(&document, "a[href*=\"/properties/genre/\"]")
        .iter()
        .map(|anchor| text(anchor).trim_end_matches(',').to_string())
        .filter(|value| !value.is_empty())
        .collect();

    let total = select(&document, "a[href*=\"/properties/episodes/\"]")
        .iter()
        .filter_map(|anchor| attr(anchor, "href"))
        .filter_map(|href| {
            href.rsplit('/')
                .next()
                .and_then(|digits| digits.split('?').next())
                .and_then(|digits| digits.parse::<u32>().ok())
        })
        .next();

    // `/anime/<id>/<slug>` → `/anime/<id>/<slug>/episode/<n>`
    let base_path = page_url.path().trim_end_matches('/').to_string();
    let episodes: Vec<Episode> = (1..=total.unwrap_or(0))
        .filter_map(|number| {
            let url = page_url
                .join(&format!("{base_path}/episode/{number}"))
                .ok()?;
            Some(Episode {
                number: Some(number),
                label: format!("Episode {number}"),
                title: None,
                url,
                released: None,
            })
        })
        .collect();

    Ok(AnimeDetail {
        source: source.clone(),
        url: page_url.clone(),
        title,
        alt_titles: Vec::new(),
        synopsis,
        genres,
        kind: Some(MediaKind::Tv),
        status: crate::model::AiringStatus::Unknown,
        year: None,
        score: None,
        total_episodes: total,
        poster,
        episodes,
    })
}

/// Episode: halaman memuat `select#changeServer` (daftar server) dan player yang
/// dimuat lewat JS dengan parameter bertanda tangan → dibuka di browser.
pub fn parse_episode(
    html_text: &str,
    page_url: &Url,
    source: &SourceId,
) -> SourceResult<(String, Vec<String>)> {
    let document = Html::parse_document(html_text);
    let title = select_first(&document, "title")
        .map(|element| text(&element))
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| {
            page_url
                .path_segments()
                .and_then(|mut segments| segments.next_back())
                .unwrap_or("episode")
                .to_string()
        });
    let _ = source;
    let servers: Vec<String> = select(&document, "select#changeServer option")
        .iter()
        .map(|option| text(option))
        .filter(|value| !value.is_empty())
        .collect();
    Ok((title, servers))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(path: &str) -> String {
        let full = format!("{}/tests/fixtures/{path}", env!("CARGO_MANIFEST_DIR"));
        std::fs::read_to_string(&full).unwrap_or_else(|err| panic!("baca {full}: {err}"))
    }

    fn base() -> Url {
        Url::parse("https://v20.kuramanime.ing/").unwrap()
    }

    #[test]
    fn search_results_are_parsed() {
        let items = parse_list(
            &fixture("kuramanime/search.html"),
            &base(),
            &SourceId::new(NAME),
        )
        .unwrap();
        assert!(items.len() >= 5, "hasil: {}", items.len());
        assert!(items
            .iter()
            .all(|item| item.url.as_str().contains("/anime/")));
        assert!(items
            .iter()
            .any(|item| item.title.to_lowercase().contains("boruto")));
        assert!(items[0].thumbnail.is_some());
    }

    #[test]
    fn detail_builds_episodes_from_count() {
        let page = base()
            .join("/anime/3/boruto-naruto-next-generations")
            .unwrap();
        let detail = parse_detail(
            &fixture("kuramanime/detail.html"),
            &page,
            &SourceId::new(NAME),
        )
        .unwrap();
        assert!(detail.title.contains("Boruto"));
        assert_eq!(detail.total_episodes, Some(293));
        assert_eq!(detail.episodes.len(), 293);
        assert!(detail.episodes[0]
            .url
            .as_str()
            .ends_with("/anime/3/boruto-naruto-next-generations/episode/1"));
        assert!(!detail.genres.is_empty());
        assert!(detail.poster.is_some());
    }

    #[test]
    fn episode_page_lists_servers() {
        let page = base()
            .join("/anime/3/boruto-naruto-next-generations/episode/1")
            .unwrap();
        let (title, servers) = parse_episode(
            &fixture("kuramanime/episode.html"),
            &page,
            &SourceId::new(NAME),
        )
        .unwrap();
        assert!(!title.is_empty());
        assert!(servers
            .iter()
            .any(|name| name.to_lowercase().contains("kuramadrive")));
    }
}
