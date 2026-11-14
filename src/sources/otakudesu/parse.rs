//! Parser murni Otakudesu (`otakudesu.blog`).
//!
//! Jebakan yang ditangani (DESIGN §22.2): `.epz`/`.epztipe` bermakna ganda
//! (ongoing: episode+hari; completed: total+skor), daftar episode newest-first,
//! mirror punya `href="#"` dengan payload base64 di `data-content`, `ul.m720p`
//! tidak ditutup dengan `</ul>`, dan label unduhan bervariasi (`360p`,
//! `Mp4 360p`, `MKV 1080p`).

use scraper::Html;
use serde::Deserialize;
use url::Url;

use crate::error::{SourceError, SourceResult};
use crate::model::{
    AiringStatus, AnimeDetail, AnimeSearchResult, Container, Episode, MediaKind, Resolution,
    SourceId,
};
use crate::sources::util;
use crate::sources::util::{abs_url, attr, select, select_first, select_in, text};

pub const NAME: &str = "otakudesu";

const PAGE_LIST: &str = "list";
const PAGE_DETAIL: &str = "detail";
const PAGE_EPISODE: &str = "episode";

/// Halaman daftar yang punya arti berbeda untuk `.epz`/`.epztipe`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListPage {
    Ongoing,
    Completed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirrorEntry {
    pub resolution: Option<Resolution>,
    pub index: u32,
    pub label: String,
    pub id: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostLink {
    pub host: String,
    pub url: Url,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadRow {
    pub resolution: Option<Resolution>,
    pub container: Option<Container>,
    pub label: String,
    pub links: Vec<HostLink>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EpisodePage {
    pub title: String,
    pub default_embed: Option<Url>,
    pub mirrors: Vec<MirrorEntry>,
    pub downloads: Vec<DownloadRow>,
}

/// Daftar ongoing/completed: `.venz li > .detpost`.
pub fn parse_list(
    html_text: &str,
    base: &Url,
    source: &SourceId,
    page: ListPage,
) -> SourceResult<Vec<AnimeSearchResult>> {
    let document = Html::parse_document(html_text);
    let items = select(&document, ".venz > ul > li > .detpost");
    let mut results = Vec::new();

    for item in &items {
        let Some(anchor) = select_in(*item, "div.thumb a").first().copied() else {
            continue;
        };
        let Some(href) = attr(&anchor, "href") else {
            continue;
        };
        let Some(url) = abs_url(base, &href) else {
            continue;
        };
        let title = select_in(*item, "h2.jdlflm")
            .first()
            .map(|element| text(element))
            .filter(|value| !value.is_empty());
        let Some(title) = title else { continue };

        let thumbnail = select_in(*item, "div.thumb img")
            .first()
            .and_then(|image| attr(image, "src"))
            .and_then(|src| abs_url(base, &src));

        let epz = select_in(*item, "div.epz").first().map(|el| text(el));
        let epztipe = select_in(*item, "div.epztipe").first().map(|el| text(el));

        let (status, score) = match page {
            ListPage::Ongoing => (Some(AiringStatus::Ongoing), None),
            ListPage::Completed => (
                Some(AiringStatus::Completed),
                epztipe.as_deref().and_then(util::parse_score),
            ),
        };

        // `.epz` = episode terakhir (ongoing) atau total episode (completed); tidak
        // ada field yang cocok di `AnimeSearchResult`, jadi sengaja tidak dipakai.
        let _ = epz;

        results.push(AnimeSearchResult {
            source: source.clone(),
            title,
            url,
            thumbnail,
            kind: None,
            status,
            score,
        });
    }

    if results.is_empty() && !html_text.contains("detpost") {
        return Err(SourceError::parse(
            NAME,
            PAGE_LIST,
            ".venz .detpost",
            "tidak ada kartu anime di halaman ini",
        ));
    }
    Ok(results)
}

/// Halaman pencarian `/?s=…&post_type=anime`.
pub fn parse_search(
    html_text: &str,
    base: &Url,
    source: &SourceId,
) -> SourceResult<Vec<AnimeSearchResult>> {
    let document = Html::parse_document(html_text);
    let items = select(&document, "ul.chivsrc > li");
    let mut results = Vec::new();

    for item in &items {
        let Some(anchor) = select_in(*item, "h2 a").first().copied() else {
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
        let thumbnail = select_in(*item, "img")
            .first()
            .and_then(|image| attr(image, "src"))
            .and_then(|src| abs_url(base, &src));

        let (status, score) = parse_search_sets(item);

        results.push(AnimeSearchResult {
            source: source.clone(),
            title,
            url,
            thumbnail,
            kind: None,
            status,
            score,
        });
    }

    if results.is_empty() && !html_text.contains("chivsrc") {
        return Err(SourceError::parse(
            NAME,
            PAGE_LIST,
            "ul.chivsrc li",
            "tidak ada hasil pencarian",
        ));
    }
    Ok(results)
}

fn parse_search_sets(item: &scraper::ElementRef<'_>) -> (Option<AiringStatus>, Option<f32>) {
    let mut status = None;
    let mut score = None;
    for set in select_in(*item, "div.set") {
        let label = select_in(set, "b")
            .first()
            .map(|element| text(element).to_ascii_lowercase())
            .unwrap_or_default();
        let value = text(&set);
        if label.starts_with("status") {
            status = Some(util::airing_status(&value));
        } else if label.starts_with("rating") {
            score = util::parse_score(value.split(':').nth(1).map(str::trim).unwrap_or_default());
        }
    }
    (status, score)
}

/// Halaman detail `/anime/<slug>/`.
pub fn parse_detail(html_text: &str, base: &Url, source: &SourceId) -> SourceResult<AnimeDetail> {
    let document = Html::parse_document(html_text);

    let title = select_first(&document, ".jdlrx > h1")
        .map(|element| text(&element))
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            SourceError::parse(
                NAME,
                PAGE_DETAIL,
                ".jdlrx h1",
                "judul anime tidak ditemukan",
            )
        })?;

    let poster = select_first(&document, ".fotoanime > img")
        .and_then(|image| attr(&image, "src"))
        .and_then(|src| abs_url(base, &src));

    let mut meta = std::collections::BTreeMap::new();
    for row in select(&document, ".fotoanime .infozin .infozingle p") {
        let label_elements = select_in(row, "b");
        let Some(label_element) = label_elements.first().copied() else {
            continue;
        };
        let label = text(&label_element).trim_end_matches(':').to_string();
        let full = text(&row);
        let value = full
            .split_once(':')
            .map(|(_, value)| value.trim().to_string())
            .unwrap_or_default();
        meta.insert(label, value);
    }

    let synopsis = select(&document, ".sinopc p")
        .iter()
        .map(|paragraph| text(paragraph))
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");

    let status = meta
        .get("Status")
        .map(|value| util::airing_status(value))
        .unwrap_or(AiringStatus::Unknown);
    let year = meta
        .get("Tanggal Rilis")
        .or_else(|| meta.get("Released"))
        .and_then(|value| util::parse_year(value));
    let score = meta.get("Skor").and_then(|value| util::parse_score(value));
    let total_episodes = meta
        .get("Total Episode")
        .and_then(|value| util::parse_episode_number(value));
    let kind = meta.get("Tipe").map(|value| util::media_kind(value));

    let alt_titles = ["Japanese", "Synonyms", "English"]
        .iter()
        .filter_map(|key| meta.get(*key))
        .filter(|value| !value.is_empty())
        .cloned()
        .collect::<Vec<_>>();

    let genres = meta
        .get("Genre")
        .map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|genre| !genre.is_empty())
                .map(ToString::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let episodes = parse_episode_block(&document, base);

    Ok(AnimeDetail {
        source: source.clone(),
        url: base.clone(),
        title,
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
}

fn parse_episode_block(document: &Html, base: &Url) -> Vec<Episode> {
    // Ada 2–3 blok `.episodelist`; hanya yang judulnya "Episode List" yang dipakai.
    let Some(block) = select(document, ".episodelist").into_iter().find(|block| {
        select_in(*block, ".monktit")
            .first()
            .map(|title| text(title).to_ascii_lowercase().contains("episode list"))
            .unwrap_or(false)
    }) else {
        return Vec::new();
    };

    let mut episodes: Vec<Episode> = select_in(block, "ul > li")
        .iter()
        .filter_map(|item| {
            let anchor = select_in(*item, "span > a").first().copied()?;
            let href = attr(&anchor, "href")?;
            let url = abs_url(base, &href)?;
            let label = text(&anchor);
            let number = util::parse_episode_number(&label);
            Some(Episode {
                number,
                label,
                title: None,
                url,
                released: None,
            })
        })
        .collect();

    episodes.reverse();
    episodes
}

/// Halaman episode: embed default + daftar mirror (payload base64) + unduhan.
pub fn parse_episode(html_text: &str, base: &Url, _source: &SourceId) -> SourceResult<EpisodePage> {
    let document = Html::parse_document(html_text);

    let title = select_first(&document, "h1.posttl")
        .map(|element| text(&element))
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            SourceError::parse(
                NAME,
                PAGE_EPISODE,
                "h1.posttl",
                "judul episode tidak ditemukan",
            )
        })?;

    let default_embed = select_first(&document, "#pembed iframe")
        .and_then(|iframe| attr(&iframe, "src"))
        .and_then(|src| abs_url(base, &src));

    let mut mirrors = Vec::new();
    for group in select(&document, ".mirrorstream ul") {
        // class `m360p`/`m480p`/`m720p`; kadang `<ul>` tidak ditutup dengan benar.
        let group_resolution = group.value().attr("class").and_then(util::parse_resolution);
        for anchor in select_in(group, "a[data-content]") {
            let Some(encoded) = attr(&anchor, "data-content") else {
                continue;
            };
            let Some(json_text) = util::b64decode_text(&encoded) else {
                continue;
            };
            #[derive(Deserialize)]
            struct Payload {
                id: u64,
                #[serde(default)]
                i: u32,
                #[serde(default)]
                q: String,
            }
            let Ok(payload) = serde_json::from_str::<Payload>(&json_text) else {
                continue;
            };
            let resolution = util::parse_resolution(&payload.q).or(group_resolution);
            mirrors.push(MirrorEntry {
                resolution,
                index: payload.i,
                label: text(&anchor),
                id: payload.id,
            });
        }
    }

    let downloads = parse_downloads(&document, base);

    Ok(EpisodePage {
        title,
        default_embed,
        mirrors,
        downloads,
    })
}

/// Blok `.download > ul > li` (episode) dan `.batchlink` (batch) berbagi bentuk baris.
pub fn parse_downloads(document: &Html, base: &Url) -> Vec<DownloadRow> {
    let mut rows = Vec::new();

    for item in select(document, ".download > ul > li") {
        let label = select_in(item, "strong")
            .first()
            .map(|element| text(element))
            .unwrap_or_default();
        let (container, resolution) = match parse_download_label(&label) {
            Some(parsed) => parsed,
            None if label.is_empty() => continue,
            // label aneh tetap ditampilkan, tetapi tanpa resolusi/container
            None => (None, None),
        };
        let links = select_in(item, "a[href]")
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
                resolution,
                container,
                label,
                links,
            });
        }
    }
    rows
}

