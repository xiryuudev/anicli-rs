//! Parser murni Animeindo (`anime-indo.lol`).
//!
//! Jebakan (research/animeindo.md): home memakai `div.ngiri` tetapi detail memakai
//! `div.ngirix`; poster lazy-load di `data-original`; semua episode ada di **satu**
//! `div.ep`; server mirror ada di `a.server[data-video]` (duplikat id `allmiror`
//! diabaikan); URL episode root-level dan tidak boleh dikonstruksi.

use scraper::Html;
use url::Url;

use crate::error::{SourceError, SourceResult};
use crate::model::{AiringStatus, AnimeDetail, AnimeSearchResult, Episode, MediaKind, SourceId};
use crate::sources::util;
use crate::sources::util::{abs_url, attr, select, select_first, select_in, text};

pub const NAME: &str = "animeindo";
const PAGE_LIST: &str = "list";
const PAGE_DETAIL: &str = "detail";
const PAGE_EPISODE: &str = "episode";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerButton {
    pub label: String,
    pub url: Url,
}

/// Home/latest: `div.list-anime` di dalam anchor.
pub fn parse_list(
    html_text: &str,
    base: &Url,
    source: &SourceId,
) -> SourceResult<Vec<AnimeSearchResult>> {
    let document = Html::parse_document(html_text);
    let mut results = Vec::new();

    for anchor in select(&document, "div.ngiri a") {
        let Some(card) = select_in(anchor, "div.list-anime").first().copied() else {
            continue;
        };
        let Some(href) = attr(&anchor, "href") else {
            continue;
        };
        let Some(url) = abs_url(base, &href) else {
            continue;
        };
        let title = select_in(card, "p")
            .first()
            .map(|element| text(element))
            .unwrap_or_default();
        if title.is_empty() {
            continue;
        }
        let image = select_in(card, "img.lazy")
            .first()
            .copied()
            .or_else(|| select_in(card, "img").first().copied());
        let thumbnail = image
            .and_then(|image| attr(&image, "data-original").or_else(|| attr(&image, "src")))
            .and_then(|src| abs_url(base, &src));

        results.push(AnimeSearchResult {
            source: source.clone(),
            title,
            url,
            thumbnail,
            kind: Some(MediaKind::Tv),
            status: None,
            score: None,
        });
    }

    if results.is_empty() && !html_text.contains("list-anime") {
        return Err(SourceError::parse(
            NAME,
            PAGE_LIST,
            "div.list-anime",
            "tidak ada kartu anime di halaman ini",
        ));
    }
    Ok(results)
}

/// Halaman hasil pencarian (setelah redirect ke `/search/<slug>/`).
///
/// Markup-nya berbeda dari home: `table.otable` dengan kolom thumb + deskripsi.
pub fn parse_search(
    html_text: &str,
    base: &Url,
    source: &SourceId,
) -> SourceResult<Vec<AnimeSearchResult>> {
    let document = Html::parse_document(html_text);
    let mut results = Vec::new();

    for row in select(&document, "table.otable tr") {
        let Some(anchor) = select_in(row, "td.videsc a").first().copied() else {
            continue;
        };
        let Some(href) = attr(&anchor, "href") else {
            continue;
        };
        let Some(url) = abs_url(base, &href) else {
            continue;
        };
        let title = text(&anchor);
        if title.is_empty() {
            continue;
        }
        let thumbnail = select_in(row, "td.vithumb img")
            .first()
            .and_then(|image| attr(image, "data-original").or_else(|| attr(image, "src")))
            .and_then(|src| abs_url(base, &src));
        results.push(AnimeSearchResult {
            source: source.clone(),
            title,
            url,
            thumbnail,
            kind: Some(MediaKind::Tv),
            status: None,
            score: None,
        });
    }

    if results.is_empty() && !html_text.contains("otable") {
        return Err(SourceError::parse(
            NAME,
            PAGE_LIST,
            "table.otable",
            "tidak ada hasil pencarian di halaman ini",
        ));
    }
    Ok(results)
}

