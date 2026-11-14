//! Parser murni Anoboy (`anoboy.id`, mirror `anoboy.nl`).
//!
//! Jebakan yang ditangani (research/anoboy.md): `.epx` berisi nomor episode di
//! homepage tetapi status di `/series/`; judul seri ada di `.tt` (text node) sementara
//! `.tt h2` adalah judul episode; nomor episode tidak punya elemen di halaman episode
//! (regex dari judul); daftar episode berasal dari `.eplister` (descending).

use scraper::Html;
use url::Url;

use crate::error::{SourceError, SourceResult};
use crate::model::{
    AiringStatus, AnimeDetail, AnimeSearchResult, Container, Episode, MediaKind, Resolution,
    SourceId,
};
use crate::sources::util;
use crate::sources::util::{abs_url, attr, select, select_first, select_in, text};

pub const NAME: &str = "anoboy";
const PAGE_LIST: &str = "list";
const PAGE_DETAIL: &str = "detail";
const PAGE_EPISODE: &str = "episode";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerIframe {
    pub url: Url,
    pub label: String,
    pub resolution: Option<Resolution>,
}

/// `article.bs` (home, search, /series/).
pub fn parse_list(
    html_text: &str,
    base: &Url,
    source: &SourceId,
    status: Option<AiringStatus>,
) -> SourceResult<Vec<AnimeSearchResult>> {
    let document = Html::parse_document(html_text);
    let mut results = Vec::new();

    for card in select(&document, "article.bs") {
        let Some(anchor) = select_in(card, "a[itemprop=\"url\"]").first().copied() else {
            continue;
        };
        let Some(href) = attr(&anchor, "href") else {
            continue;
        };
        let Some(url) = abs_url(base, &href) else {
            continue;
        };
        // Judul seri: text node LANGSUNG pertama `.tt` (anak <h2> adalah judul
        // episode; `text()` akan menggabungkan keduanya).
        let series_title = select_in(card, ".tt")
            .first()
            .and_then(|element| {
                element
                    .children()
                    .filter_map(|node| node.value().as_text())
                    .map(|text| util::collapse_whitespace(text))
                    .find(|value| !value.is_empty())
            })
            .filter(|value| !value.is_empty());
        let title = series_title
            .or_else(|| {
                select_in(card, ".tt h2")
                    .first()
                    .map(|element| text(element))
            })
            .unwrap_or_default();
        if title.is_empty() {
            continue;
        }
        let thumbnail = select_in(card, "img.ts-post-image")
            .first()
            .and_then(|image| attr(image, "src"))
            .and_then(|src| abs_url(base, &src));

        let badge = select_in(card, ".limit .bt .epx")
            .first()
            .map(|element| text(element))
            .unwrap_or_default();
        let status = status.or_else(|| {
            if badge.is_empty() {
                None
            } else if util::airing_status(&badge) != AiringStatus::Unknown {
                Some(util::airing_status(&badge))
            } else {
                Some(AiringStatus::Ongoing)
            }
        });

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

    // Halaman kosong sah (search "Not Found"); markup hilang = struktur berubah.
    if results.is_empty()
        && !html_text.contains(r#"article class="bs"#)
        && !html_text.contains("Not Found")
    {
        return Err(SourceError::parse(
            NAME,
            PAGE_LIST,
            "article.bs",
            "tidak ada kartu anime dan bukan empty-state",
        ));
    }
    Ok(results)
}

/// Detail series `/series/<slug>/`.
pub fn parse_detail(html_text: &str, base: &Url, source: &SourceId) -> SourceResult<AnimeDetail> {
    let document = Html::parse_document(html_text);

    let title = select_first(&document, "h1.entry-title")
        .map(|element| text(&element))
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            SourceError::parse(
                NAME,
                PAGE_DETAIL,
                "h1.entry-title",
                "judul series tidak ditemukan",
            )
        })?;

    let poster = select(&document, ".thumbook img.ts-post-image")
        .first()
        .and_then(|image| attr(image, "src"))
        .and_then(|src| abs_url(base, &src));

    let mut meta = std::collections::BTreeMap::new();
    for span in select(&document, ".info-content .spe span") {
        let full = text(&span);
        let Some((label, value)) = full.split_once(':') else {
            continue;
        };
        meta.insert(label.trim().to_string(), value.trim().to_string());
    }

    let genres = select(&document, ".genxed a")
        .iter()
        .map(|anchor| text(anchor))
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();

    let synopsis = select(&document, ".synp .entry-content p")
        .iter()
        .map(|paragraph| text(paragraph))
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");

    let alt_titles = select(&document, ".alter")
        .first()
        .map(|element| {
            text(element)
                .split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToString::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let status = meta
        .get("Status")
        .map(|value| util::airing_status(value))
        .unwrap_or(AiringStatus::Unknown);
    let kind = meta.get("Type").map(|value| util::media_kind(value));
    let year = meta
        .get("Released")
        .and_then(|value| util::parse_year(value));

    let mut episodes = parse_episode_list(&document, base);
    episodes.reverse();

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
        score: None,
        total_episodes: u32::try_from(episodes.len()).ok(),
        poster,
        episodes,
    })
}

