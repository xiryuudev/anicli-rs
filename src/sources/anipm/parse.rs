//! Parser murni ani.pm (source berbahasa Inggris).
//!
//! ani.pm adalah SPA, tapi menyediakan:
//! - JSON API pencarian: `GET /api/anime/search?q=…` (`items[]`);
//! - metadata di halaman detail lewat `<script type="application/ld+json">`
//!   (`TVSeries`: name, alternateName, image, description, genre, numberOfEpisodes);
//! - URL episode dapat dibentuk: `/anime/<routeId>/episode-<n>`.

use scraper::Html;
use serde::Deserialize;
use url::Url;

use crate::error::{SourceError, SourceResult};
use crate::model::{AiringStatus, AnimeDetail, AnimeSearchResult, Episode, MediaKind, SourceId};
use crate::sources::util;

pub const NAME: &str = "anipm";
const PAGE_LIST: &str = "list";
const PAGE_DETAIL: &str = "detail";

#[derive(Debug, Deserialize)]
struct RawSearch {
    #[serde(default)]
    items: Vec<RawItem>,
}

#[derive(Debug, Deserialize)]
struct RawItem {
    #[serde(default)]
    title: String,
    #[serde(rename = "routeId", default)]
    route_id: String,
    #[serde(default)]
    poster: Option<String>,
    #[serde(default)]
    score: Option<f32>,
    #[serde(default)]
    status: Option<String>,
    #[serde(rename = "type", default)]
    kind: Option<String>,
}

/// Parse JSON `/api/anime/search`.
pub fn parse_search(
    json_text: &str,
    base: &Url,
    source: &SourceId,
) -> SourceResult<Vec<AnimeSearchResult>> {
    let raw: RawSearch = serde_json::from_str(json_text).map_err(|err| {
        SourceError::parse(
            NAME,
            PAGE_LIST,
            "json",
            format!("JSON pencarian tidak valid: {err}"),
        )
    })?;
    let mut results = Vec::new();
    for item in raw.items {
        if item.title.is_empty() || item.route_id.is_empty() {
            continue;
        }
        let Some(url) = base.join(&format!("/anime/{}", item.route_id)).ok() else {
            continue;
        };
        let thumbnail = item
            .poster
            .as_deref()
            .filter(|value| !value.is_empty())
            .and_then(|src| base.join(src).ok());
        results.push(AnimeSearchResult {
            source: source.clone(),
            title: item.title,
            url,
            thumbnail,
            kind: item.kind.as_deref().map(util::media_kind),
            status: item.status.as_deref().map(util::airing_status),
            score: item.score,
        });
    }
    if results.is_empty() && !json_text.contains("\"items\"") {
        return Err(SourceError::parse(
            NAME,
            PAGE_LIST,
            "items",
            "respons pencarian bukan JSON yang diharapkan",
        ));
    }
    Ok(results)
}

/// Metadata `TVSeries` dari LD+JSON halaman detail.
#[derive(Debug, Default, serde::Deserialize)]
struct LdSeries {
    #[serde(default)]
    name: String,
    #[serde(rename = "alternateName", default)]
    alternate_name: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    image: Option<String>,
    #[serde(default)]
    genre: Vec<String>,
    #[serde(rename = "datePublished", default)]
    date_published: Option<String>,
}

/// Parse halaman detail: LD+JSON + daftar episode yang dibentuk dari `numberOfEpisodes`.
pub fn parse_detail(
    html_text: &str,
    page_url: &Url,
    base: &Url,
    source: &SourceId,
) -> SourceResult<AnimeDetail> {
    let document = Html::parse_document(html_text);
    let mut series: Option<LdSeries> = None;
    for script in crate::sources::util::select(&document, "script[type=\"application/ld+json\"]") {
        let raw = script.text().collect::<String>();
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw) else {
            continue;
        };
        let is_series = value
            .get("@type")
            .and_then(|kind| kind.as_str())
            .is_some_and(|kind| kind == "TVSeries" || kind == "Movie");
        if !is_series {
            continue;
        }
        if let Ok(parsed) = serde_json::from_value::<LdSeries>(value) {
            series = Some(parsed);
            break;
        }
    }

    let series = match series {
        Some(series) => series,
        None => {
            // ani.pm mengembalikan shell SPA untuk judul yang tidak punya halaman
            // (mis. film yang tidak di-host): <title>ani.pm</title> tanpa metadata.
            let page_title = crate::sources::util::select_first(&document, "title")
                .map(|element| crate::sources::util::text(&element))
                .unwrap_or_default();
            if page_title.trim().eq_ignore_ascii_case("ani.pm") || page_title.is_empty() {
                return Err(SourceError::not_found(
                    NAME,
                    "judul ini tidak tersedia di ani.pm (halaman kosong, bukan katalog)"
                        .to_string(),
                ));
            }
            return Err(SourceError::parse(
                NAME,
                PAGE_DETAIL,
                "application/ld+json",
                "metadata TVSeries tidak ditemukan di halaman detail",
            ));
        }
    };
    if series.name.trim().is_empty() {
        return Err(SourceError::parse(
            NAME,
            PAGE_DETAIL,
            "name",
            "judul kosong pada metadata",
        ));
    }

    let episodes = parse_episode_links(&document, base);
    // numberOfEpisodes bisa lebih kecil/lebih besar dari halaman episode yang benar
    // (mis. film = 1, CM = 35); utamakan jumlah link nyata.
    // Jangan memakai numberOfEpisodes untuk membentuk URL: halaman hanya memuat
    // sebagian episode (mis. Naruto 120 dari 220) dan beberapa judul tidak di-host.
    // Yang ditampilkan hanya episode yang benar-benar ada di halaman.
    let total = u32::try_from(episodes.len()).unwrap_or(0);

    let year = series
        .date_published
        .as_deref()
        .and_then(|raw| raw.get(0..4))
        .and_then(|year| year.parse::<u16>().ok());

    Ok(AnimeDetail {
        source: source.clone(),
        url: page_url.clone(),
        title: series.name,
        alt_titles: series.alternate_name.into_iter().collect(),
        synopsis: series.description.unwrap_or_default(),
        genres: series.genre,
        kind: Some(MediaKind::Tv),
        status: AiringStatus::Unknown,
        year,
        score: None,
        total_episodes: (total > 0).then_some(total),
        poster: series.image.and_then(|src| base.join(&src).ok()),
        episodes,
    })
}

