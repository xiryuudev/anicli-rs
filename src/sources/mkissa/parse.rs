//! Parser murni `mkissa.to` (English) — lewat GraphQL API `api.mkissa.net`.
//!
//! Website-nya SPA + challenge JS, tetapi API GraphQL-nya menerima query mentah
//! (introspection terbuka). Endpoint `episode` butuh crypto (`AA_CRYPTO_MISSING`),
//! jadi source ini: search + detail + daftar episode (tanpa streams).

use serde::Deserialize;
use url::Url;

use crate::error::{SourceError, SourceResult};
use crate::model::{AiringStatus, AnimeDetail, AnimeSearchResult, Episode, MediaKind, SourceId};
use crate::sources::util;

pub const NAME: &str = "mkissa";
const PAGE_LIST: &str = "list";
const PAGE_DETAIL: &str = "detail";

#[derive(Debug, Deserialize)]
struct GraphQl {
    #[serde(default)]
    data: Option<serde_json::Value>,
    #[serde(default)]
    errors: Vec<GraphQlError>,
}

#[derive(Debug, Deserialize)]
struct GraphQlError {
    #[serde(default)]
    message: String,
}

fn unwrap_graphql(json_text: &str) -> SourceResult<serde_json::Value> {
    let response: GraphQl = serde_json::from_str(json_text).map_err(|err| {
        SourceError::parse(
            NAME,
            PAGE_LIST,
            "json",
            format!("JSON GraphQL tidak valid: {err}"),
        )
    })?;
    if let Some(error) = response.errors.first() {
        return Err(SourceError::parse(
            NAME,
            PAGE_LIST,
            "graphql",
            format!("GraphQL error: {}", error.message),
        ));
    }
    response
        .data
        .ok_or_else(|| SourceError::parse(NAME, PAGE_LIST, "data", "respons GraphQL tanpa data"))
}

