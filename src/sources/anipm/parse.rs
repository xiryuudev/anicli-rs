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
    #[serde(rename = "numberOfEpisodes", default)]
    number_of_episodes: Option<u32>,
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

    let series = series.ok_or_else(|| {
        SourceError::parse(
            NAME,
            PAGE_DETAIL,
            "application/ld+json",
            "metadata TVSeries tidak ditemukan di halaman detail",
        )
    })?;
    if series.name.trim().is_empty() {
        return Err(SourceError::parse(
            NAME,
            PAGE_DETAIL,
            "name",
            "judul kosong pada metadata",
        ));
    }

    let route_id = route_id_from_url(page_url).unwrap_or_default();
    let total = series.number_of_episodes.unwrap_or(0);
    let episodes = build_episodes(base, &route_id, total);

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

/// Bentuk daftar episode `n = 1..=total`.
pub fn build_episodes(base: &Url, route_id: &str, total: u32) -> Vec<Episode> {
    if route_id.is_empty() || total == 0 {
        return Vec::new();
    }
    (1..=total)
        .filter_map(|number| {
            let url = base
                .join(&format!("/anime/{route_id}/episode-{number}"))
                .ok()?;
            Some(Episode {
                number: Some(number),
                label: format!("Episode {number}"),
                title: None,
                url,
                released: None,
            })
        })
        .collect()
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
        assert_eq!(detail.total_episodes, Some(220));
        assert_eq!(detail.episodes.len(), 220);
        assert_eq!(detail.episodes[0].number, Some(1));
        assert!(detail.episodes[0]
            .url
            .as_str()
            .ends_with("/anime/naruto-958-cfreps/episode-1"));
        assert!(detail.poster.is_some());
        assert_eq!(detail.year, Some(2002));
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