fn parse_episode_list(document: &Html, base: &Url) -> Vec<Episode> {
    select(document, ".eplister li a")
        .iter()
        .filter_map(|anchor| {
            let href = attr(anchor, "href")?;
            let url = abs_url(base, &href)?;
            let number = select_in(*anchor, ".epl-num")
                .first()
                .map(|element| text(element))
                .and_then(|value| util::parse_episode_number(&value))
                .or_else(|| {
                    select_in(*anchor, ".epl-title")
                        .first()
                        .and_then(|element| util::parse_episode_number(&text(element)))
                });
            let label = select_in(*anchor, ".epl-title")
                .first()
                .map(|element| text(element))
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| {
                    number
                        .map(|number| format!("Episode {number}"))
                        .unwrap_or_default()
                });
            Some(Episode {
                number,
                label,
                title: None,
                url,
                released: None,
            })
        })
        .collect()
}

/// Halaman episode: iframe default + server mirror (base64 di `<select>`).
pub fn parse_episode(
    html_text: &str,
    base: &Url,
    _source: &SourceId,
) -> SourceResult<(String, Option<u32>, Vec<PlayerIframe>)> {
    let document = Html::parse_document(html_text);

    let title = select_first(&document, "h1.entry-title")
        .map(|element| text(&element))
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            SourceError::parse(
                NAME,
                PAGE_EPISODE,
                "h1.entry-title",
                "judul episode tidak ditemukan",
            )
        })?;
    let episode_number = util::parse_episode_number(&title);

    let mut iframes: Vec<PlayerIframe> = Vec::new();
    if let Some(src) = select_first(&document, "#pembed iframe")
        .or_else(|| select_first(&document, ".player-embed iframe"))
        .and_then(|iframe| attr(&iframe, "src"))
        .and_then(|src| abs_url(base, &src))
    {
        iframes.push(PlayerIframe {
            url: src,
            label: "Default".to_string(),
            resolution: None,
        });
    }

    for option in select(&document, "select.mirror option[value]") {
        let Some(encoded) = attr(&option, "value") else {
            continue;
        };
        if encoded.is_empty() {
            continue;
        }
        let Some(decoded) = util::b64decode_text(&encoded) else {
            continue;
        };
        let Some(src) = crate::embed::iframe_src(&decoded) else {
            continue;
        };
        let Ok(url) = Url::parse(src.trim()) else {
            continue;
        };
        iframes.push(PlayerIframe {
            url,
            label: "Mirror".to_string(),
            resolution: None,
        });
    }

    if iframes.is_empty() {
        return Err(SourceError::parse(
            NAME,
            PAGE_EPISODE,
            "streams",
            "halaman episode tidak memuat iframe player",
        ));
    }
    Ok((title, episode_number, iframes))
}