/// Search: `shows(search:{query}) { edges { _id name englishName thumbnail } }`.
pub fn parse_search(
    json_text: &str,
    base: &Url,
    source: &SourceId,
) -> SourceResult<Vec<AnimeSearchResult>> {
    let data = unwrap_graphql(json_text)?;
    let edges = data
        .pointer("/shows/edges")
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default();
    let mut results = Vec::new();
    for edge in edges {
        let Some(id) = edge.get("_id").and_then(|value| value.as_str()) else {
            continue;
        };
        let title = edge
            .get("englishName")
            .and_then(|value| value.as_str())
            .filter(|value| !value.trim().is_empty())
            .or_else(|| edge.get("name").and_then(|value| value.as_str()))
            .unwrap_or_default()
            .to_string();
        if title.is_empty() {
            continue;
        }
        let Some(url) = base.join(&format!("/anime/{id}")).ok() else {
            continue;
        };
        let thumbnail = edge
            .get("thumbnail")
            .and_then(|value| value.as_str())
            .and_then(|src| Url::parse(src).ok());
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
    Ok(results)
}

/// Detail + daftar episode dari `show(_id)` (memakai `availableEpisodesDetail`).
pub fn parse_detail(
    json_text: &str,
    page_url: &Url,
    base: &Url,
    source: &SourceId,
) -> SourceResult<AnimeDetail> {
    let data = unwrap_graphql(json_text)?;
    let show = data.get("show").ok_or_else(|| {
        SourceError::parse(NAME, PAGE_DETAIL, "show", "objek show tidak ada di respons")
    })?;

    let id = show
        .get("_id")
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .to_string();
    let title = show
        .get("englishName")
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
        .or_else(|| show.get("name").and_then(|value| value.as_str()))
        .unwrap_or_default()
        .to_string();
    if title.is_empty() {
        return Err(SourceError::parse(
            NAME,
            PAGE_DETAIL,
            "name",
            "judul kosong",
        ));
    }

    let description = show
        .get("description")
        .and_then(|value| value.as_str())
        .map(|raw| util::collapse_whitespace(&strip_tags(raw)))
        .unwrap_or_default();
    let thumbnail = show
        .get("thumbnail")
        .and_then(|value| value.as_str())
        .and_then(|src| Url::parse(src).ok());
    let score = show
        .get("score")
        .and_then(|value| value.as_f64())
        .map(|value| value as f32);
    let kind = show
        .get("type")
        .and_then(|value| value.as_str())
        .map(util::media_kind);
    let status = show
        .get("status")
        .and_then(|value| value.as_str())
        .map(util::airing_status)
        .unwrap_or(AiringStatus::Unknown);
    let episode_count = show.get("episodeCount").and_then(json_u32);
    let episodes = episodes_from_available(show, base, &id);

    Ok(AnimeDetail {
        source: source.clone(),
        url: page_url.clone(),
        title,
        alt_titles: show
            .get("nativeName")
            .and_then(|value| value.as_str())
            .map(str::to_string)
            .into_iter()
            .collect(),
        synopsis: description,
        genres: Vec::new(),
        kind: kind.or(Some(MediaKind::Tv)),
        status,
        year: None,
        score,
        total_episodes: episode_count,
        poster: thumbnail,
        episodes,
    })
}

/// `availableEpisodesDetail.sub` (fallback `dub`) → daftar episode menaik.
fn episodes_from_available(show: &serde_json::Value, base: &Url, id: &str) -> Vec<Episode> {
    let detail = show.get("availableEpisodesDetail");
    let numbers: Vec<String> = ["sub", "dub"]
        .iter()
        .find_map(|kind| {
            detail
                .and_then(|value| value.get(*kind))
                .and_then(|value| value.as_array())
                .filter(|items| !items.is_empty())
                .map(|items| {
                    items
                        .iter()
                        .filter_map(|item| item.as_str().map(str::to_string))
                        .collect()
                })
        })
        .unwrap_or_default();

    let mut episodes: Vec<Episode> = numbers
        .into_iter()
        .filter_map(|number| {
            let parsed = util::parse_episode_number(&number);
            let url = base.join(&format!("/anime/{id}/episode-{number}")).ok()?;
            Some(Episode {
                number: parsed,
                label: format!("Episode {number}"),
                title: None,
                url,
                released: None,
            })
        })
        .collect();
    episodes.sort_by_key(|episode| episode.number);
    episodes
}

/// API mkissa mengembalikan angka sebagai `String` pada beberapa field
/// (mis. `episodeCount: "12"`), jadi terima keduanya.
fn json_u32(value: &serde_json::Value) -> Option<u32> {
    value
        .as_u64()
        .map(|value| value as u32)
        .or_else(|| value.as_str().and_then(|raw| raw.trim().parse().ok()))
}

fn strip_tags(raw: &str) -> String {
    let mut output = String::with_capacity(raw.len());
    let mut in_tag = false;
    for ch in raw.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => output.push(ch),
            _ => {}
        }
    }
    util::decode_entities(&output)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(path: &str) -> String {
        let full = format!("{}/tests/fixtures/{path}", env!("CARGO_MANIFEST_DIR"));
        std::fs::read_to_string(&full).unwrap_or_else(|err| panic!("baca {full}: {err}"))
    }

    fn base() -> Url {
        Url::parse("https://mkissa.to/").unwrap()
    }

    #[test]
    fn search_results_are_parsed() {
        let items = parse_search(
            &fixture("mkissa/search.json"),
            &base(),
            &SourceId::new(NAME),
        )
        .unwrap();
        assert!(items.len() >= 3, "hasil: {}", items.len());
        assert!(items[0].title.to_lowercase().contains("naruto"));
        assert!(items[0].url.as_str().contains("/anime/"));
        assert!(items[0].thumbnail.is_some());
    }

    #[test]
    fn detail_parses_available_episodes() {
        let page = base().join("/anime/u85xuZjGw7eb6u3um").unwrap();
        let detail = parse_detail(
            &fixture("mkissa/detail.json"),
            &page,
            &base(),
            &SourceId::new(NAME),
        )
        .unwrap();
        assert_eq!(detail.title, "MF GHOST Season 2");
        assert!(!detail.synopsis.is_empty());
        assert!(detail.score.is_some());
        assert_eq!(detail.total_episodes, Some(12));
        assert_eq!(detail.episodes.len(), 12);
        assert_eq!(detail.episodes.first().unwrap().number, Some(1));
        assert_eq!(detail.episodes.last().unwrap().number, Some(12));
        assert!(detail.episodes[0]
            .url
            .as_str()
            .contains("/anime/u85xuZjGw7eb6u3um/episode-1"));
        assert!(detail.poster.is_some());
    }

    #[test]
    fn graphql_error_is_structured() {
        let err = parse_search(
            "{\"errors\":[{\"message\":\"AA_CRYPTO_MISSING\"}]}",
            &base(),
            &SourceId::new(NAME),
        )
        .unwrap_err();
        assert!(
            format!("{err:?}").contains("AA_CRYPTO_MISSING"),
            "pesan error harus menyimpan detail: {err:?}"
        );
    }
}