/// `360p`, `Mp4 360p`, `MKV 1080p`, `MP4 720p` → (container, resolusi).
pub fn parse_download_label(label: &str) -> Option<(Option<Container>, Option<Resolution>)> {
    let resolution = util::parse_resolution(label);
    let lowered = label.to_ascii_lowercase();
    let container = if lowered.contains("mkv") {
        Some(Container::Mkv)
    } else if lowered.contains("mp4") {
        Some(Container::Mp4)
    } else {
        None
    };
    if resolution.is_none() && container.is_none() {
        return None;
    }
    Some((container, resolution))
}

/// Tipe media dari teks (dipakai bila perlu).
pub fn kind_from_label(label: &str) -> MediaKind {
    util::media_kind(label)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(path: &str) -> String {
        let full = format!("{}/tests/fixtures/{path}", env!("CARGO_MANIFEST_DIR"));
        std::fs::read_to_string(&full).unwrap_or_else(|err| panic!("baca {full}: {err}"))
    }

    fn base() -> Url {
        Url::parse("https://otakudesu.blog/").unwrap()
    }

    fn id() -> SourceId {
        SourceId::new(NAME)
    }

    #[test]
    fn ongoing_list_is_parsed() {
        let items = parse_list(
            &fixture("otakudesu/ongoing.html"),
            &base(),
            &id(),
            ListPage::Ongoing,
        )
        .unwrap();
        assert_eq!(items.len(), 25);
        assert_eq!(items[0].title, "Shuiro no Kamen");
        assert_eq!(items[0].status, Some(AiringStatus::Ongoing));
        assert!(items[0].url.as_str().contains("/anime/"));
        assert!(items[0].score.is_none(), "ongoing tidak punya skor");
    }

    #[test]
    fn completed_list_uses_score_not_weekday() {
        let items = parse_list(
            &fixture("otakudesu/completed.html"),
            &base(),
            &id(),
            ListPage::Completed,
        )
        .unwrap();
        assert!(!items.is_empty());
        assert!(items
            .iter()
            .all(|item| item.status == Some(AiringStatus::Completed)));
        assert!(
            items.iter().any(|item| item.score.is_some()),
            "completed harus memakai .epztipe sebagai skor"
        );
    }

    #[test]
    fn search_page_is_parsed() {
        let items = parse_search(&fixture("otakudesu/search.html"), &base(), &id()).unwrap();
        assert_eq!(items.len(), 5);
        assert!(items[0].title.contains("Kaguya-sama"));
        assert!(items
            .iter()
            .any(|item| item.status == Some(AiringStatus::Ongoing)));
    }

    #[test]
    fn detail_page_is_parsed_with_episode_list() {
        let detail = parse_detail(&fixture("otakudesu/detail.html"), &base(), &id()).unwrap();
        assert!(detail.title.contains("Super no Ura de Yani Suu Futari"));
        assert_eq!(detail.status, AiringStatus::Completed);
        assert_eq!(detail.total_episodes, Some(12));
        assert_eq!(detail.year, Some(2026));
        assert_eq!(detail.score, Some(8.20));
        assert!(detail.genres.contains(&"Comedy".to_string()));
        assert!(!detail.synopsis.is_empty());
        assert_eq!(detail.episodes.len(), 12);
        assert_eq!(detail.episodes[0].number, Some(1));
        assert_eq!(detail.episodes[11].number, Some(12));
    }

    #[test]
    fn episode_page_has_default_embed_and_mirrors() {
        let page = parse_episode(&fixture("otakudesu/episode.html"), &base(), &id()).unwrap();
        assert!(page.title.contains("Episode 1"));
        let embed = page.default_embed.expect("embed default harus ada");
        assert!(embed.as_str().contains("desustream"));
        // m360p kosong, m480p & m720p masing-masing 4 mirror
        assert_eq!(page.mirrors.len(), 8);
        assert!(page
            .mirrors
            .iter()
            .any(|mirror| mirror.resolution == Some(Resolution::P480)));
        assert!(page
            .mirrors
            .iter()
            .any(|mirror| mirror.resolution == Some(Resolution::P720)));
        assert!(page.mirrors.iter().all(|mirror| mirror.id > 0));
    }

    #[test]
    fn download_rows_handle_label_variants() {
        let document = Html::parse_document(&fixture("otakudesu/episode.html"));
        let rows = parse_downloads(&document, &base());
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].resolution, Some(Resolution::P360));
        assert!(rows[0].links.iter().any(|link| link.host == "PDrain"));

        for (label, resolution, container) in [
            ("Mp4 360p", Some(Resolution::P360), Some(Container::Mp4)),
            ("MKV 1080p", Some(Resolution::P1080), Some(Container::Mkv)),
            ("MP4 720p", Some(Resolution::P720), Some(Container::Mp4)),
            ("480p", Some(Resolution::P480), None),
        ] {
            let parsed = parse_download_label(label).unwrap();
            assert_eq!(parsed, (container, resolution), "label {label:?}");
        }
        assert!(parse_download_label("Batch").is_none());
    }
}