/// Streaming dari Anoboy: iframe Blogger bisa di-resolve ke MP4 `googlevideo`.
pub async fn resolve_streams(
    client: &crate::sources::anoboy::client::Anoboy,
    episode_url: &str,
) -> SourceResult<Vec<crate::model::StreamLink>> {
    let page_url = Url::parse(episode_url).map_err(|err| {
        SourceError::parse(NAME, PAGE_EPISODE, "url", format!("URL tidak valid: {err}"))
    })?;
    let html = client.fetch_text(&page_url).await?;
    let (_title, _number, iframes) = parse_episode(&html, &page_url, client.source_id())?;

    let mut links = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for iframe in iframes {
        if crate::embed::classify_server(&iframe.url) == crate::model::ServerKind::Blogspot {
            if let Ok(Some(link)) = client.resolve_blogger(&iframe.url).await {
                if seen.insert(link.url.to_string()) {
                    links.push(link);
                }
                continue;
            }
        }
        let server = crate::embed::classify_server(&iframe.url);
        let link = crate::model::StreamLink {
            source: client.source_id().clone(),
            resolution: iframe.resolution,
            label: format!("{server} {}", iframe.label),
            server,
            container: None,
            url: iframe.url.clone(),
            headers: crate::model::RequestHeaders {
                referer: Some(page_url.clone()),
                user_agent: None,
            },
            stream_kind: crate::model::StreamKind::Embed,
        };
        if seen.insert(link.url.to_string()) {
            links.push(link);
        }
    }

    if links.is_empty() {
        return Err(SourceError::parse(
            NAME,
            PAGE_EPISODE,
            "streams",
            "tidak ada server yang bisa dipakai",
        ));
    }
    Ok(links)
}

/// Read container from a label (dipakai helper download nanti).
pub fn container_from_label(label: &str) -> Option<Container> {
    let lowered = label.to_ascii_lowercase();
    if lowered.contains("mkv") {
        Some(Container::Mkv)
    } else if lowered.contains("mp4") {
        Some(Container::Mp4)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(path: &str) -> String {
        let full = format!("{}/tests/fixtures/{path}", env!("CARGO_MANIFEST_DIR"));
        std::fs::read_to_string(&full).unwrap_or_else(|err| panic!("baca {full}: {err}"))
    }

    fn base() -> Url {
        Url::parse("https://anoboy.id/").unwrap()
    }

    fn id() -> SourceId {
        SourceId::new(NAME)
    }

    #[test]
    fn home_list_uses_episode_urls_and_titles() {
        let items = parse_list(&fixture("anoboy/home.html"), &base(), &id(), None).unwrap();
        assert_eq!(items.len(), 49);
        assert!(!items[0].title.is_empty());
        assert!(items[0].url.as_str().contains("anoboy"));
        assert!(items.iter().all(|item| item.kind == Some(MediaKind::Tv)));
    }

    #[test]
    fn search_results_and_empty_state() {
        let items = parse_list(&fixture("anoboy/search.html"), &base(), &id(), None).unwrap();
        assert!(!items.is_empty());
        assert!(items[0].title.to_lowercase().contains("nige"));

        let empty = parse_list(&fixture("anoboy/search_empty.html"), &base(), &id(), None).unwrap();
        assert!(empty.is_empty(), "empty-state tidak boleh jadi error");
    }

    #[test]
    fn series_list_has_status_badges() {
        let items = parse_list(&fixture("anoboy/list.html"), &base(), &id(), None).unwrap();
        assert_eq!(items.len(), 10);
        assert_eq!(items[0].status, Some(AiringStatus::Ongoing));
    }

    #[test]
    fn detail_parses_metadata_and_episodes() {
        let detail = parse_detail(&fixture("anoboy/detail.html"), &base(), &id()).unwrap();
        assert!(detail.title.to_lowercase().contains("nige"));
        assert_eq!(detail.status, AiringStatus::Ongoing);
        assert_eq!(detail.episodes.len(), 8);
        assert_eq!(detail.episodes[0].number, Some(1));
        assert_eq!(detail.episodes[7].number, Some(8));
        assert!(detail.genres.contains(&"Adventure".to_string()));
        assert!(!detail.synopsis.is_empty());
        assert!(detail
            .alt_titles
            .iter()
            .any(|title| title.contains("Elusive")));
    }

    #[test]
    fn episode_page_extracts_iframe() {
        let (title, number, iframes) =
            parse_episode(&fixture("anoboy/episode.html"), &base(), &id()).unwrap();
        assert!(title.to_lowercase().contains("episode"));
        assert_eq!(number, Some(14));
        assert!(!iframes.is_empty());
        assert!(iframes[0].url.as_str().contains("blogger.com"));
    }

    #[test]
    fn container_labels() {
        assert_eq!(container_from_label("MKV 720p"), Some(Container::Mkv));
        assert_eq!(container_from_label("mp4"), Some(Container::Mp4));
        assert_eq!(container_from_label("360p"), None);
    }
}
