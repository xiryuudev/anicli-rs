//! Parser murni Animotvslash (English, tema Madara) — `animotvslash.org`.

use scraper::Html;
use url::Url;

use crate::error::{SourceError, SourceResult};
use crate::model::{AnimeDetail, AnimeSearchResult, Episode, MediaKind, SourceId};
use crate::sources::util;
use crate::sources::util::{abs_url, attr, select, select_first, select_in, text};

pub const NAME: &str = "animotvslash";
const PAGE_LIST: &str = "list";
const PAGE_DETAIL: &str = "detail";
const PAGE_EPISODE: &str = "episode";

/// Hasil pencarian/listing (`div.bsx > a.tip`).
pub fn parse_list(
    html_text: &str,
    base: &Url,
    source: &SourceId,
) -> SourceResult<Vec<AnimeSearchResult>> {
    let document = Html::parse_document(html_text);
    let mut results = Vec::new();
    for card in select(&document, "div.bsx a.tip[href]") {
        let Some(href) = attr(&card, "href") else {
            continue;
        };
        let Some(url) = abs_url(base, &href) else {
            continue;
        };
        let title = attr(&card, "title")
            .filter(|value| !value.trim().is_empty())
            .or_else(|| {
                select_in(card, "h4, .tt")
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
        let status = select_in(card, ".status, .epx")
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
        let lowered = html_text.to_lowercase();
        let no_results = lowered.contains("nothing found")
            || lowered.contains("no results")
            || lowered.contains("tidak ditemukan");
        if !no_results {
            return Err(SourceError::parse(
                NAME,
                PAGE_LIST,
                "div.bsx a.tip",
                "tidak ada kartu anime di halaman ini",
            ));
        }
    }
    Ok(results)
}

/// Detail `/anime/<slug>/`.
pub fn parse_detail(html_text: &str, base: &Url, source: &SourceId) -> SourceResult<AnimeDetail> {
    let document = Html::parse_document(html_text);
    let title = select_first(&document, "h1.entry-title")
        .or_else(|| select_first(&document, "div.infox h1"))
        .or_else(|| select_first(&document, "h1"))
        .map(|element| text(&element))
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            SourceError::parse(NAME, PAGE_DETAIL, "h1.entry-title", "judul tidak ditemukan")
        })?;

    let poster = select_first(&document, "div.thumb img, div.anime__details__pic img")
        .and_then(|image| attr(&image, "src"))
        .and_then(|src| abs_url(base, &src));
    let genres: Vec<String> = select(&document, "div.genxed a")
        .iter()
        .map(|anchor| text(anchor))
        .filter(|value| !value.is_empty())
        .collect();
    let synopsis = select_first(&document, "div.desc, div.synp")
        .map(|element| text(&element))
        .filter(|value| !value.is_empty())
        .unwrap_or_default();
    let status = select_first(
        &document,
        "div.infox .spe span, div.anime__details__text .status",
    )
    .map(|element| text(&element))
    .map(|value| util::airing_status(&value))
    .unwrap_or(crate::model::AiringStatus::Unknown);

    let mut episodes: Vec<Episode> = select(&document, "a[href*=\"-episode-\"]")
        .iter()
        .filter_map(|anchor| {
            let href = attr(anchor, "href")?;
            let url = abs_url(base, &href)?;
            let number = util::parse_episode_number(url.path())?;
            Some(Episode {
                number: Some(number),
                label: format!("Episode {number}"),
                title: None,
                url,
                released: None,
            })
        })
        .collect();
    episodes.sort_by_key(|episode| episode.number);
    episodes.dedup_by_key(|episode| episode.number);

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

/// Halaman episode: iframe player pihak ketiga (`*.p2pplay.pro`).
pub fn parse_episode(
    html_text: &str,
    base: &Url,
    _source: &SourceId,
) -> SourceResult<(String, Option<Url>)> {
    let document = Html::parse_document(html_text);
    let title = select_first(&document, "h1.entry-title")
        .or_else(|| select_first(&document, "h1"))
        .map(|element| text(&element))
        .filter(|value| !value.is_empty())
        .ok_or_else(|| SourceError::parse(NAME, PAGE_EPISODE, "h1", "judul episode tidak ada"))?;
    let iframe = select_first(&document, "iframe[src]")
        .and_then(|frame| attr(&frame, "src"))
        .and_then(|src| abs_url(base, &src));
    Ok((title, iframe))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(path: &str) -> String {
        let full = format!("{}/tests/fixtures/{path}", env!("CARGO_MANIFEST_DIR"));
        std::fs::read_to_string(&full).unwrap_or_else(|err| panic!("baca {full}: {err}"))
    }

    fn base() -> Url {
        Url::parse("https://animotvslash.org/").unwrap()
    }

    #[test]
    fn search_results_are_parsed() {
        let items = parse_list(
            &fixture("animotvslash/search.html"),
            &base(),
            &SourceId::new(NAME),
        )
        .unwrap();
        assert!(items.len() >= 5, "hasil: {}", items.len());
        assert!(items
            .iter()
            .any(|item| item.title.to_lowercase().contains("naruto")));
        assert!(items
            .iter()
            .all(|item| item.url.as_str().contains("/anime/")));
    }

    #[test]
    fn detail_lists_episodes() {
        let page = base()
            .join("/anime/boruto-naruto-next-generations-tv/")
            .unwrap();
        let detail = parse_detail(
            &fixture("animotvslash/detail_series.html"),
            &page,
            &SourceId::new(NAME),
        )
        .unwrap();
        assert!(detail.title.to_lowercase().contains("boruto"));
        assert!(
            detail.episodes.len() >= 100,
            "episode: {}",
            detail.episodes.len()
        );
        assert_eq!(detail.episodes[0].number, Some(1));
    }

    #[test]
    fn episode_page_has_embed_iframe() {
        let page = base().join("/naruto-x-ut-episode-1/").unwrap();
        let (_title, embed) = parse_episode(
            &fixture("animotvslash/episode.html"),
            &page,
            &SourceId::new(NAME),
        )
        .unwrap();
        let embed = embed.expect("iframe player harus ada");
        assert!(embed.as_str().contains("p2pplay"), "{embed}");
    }
}
