//! Parser murni ani.pm (source berbahasa Inggris).
//!
//! ani.pm adalah SPA: sebagian halaman detail tidak di-prerender sehingga HTML-nya
//! hanya shell (`<title>ani.pm</title>`). Karena itu data diambil dari **API JSON**:
//! - pencarian: `GET /api/anime/search?q=…`
//! - detail + daftar episode + token player:
//!   `GET /api/anime/playback-bootstrap/settlar/<routeId>?ep=<n>&lang=sub`
//!   → `core` (metadata + `episodes[]`), `availability`, `settlarSelection`.

use serde::Deserialize;
use url::Url;

use crate::error::{SourceError, SourceResult};
use crate::model::{AiringStatus, AnimeDetail, AnimeSearchResult, Episode, SourceId};
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
    /// Provider asal (`settlar`, `anilist`, …) — menentukan prefix path & bootstrap.
    #[serde(default)]
    source: String,
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
        // Provider `anilist` hanya dilayani di `/ani/…`, `settlar` di `/anime/…`.
        let Some(url) = base
            .join(&format!("{}{}", path_prefix(&item.source), item.route_id))
            .ok()
        else {
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

/// `core.episodes[]` satu episode.
#[derive(Debug, Deserialize)]
struct RawEpisode {
    #[serde(default)]
    number: Option<f64>,
    #[serde(default)]
    title: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawCore {
    #[serde(default)]
    title: String,
    #[serde(default)]
    native: Option<String>,
    #[serde(default)]
    synopsis: Option<String>,
    #[serde(default)]
    poster: Option<String>,
    #[serde(default)]
    genres: Vec<String>,
    #[serde(default)]
    year: Option<u16>,
    #[serde(default)]
    score: Option<f32>,
    #[serde(default)]
    status: Option<String>,
    #[serde(rename = "type", default)]
    kind: Option<String>,
    #[serde(rename = "episodeCount", default)]
    episode_count: Option<u32>,
    #[serde(default)]
    episodes: Vec<RawEpisode>,
}

#[derive(Debug, Deserialize)]
struct RawBootstrap {
    #[serde(default)]
    core: Option<RawCore>,
}

/// Detail + daftar episode dari `playback-bootstrap`.
pub fn parse_detail(
    json_text: &str,
    page_url: &Url,
    base: &Url,
    source: &SourceId,
) -> SourceResult<AnimeDetail> {
    let raw: RawBootstrap = serde_json::from_str(json_text).map_err(|err| {
        SourceError::parse(
            NAME,
            PAGE_DETAIL,
            "json",
            format!("metadata bukan JSON (halaman mungkin tidak ada): {err}"),
        )
    })?;
    let core = raw.core.ok_or_else(|| {
        SourceError::not_found(NAME, "judul ini tidak punya metadata di ani.pm".to_string())
    })?;
    if core.title.trim().is_empty() {
        return Err(SourceError::parse(
            NAME,
            PAGE_DETAIL,
            "title",
            "judul kosong",
        ));
    }

    let episodes: Vec<Episode> = core
        .episodes
        .iter()
        .filter_map(|episode| {
            let number = episode.number.map(|value| value as u32)?;
            let prefix = path_prefix(provider_from_path(page_url));
            let url = base
                .join(&format!(
                    "{prefix}{}/episode-{number}",
                    route_id(page_url).unwrap_or_default()
                ))
                .ok()?;
            Some(Episode {
                number: Some(number),
                label: episode
                    .title
                    .clone()
                    .filter(|value| !value.trim().is_empty())
                    .unwrap_or_else(|| format!("Episode {number}")),
                title: episode.title.clone(),
                url,
                released: None,
            })
        })
        .collect();

    Ok(AnimeDetail {
        source: source.clone(),
        url: page_url.clone(),
        title: core.title,
        alt_titles: core.native.into_iter().collect(),
        synopsis: core.synopsis.unwrap_or_default(),
        genres: core.genres,
        kind: core.kind.as_deref().map(util::media_kind),
        status: core
            .status
            .as_deref()
            .map(util::airing_status)
            .unwrap_or(AiringStatus::Unknown),
        year: core.year,
        score: core.score,
        total_episodes: core.episode_count,
        poster: core.poster.and_then(|src| base.join(&src).ok()),
        episodes,
    })
}

/// Prefix path untuk provider (`anilist` → `/ani/`, lainnya `/anime/`).
pub fn path_prefix(provider: &str) -> &'static str {
    if provider.eq_ignore_ascii_case("anilist") {
        "/ani/"
    } else {
        "/anime/"
    }
}

/// Provider dari prefix path (`/ani/…` → anilist, `/anime/…` → settlar).
pub fn provider_from_path(url: &Url) -> &'static str {
    match url.path_segments().and_then(|mut segments| segments.next()) {
        Some("ani") => "anilist",
        _ => "settlar",
    }
}