/// Detail `/anime/<slug>/`.
pub fn parse_detail(html_text: &str, base: &Url, source: &SourceId) -> SourceResult<AnimeDetail> {
    let document = Html::parse_document(html_text);

    let title = select_first(&document, "div.ngirix h1.title")
        .or_else(|| select_first(&document, "h1.title"))
        .map(|element| text(&element))
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            SourceError::parse(
                NAME,
                PAGE_DETAIL,
                "div.ngirix h1.title",
                "judul tidak ditemukan",
            )
        })?;

    let poster = select_first(&document, "div.ngirix div.menu div.detail img")
        .and_then(|image| attr(&image, "data-original").or_else(|| attr(&image, "src")))
        .and_then(|src| abs_url(base, &src));

    let genres = select(&document, "div.ngirix div.detail li a[href^=\"/genres/\"]")
        .iter()
        .map(|anchor| text(anchor))
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();

    let synopsis = select(&document, "div.ngirix div.detail p")
        .iter()
        .map(|paragraph| text(paragraph))
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");

    let mut episodes: Vec<Episode> = select(&document, "div.ep a")
        .iter()
        .filter_map(|anchor| {
            let href = attr(anchor, "href")?;
            let url = abs_url(base, &href)?;
            let raw = text(anchor);
            let number = util::parse_episode_number(&raw);
            Some(Episode {
                number,
                label: number
                    .map(|number| format!("Episode {number}"))
                    .unwrap_or(raw),
                title: None,
                url,
                released: None,
            })
        })
        .collect();
    // Animeindo menyimpan episode dalam satu div; urutkan berdasarkan nomor.
    episodes.sort_by_key(|episode| episode.number);

    Ok(AnimeDetail {
        source: source.clone(),
        url: base.clone(),
        title,
        alt_titles: Vec::new(),
        synopsis,
        genres,
        kind: Some(MediaKind::Tv),
        status: AiringStatus::Unknown,
        year: None,
        score: None,
        total_episodes: u32::try_from(episodes.len()).ok(),
        poster,
        episodes,
    })
}

/// Halaman episode: iframe default + semua mirror `a.server[data-video]`.
pub fn parse_episode(
    html_text: &str,
    base: &Url,
    _source: &SourceId,
) -> SourceResult<(String, Option<u32>, Vec<ServerButton>)> {
    let document = Html::parse_document(html_text);
    let title = select_first(&document, "div.ngiri h1.title")
        .or_else(|| select_first(&document, "h1.title"))
        .map(|element| text(&element))
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            SourceError::parse(
                NAME,
                PAGE_EPISODE,
                "div.ngiri h1.title",
                "judul episode tidak ditemukan",
            )
        })?;
    let episode_number = util::parse_episode_number(&title);

    let mut servers = Vec::new();
    if let Some(src) = select_first(&document, "iframe#tontonin")
        .and_then(|iframe| attr(&iframe, "src"))
        .and_then(|src| abs_url(base, &src))
    {
        servers.push(ServerButton {
            label: "Default".to_string(),
            url: src,
        });
    }
    for anchor in select(&document, "div.servers a.server[data-video]") {
        let Some(video) = attr(&anchor, "data-video") else {
            continue;
        };
        let Some(url) = abs_url(base, &video) else {
            continue;
        };
        if servers.iter().any(|existing| existing.url == url) {
            continue;
        }
        servers.push(ServerButton {
            label: text(&anchor),
            url,
        });
    }

    if servers.is_empty() {
        return Err(SourceError::parse(
            NAME,
            PAGE_EPISODE,
            "div.servers a.server",
            "tidak ada server video di halaman episode",
        ));
    }
    Ok((title, episode_number, servers))
}

/// Cek apakah player page memuat media langsung (`<source src>`).
pub fn player_media(html_text: &str) -> Option<Url> {
    crate::embed::desustream_media(html_text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(path: &str) -> String {
        let full = format!("{}/tests/fixtures/{path}", env!("CARGO_MANIFEST_DIR"));
        std::fs::read_to_string(&full).unwrap_or_else(|err| panic!("baca {full}: {err}"))
    }

    fn base() -> Url {
        Url::parse("https://anime-indo.lol/").unwrap()
    }

    fn id() -> SourceId {
        SourceId::new(NAME)
    }

    #[test]
    fn search_results_use_otable_layout() {
        let items = parse_search(&fixture("animeindo/search.html"), &base(), &id()).unwrap();
        assert!(!items.is_empty());
        assert!(items[0].title.to_lowercase().contains("black clover"));
        assert!(items[0].url.as_str().contains("anime-indo"));
    }

    #[test]
    fn detail_parses_episode_links_from_single_div_ep() {
        let detail = parse_detail(&fixture("animeindo/detail.html"), &base(), &id()).unwrap();
        assert!(!detail.title.is_empty());
        assert!(!detail.episodes.is_empty());
        // Kronologis naik setelah di-reverse.
        let first = detail.episodes.first().unwrap().number;
        let last = detail.episodes.last().unwrap().number;
        assert!(
            first < last,
            "urutan episode harus naik: {first:?}..{last:?}"
        );
        assert!(detail.poster.is_some(), "poster lazy-load harus terbaca");
    }

    #[test]
    fn episode_page_lists_servers() {
        let (title, number, servers) =
            parse_episode(&fixture("animeindo/episode.html"), &base(), &id()).unwrap();
        assert!(title.to_lowercase().contains("episode"));
        assert_eq!(number, Some(7));
        assert!(!servers.is_empty());
        assert!(servers
            .iter()
            .any(|server| server.url.as_str().contains("xtwap")
                || server.url.as_str().contains("gdplayer")));
    }

    #[test]
    fn xtwap_player_exposes_googlevideo_source() {
        let html = fixture("animeindo/xtwap_btube3.html");
        let media = player_media(&html).expect("harus ada <source src>");
        assert!(media.as_str().contains("googlevideo"), "{media}");
    }
}