/// `/anime/<routeId>[/episode-N]` → `routeId`.
pub fn route_id_from_url(url: &Url) -> Option<String> {
    let segments: Vec<&str> = url.path_segments()?.collect();
    let index = segments.iter().position(|segment| *segment == "anime")?;
    segments.get(index + 1).map(|slug| (*slug).to_string())
}

/// Ambil episode dari link nyata di halaman detail (`/anime/<id>/episode-N`).
fn parse_episode_links(document: &Html, base: &Url) -> Vec<Episode> {
    let mut episodes: Vec<Episode> =
        crate::sources::util::select(document, r#"a[href*="/episode-"]"#)
            .iter()
            .filter_map(|anchor| {
                let href = crate::sources::util::attr(anchor, "href")?;
                let url = crate::sources::util::abs_url(base, &href)?;
                // path = "/anime/<id>/episode-12" → ambil angka dari segmen terakhir
                // (`parse_episode_number` pada path penuh gagal karena "episode-12").
                let segment = url
                    .path_segments()
                    .and_then(|mut segments| segments.next_back())
                    .unwrap_or_default();
                let number = segment
                    .rsplit('-')
                    .next()
                    .and_then(|digits| digits.parse::<u32>().ok())
                    .or_else(|| crate::sources::util::parse_episode_number(segment))?;
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
    episodes
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(path: &str) -> String {
        let full = format!("{}/tests/fixtures/{path}", env!("CARGO_MANIFEST_DIR"));
        std::fs::read_to_string(&full).unwrap_or_else(|err| panic!("baca {full}: {err}"))
    }

    fn base() -> Url {
        Url::parse("https://ani.pm/").unwrap()
    }

    #[test]
    fn search_json_is_parsed() {
        let items =
            parse_search(&fixture("anipm/search.json"), &base(), &SourceId::new(NAME)).unwrap();
        assert!(items.len() >= 10, "hasil: {}", items.len());
        let naruto = items
            .iter()
            .find(|item| item.title == "Naruto")
            .expect("Naruto ada di fixture");
        assert!(naruto.url.as_str().contains("/anime/naruto-958-"));
        assert!(naruto.thumbnail.is_some());
    }

    #[test]
    fn detail_uses_ld_json_and_builds_episodes() {
        let page = base().join("/anime/naruto-958-cfreps").unwrap();
        let detail = parse_detail(
            &fixture("anipm/detail.html"),
            &page,
            &base(),
            &SourceId::new(NAME),
        )
        .unwrap();
        assert_eq!(detail.title, "Naruto");
        assert!(detail.synopsis.to_lowercase().contains("naruto"));
        assert!(!detail.genres.is_empty());
        // Halaman 1 memuat 120 episode (sisanya lewat paginasi/JS di situs).
        assert_eq!(detail.episodes.len(), 120);
        assert_eq!(detail.total_episodes, Some(120));
        assert_eq!(detail.episodes[0].number, Some(1));
        assert!(detail.episodes[0]
            .url
            .as_str()
            .ends_with("/anime/naruto-958-cfreps/episode-1"));
        assert!(detail.poster.is_some());
        assert_eq!(detail.year, Some(2002));
    }

    #[test]
    fn detail_with_real_episode_links() {
        let page = base()
            .join("/anime/japan-anima-tor-s-exhibition-1972-jexvbb")
            .unwrap();
        let detail = parse_detail(
            &fixture("anipm/detail_episodes.html"),
            &page,
            &base(),
            &SourceId::new(NAME),
        )
        .unwrap();
        assert_eq!(detail.episodes.len(), 35);
        assert_eq!(detail.episodes[0].number, Some(1));
        assert!(detail.episodes[0].url.as_str().ends_with("/episode-1"));
    }

    #[test]
    fn missing_page_is_reported_as_not_found() {
        let page = base()
            .join("/anime/evangelion-2-0-you-can-not-advance-1431-eskifx")
            .unwrap();
        let err = parse_detail(
            &fixture("anipm/not_found.html"),
            &page,
            &base(),
            &SourceId::new(NAME),
        )
        .unwrap_err();
        assert!(
            err.user_message()
                .to_lowercase()
                .contains("tidak ditemukan"),
            "{}",
            err.user_message()
        );
    }

    #[test]
    fn route_id_extraction() {
        let url = base().join("/anime/one-piece-1642-slrfiu").unwrap();
        assert_eq!(
            route_id_from_url(&url).as_deref(),
            Some("one-piece-1642-slrfiu")
        );
    }
}