/// Provider yang dicoba berurutan saat bootstrap (yang pertama paling mungkin benar).
pub fn provider_candidates(preferred: &str) -> Vec<&'static str> {
    if preferred.eq_ignore_ascii_case("anilist") {
        vec!["anilist", "settlar"]
    } else {
        vec!["settlar", "anilist"]
    }
}

/// `/anime/<routeId>[/episode-N]` → `routeId`.
pub fn route_id(url: &Url) -> Option<String> {
    let segments: Vec<&str> = url.path_segments()?.collect();
    let index = segments
        .iter()
        .position(|segment| *segment == "anime" || *segment == "ani")?;
    segments.get(index + 1).map(|slug| (*slug).to_string())
}

/// `/anime/<routeId>/episode-<n>` → `<n>`.
pub fn episode_number(url: &Url) -> Option<u32> {
    let segments: Vec<&str> = url.path_segments()?.collect();
    let index = segments
        .iter()
        .position(|segment| *segment == "anime" || *segment == "ani")?;
    segments
        .get(index + 2)?
        .strip_prefix("episode-")?
        .parse()
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::MediaKind;

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
    fn detail_from_bootstrap_has_all_episodes() {
        let page = base().join("/anime/naruto-958-cfreps").unwrap();
        let detail = parse_detail(
            &fixture("anipm/bootstrap.json"),
            &page,
            &base(),
            &SourceId::new(NAME),
        )
        .unwrap();
        assert_eq!(detail.title, "Naruto");
        assert!(!detail.synopsis.is_empty());
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
    fn movie_page_without_prerendered_html_still_works() {
        let page = base()
            .join("/anime/evangelion-2-0-you-can-not-advance-1431-eskifx")
            .unwrap();
        let detail = parse_detail(
            &fixture("anipm/bootstrap_movie.json"),
            &page,
            &base(),
            &SourceId::new(NAME),
        )
        .unwrap();
        assert_eq!(detail.title, "Evangelion: 2.0 You Can (Not) Advance");
        assert_eq!(detail.kind, Some(MediaKind::Movie));
        assert_eq!(detail.episodes.len(), 1);
    }

    #[test]
    fn anilist_search_uses_ani_prefix_and_detail_works() {
        let items = parse_search(
            &fixture("anipm/search_anilist.json"),
            &base(),
            &SourceId::new(NAME),
        )
        .unwrap();
        assert_eq!(items.len(), 1);
        // Provider `anilist` → path `/ani/`, bukan `/anime/`.
        assert!(
            items[0].url.as_str().contains("/ani/the-laid-off"),
            "{}",
            items[0].url
        );

        let page = items[0].url.clone();
        let detail = parse_detail(
            &fixture("anipm/bootstrap_anilist.json"),
            &page,
            &base(),
            &SourceId::new(NAME),
        )
        .unwrap();
        assert!(detail.title.to_lowercase().contains("laid-off"));
        assert_eq!(detail.episodes.len(), 7);
        // Episode mengikuti prefix halaman (`/ani/...`).
        assert!(
            detail.episodes[0].url.as_str().contains("/ani/"),
            "{}",
            detail.episodes[0].url
        );
    }

    #[test]
    fn provider_maps_to_path_prefix() {
        assert_eq!(path_prefix("anilist"), "/ani/");
        assert_eq!(path_prefix("settlar"), "/anime/");
        assert_eq!(path_prefix(""), "/anime/");
        let ani = base().join("/ani/x-1-abc").unwrap();
        let anime = base().join("/anime/x-1-abc").unwrap();
        assert_eq!(provider_from_path(&ani), "anilist");
        assert_eq!(provider_from_path(&anime), "settlar");
        assert_eq!(provider_candidates("anilist")[0], "anilist");
        assert_eq!(provider_candidates("settlar")[0], "settlar");
    }

    #[test]
    fn route_and_episode_are_parsed_from_url() {
        let url = base().join("/anime/naruto-958-cfreps/episode-12").unwrap();
        assert_eq!(route_id(&url).as_deref(), Some("naruto-958-cfreps"));
        assert_eq!(episode_number(&url), Some(12));
    }
}
