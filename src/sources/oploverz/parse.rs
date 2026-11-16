//! Parser Oploverz — sumber utama adalah JSON API `backapi.oploverz.ac`.

use serde::Deserialize;
use url::Url;

use crate::error::{SourceError, SourceResult};
use crate::model::{AnimeDetail, AnimeSearchResult, Episode, SourceId};
use crate::sources::util;

pub const NAME: &str = "oploverz";

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Meta {
    #[serde(rename = "currentPage", default)]
    pub current_page: u32,
    #[serde(rename = "lastPage", default)]
    pub last_page: u32,
    #[serde(default)]
    pub total: u32,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct Genre {
    #[serde(default)]
    name: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct SeriesItem {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub slug: String,
    #[serde(default)]
    pub description: String,
    #[serde(rename = "releaseType", default)]
    pub release_type: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub poster: Option<String>,
    #[serde(default)]
    pub score: Option<f32>,
    #[serde(rename = "totalEpisodes", default)]
    pub total_episodes: Option<u32>,
    #[serde(rename = "japaneseTitle", default)]
    pub japanese_title: String,
    #[serde(default)]
    genres: Vec<Genre>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct StreamEntry {
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub url: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct EpisodeItem {
    #[serde(rename = "episodeNumber", default)]
    pub episode_number: Option<String>,
    #[serde(rename = "streamUrl", default)]
    pub stream_url: Vec<StreamEntry>,
    #[serde(rename = "releasedAt", default)]
    pub released_at: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ListEnvelope<T> {
    #[serde(default)]
    meta: Meta,
    #[serde(default)]
    data: T,
}

/// `/api/series?page=&q=` → daftar seri + meta paginasi.
pub fn parse_series(
    json_text: &str,
    source: &SourceId,
) -> SourceResult<(Vec<AnimeSearchResult>, Meta)> {
    let envelope: ListEnvelope<Vec<SeriesItem>> =
        serde_json::from_str(json_text).map_err(|err| {
            SourceError::parse(
                NAME,
                "list",
                "api.series",
                format!("JSON tidak sesuai: {err}"),
            )
        })?;
    let results = envelope
        .data
        .into_iter()
        .filter(|item| !item.title.is_empty())
        .map(|item| {
            let url = Url::parse(&format!("https://oploverz.diy/series/{}/", item.slug)).ok();
            AnimeSearchResult {
                source: source.clone(),
                title: item.title,
                url: url.unwrap_or_else(|| Url::parse("https://oploverz.diy/").expect("konstan")),
                thumbnail: item.poster.as_deref().and_then(|p| Url::parse(p).ok()),
                kind: Some(util::media_kind(&item.release_type)),
                status: Some(util::airing_status(&item.status)),
                score: item.score,
            }
        })
        .collect();
    Ok((results, envelope.meta))
}

/// `/api/series/{slug}` → satu objek seri.
pub fn parse_series_detail(json_text: &str, source: &SourceId) -> SourceResult<AnimeDetail> {
    // Endpoint tunggal membungkus objek: {"data": {...}}. Dukung bentuk langsung juga.
    #[derive(Debug, Deserialize)]
    struct DetailEnvelope {
        #[serde(default)]
        data: Option<SeriesItem>,
    }
    let item = match serde_json::from_str::<DetailEnvelope>(json_text) {
        Ok(DetailEnvelope { data: Some(item) }) => item,
        _ => serde_json::from_str::<SeriesItem>(json_text).map_err(|err| {
            SourceError::parse(
                NAME,
                "detail",
                "api.series",
                format!("JSON tidak sesuai: {err}"),
            )
        })?,
    };
    let url = Url::parse(&format!("https://oploverz.diy/series/{}/", item.slug))
        .unwrap_or_else(|_| Url::parse("https://oploverz.diy/").expect("konstan"));
    Ok(AnimeDetail {
        source: source.clone(),
        url,
        title: item.title,
        alt_titles: if item.japanese_title.is_empty() {
            Vec::new()
        } else {
            vec![item.japanese_title]
        },
        synopsis: item.description,
        genres: item.genres.into_iter().map(|genre| genre.name).collect(),
        kind: Some(util::media_kind(&item.release_type)),
        status: util::airing_status(&item.status),
        year: None,
        score: item.score,
        total_episodes: item.total_episodes,
        poster: item.poster.as_deref().and_then(|p| Url::parse(p).ok()),
        episodes: Vec::new(),
    })
}

/// Satu halaman episode beserta meta (dipakai client untuk mencari episode tertentu).
#[derive(Debug)]
pub struct EpisodeBatch {
    pub meta: Meta,
    pub items: Vec<EpisodeItem>,
}

/// Parse halaman episode apa adanya (tanpa konversi ke model).
pub fn parse_episode_batch(json_text: &str) -> SourceResult<EpisodeBatch> {
    let envelope: ListEnvelope<Vec<EpisodeItem>> =
        serde_json::from_str(json_text).map_err(|err| {
            SourceError::parse(
                NAME,
                "episodes",
                "api.episodes",
                format!("JSON tidak sesuai: {err}"),
            )
        })?;
    Ok(EpisodeBatch {
        meta: envelope.meta,
        items: envelope.data,
    })
}

/// `/api/series/{slug}/episodes?page=` → episode + meta.
pub fn parse_episodes(
    json_text: &str,
    slug: &str,
    _source: &SourceId,
) -> SourceResult<(Vec<Episode>, Meta)> {
    let envelope = parse_episode_batch(json_text)?;
    let episodes = envelope
        .items
        .into_iter()
        .filter_map(|item| {
            let number = item
                .episode_number
                .as_deref()
                .and_then(util::parse_episode_number);
            let number_label = number
                .map(|number| number.to_string())
                .or_else(|| item.episode_number.clone())
                .unwrap_or_default();
            if number_label.is_empty() {
                return None;
            }
            let url = Url::parse(&format!(
                "https://oploverz.diy/series/{slug}/episode/{number_label}"
            ))
            .ok()?;
            Some(Episode {
                number,
                label: format!("Episode {number_label}"),
                title: None,
                url,
                released: None,
            })
        })
        .collect();
    Ok((episodes, envelope.meta))
}

/// `streamUrl` → `StreamLink` embed (resolusi dari label "Nonton Online 720p").
pub fn parse_stream_entries(
    entries: &[StreamEntry],
    episode_url: &Url,
    source: &SourceId,
) -> Vec<crate::model::StreamLink> {
    entries
        .iter()
        .filter_map(|entry| {
            let url = Url::parse(entry.url.trim()).ok()?;
            Some(crate::model::StreamLink {
                source: source.clone(),
                resolution: util::parse_resolution(&entry.source),
                label: entry.source.clone(),
                server: crate::embed::classify_server(&url),
                container: None,
                url,
                headers: crate::model::RequestHeaders {
                    referer: Some(episode_url.clone()),
                    user_agent: None,
                },
                stream_kind: crate::model::StreamKind::Embed,
            })
        })
        .collect()
}

/// Status "berlangsung"/"ongoing" untuk filter browse.
pub fn is_ongoing(status: &str) -> bool {
    let lowered = status.to_ascii_lowercase();
    lowered.contains("berlangsung") || lowered.contains("ongoing")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(path: &str) -> String {
        let full = format!("{}/tests/fixtures/{path}", env!("CARGO_MANIFEST_DIR"));
        std::fs::read_to_string(&full).unwrap_or_else(|err| panic!("baca {full}: {err}"))
    }

    fn id() -> SourceId {
        SourceId::new(NAME)
    }

    #[test]
    fn series_search_is_parsed() {
        let (results, meta) =
            parse_series(&fixture("oploverz/api_series_q_naruto.json"), &id()).unwrap();
        assert_eq!(results.len(), 10);
        assert_eq!(meta.total, 13);
        assert!(results[0].title.to_lowercase().contains("naruto"));
        assert!(results[0].thumbnail.is_some());
    }

    #[test]
    fn episodes_and_streams_are_parsed() {
        let (episodes, meta) =
            parse_episodes(&fixture("oploverz/api_eps.json"), "one-piece", &id()).unwrap();
        assert_eq!(episodes.len(), 10);
        assert_eq!(meta.total, 1180);
        assert_eq!(episodes[0].number, Some(1180));
        let raw: ListEnvelope<Vec<EpisodeItem>> =
            serde_json::from_str(&fixture("oploverz/api_eps.json")).unwrap();
        let links = parse_stream_entries(&raw.data[0].stream_url, &episodes[0].url, &id());
        assert!(!links.is_empty());
        assert!(links.iter().any(|link| link.resolution.is_some()));
        assert!(links
            .iter()
            .all(|link| link.stream_kind == crate::model::StreamKind::Embed));
    }

    #[test]
    fn status_filter_helper() {
        assert!(is_ongoing("Berlangsung"));
        assert!(is_ongoing("Ongoing"));
        assert!(!is_ongoing("Completed"));
    }
}

#[cfg(test)]
mod detail_fixture_tests {
    use super::*;

    #[test]
    fn detail_envelope_fixture_parses_title() {
        let full = format!(
            "{}/tests/fixtures/oploverz/api_series_detail.json",
            env!("CARGO_MANIFEST_DIR")
        );
        let raw = std::fs::read_to_string(&full).unwrap();
        let detail = parse_series_detail(&raw, &SourceId::new(NAME)).unwrap();
        assert_eq!(detail.title, "Naruto Shippuden the Movie 7: The Last");
        assert_eq!(detail.genres.len(), 8);
        assert_eq!(detail.score, Some(7.79));
        assert_eq!(detail.status, crate::model::AiringStatus::Completed);
        assert_eq!(detail.total_episodes, Some(1));
        assert!(detail.poster.is_some());
    }
}
