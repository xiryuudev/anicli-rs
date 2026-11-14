//! Parser murni Samehadaku (`v2.samehadaku.how`).
//!
//! Semua fungsi menerima HTML/JSON sebagai `&str` dan tidak melakukan I/O, sehingga
//! bisa dites offline dengan fixture di `tests/fixtures/samehadaku/`.
//!
//! Jebakan yang sudah ditangani (lihat DESIGN §22.2):
//! - dua komponen kartu: `article.animepost` (arsip/search) vs `.post-show li` (widget);
//! - `.type` bermakna ganda → tipe media diambil dari `.stooltip .type-badge`;
//! - `div.custom-scrollbar` cocok ke sidebar → pakai `.space-y-2`;
//! - `data-embed` bisa berupa iframe ter-escape atau shortcode yang belum ter-resolve;
//! - resolusi hanya ada di label teks tombol server.

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

/// Nama source untuk pesan error.
pub const NAME: &str = "samehadaku";

const PAGE_LIST: &str = "list";
const PAGE_DETAIL: &str = "detail";
const PAGE_EPISODE: &str = "episode";

/// Bootstrap JS yang ada di setiap halaman: endpoint AJAX + nonce.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SamehadaData {
    pub ajax_url: Url,
    pub rest_url: Url,
    pub nonce: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerButton {
    pub label: String,
    pub post: Option<String>,
    pub nume: Option<String>,
    pub data_type: Option<String>,
    /// Isi `data-embed` setelah di-unescape (bisa iframe HTML atau shortcode).
    pub embed: Option<String>,
    pub data_url: Option<String>,
    pub resolution: Option<Resolution>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EpisodePage {
    pub title: String,
    pub episode_number: Option<u32>,
    pub default_embed: Option<Url>,
    pub servers: Vec<ServerButton>,
    pub samehada: Option<SamehadaData>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostLink {
    pub host: String,
    pub url: Url,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadRow {
    pub format: String,
    pub label: String,
    pub resolution: Option<Resolution>,
    pub container: Option<Container>,
    pub links: Vec<HostLink>,
}

#[derive(Debug, Deserialize)]
struct RestAnime {
    #[serde(default)]
    link: String,
    #[serde(default)]
    title: RestRendered,
}

#[derive(Debug, Default, Deserialize)]
struct RestRendered {
    #[serde(default)]
    rendered: String,
}

/// Hasil pencarian dari WP REST `/wp-json/wp/v2/anime?search=…` (daftar **seri**).
pub fn parse_search_rest(
    json_text: &str,
    source: &SourceId,
) -> SourceResult<Vec<AnimeSearchResult>> {
    let items: Vec<RestAnime> = serde_json::from_str(json_text).map_err(|err| {
        SourceError::parse(
            NAME,
            PAGE_LIST,
            "search_rest",
            format!("JSON tidak sesuai bentuk yang diharapkan: {err}"),
        )
    })?;

    let mut results = Vec::new();
    for item in items {
        let Ok(url) = Url::parse(item.link.trim()) else {
            continue;
        };
        let title = util::decode_entities(item.title.rendered.trim());
        if title.is_empty() {
            continue;
        }
        results.push(AnimeSearchResult {
            source: source.clone(),
            title,
            url,
            thumbnail: None,
            kind: None,
            status: None,
            score: None,
        });
    }
    Ok(results)
}

/// Grid arsip: search HTML, `/daftar-anime-2/`, `/daftar-batch/`.
pub fn parse_grid(
    html_text: &str,
    base: &Url,
    source: &SourceId,
    status: Option<AiringStatus>,
) -> SourceResult<Vec<AnimeSearchResult>> {
    let document = Html::parse_document(html_text);
    let cards = select(&document, "article.animepost");
    let mut results = Vec::new();

    for card in &cards {
        let href = select_in(*card, ".animposx > a")
            .first()
            .and_then(|anchor| attr(anchor, "href"))
            .or_else(|| {
                select_in(*card, ".animposx a")
                    .first()
                    .and_then(|anchor| attr(anchor, "href"))
            });
        let Some(href) = href else { continue };
        let Some(url) = abs_url(base, &href) else {
            continue;
        };
        let title = select_in(*card, ".data .title h2")
            .first()
            .map(|element| text(element))
            .filter(|value| !value.is_empty());
        let Some(title) = title else { continue };

        let thumbnail = select_in(*card, "img.anmsa")
            .first()
            .and_then(|image| attr(image, "src"))
            .and_then(|src| abs_url(base, &src));
        let kind = select_in(*card, ".stooltip .type-badge")
            .first()
            .map(|element| util::media_kind(&text(element)));
        let score = select_in(*card, ".content-thumb .score")
            .first()
            .and_then(|element| util::parse_score(&text(element)));

        results.push(AnimeSearchResult {
            source: source.clone(),
            title,
            url,
            thumbnail,
            kind,
            status,
            score,
        });
    }

    // Halaman kosong sah; markup yang hilang total berarti struktur berubah.
    if results.is_empty() && !html_text.contains("animepost") {
        return Err(SourceError::parse(
            NAME,
            PAGE_LIST,
            "article.animepost",
            "tidak ada kartu arsip di halaman ini",
        ));
    }
    Ok(results)
}

/// Widget "latest episode" (`/anime-terbaru/`) — link judul = seri, link thumb = episode.
pub fn parse_ongoing(
    html_text: &str,
    base: &Url,
    source: &SourceId,
) -> SourceResult<Vec<AnimeSearchResult>> {
    let document = Html::parse_document(html_text);
    let items = select(&document, ".post-show > ul > li");
    let mut results = Vec::new();

    for item in &items {
        let Some(anchor) = select_in(*item, ".entry-title a").first().copied() else {
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
        let thumbnail = select_in(*item, ".thumb img")
            .first()
            .and_then(|image| attr(image, "src"))
            .and_then(|src| abs_url(base, &src));
        let kind = select_in(*item, ".stooltip .type-badge")
            .first()
            .map(|element| util::media_kind(&text(element)));

        results.push(AnimeSearchResult {
            source: source.clone(),
            title,
            url,
            thumbnail,
            kind,
            status: Some(AiringStatus::Ongoing),
            score: None,
        });
    }

    if results.is_empty() && !html_text.contains("post-show") {
        return Err(SourceError::parse(
            NAME,
            PAGE_LIST,
            ".post-show li",
            "tidak ada item ongoing di halaman ini",
        ));
    }
    Ok(results)
}

/// Halaman detail anime.
pub fn parse_detail(html_text: &str, base: &Url, source: &SourceId) -> SourceResult<AnimeDetail> {
    let document = Html::parse_document(html_text);

    let title = select_first(&document, "h1")
        .map(|element| text(&element))
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            SourceError::parse(
                NAME,
                PAGE_DETAIL,
                "h1",
                "judul anime tidak ditemukan (struktur halaman berubah?)",
            )
        })?;

    let url = Url::parse(base.as_str()).ok();
    let poster = select(&document, "div.w-48 img")
        .first()
        .and_then(|image| attr(image, "src"))
        .and_then(|src| abs_url(base, &src));

    let synopsis = select(&document, "div.entry-content-single p")
        .iter()
        .map(|paragraph| text(paragraph))
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");

    let mut meta = std::collections::BTreeMap::new();
    for label_span in select(&document, "span.w-28") {
        let label = text(&label_span);
        let Some(parent) = label_span.parent().and_then(scraper::ElementRef::wrap) else {
            continue;
        };
        let spans = select_in(parent, "span");
        let value = spans
            .get(1)
            .map(|element| text(element))
            .unwrap_or_default();
        if !label.is_empty() {
            meta.insert(label, value);
        }
    }

    let status = meta
        .get("Status")
        .map(|value| util::airing_status(value))
        .unwrap_or(AiringStatus::Unknown);
    let year = meta
        .get("Released")
        .and_then(|value| util::parse_year(value));
    let score = select_first(&document, "span.font-extrabold")
        .map(|element| text(&element))
        .and_then(|value| util::parse_score(&value));
    let total_episodes = meta
        .get("Total Episode")
        .and_then(|value| util::parse_episode_number(value));
    let kind = meta.get("Type").map(|value| util::media_kind(value));

    let mut genres = select(&document, r#"div.pt-3.border-t a[href*="/genre/"]"#)
        .iter()
        .map(|anchor| text(anchor))
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    genres.dedup();

    let alt_titles = ["Japanese", "English", "Synonyms"]
        .iter()
        .filter_map(|key| meta.get(*key))
        .filter(|value| !value.is_empty())
        .cloned()
        .collect::<Vec<_>>();

    let episodes = parse_episode_list(&document, base);

    Ok(AnimeDetail {
        source: source.clone(),
        url: url.unwrap_or_else(|| base.clone()),
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

fn parse_episode_list(document: &Html, base: &Url) -> Vec<Episode> {
    let Some(container) = select_first(document, "div.custom-scrollbar.space-y-2") else {
        return Vec::new();
    };

    let rows: Vec<scraper::ElementRef<'_>> = container
        .children()
        .filter_map(scraper::ElementRef::wrap)
        .filter(|element| element.value().name() == "div")
        .collect();
    let mut episodes: Vec<Episode> = rows
        .iter()
        .filter_map(|row| {
            let anchor = select_in(*row, "h4 a").first().copied()?;
            let href = attr(&anchor, "href")?;
            let url = abs_url(base, &href)?;
            let label = text(&anchor);
            let number = select_in(*row, ".shrink-0.w-8 a")
                .first()
                .map(|element| text(element))
                .and_then(|value| util::parse_episode_number(&value))
                .or_else(|| util::parse_episode_number(&label));
            let released = select_in(*row, "span")
                .last()
                .map(|element| text(element))
                .filter(|value| !value.is_empty());
            Some(Episode {
                number,
                label,
                title: None,
                url,
                released: None,
            })
            .map(|episode| Episode {
                released: released.as_deref().and_then(parse_indonesian_date),
                ..episode
            })
        })
        .collect();

    // Situs menampilkan episode terbaru lebih dulu.
    episodes.reverse();
    episodes
}

/// Tanggal Indonesia sederhana (`11 November 2026`, `4 Nov 2026`).
fn parse_indonesian_date(text: &str) -> Option<chrono::NaiveDate> {
    const MONTHS: [&str; 12] = [
        "januari",
        "februari",
        "maret",
        "april",
        "mei",
        "juni",
        "juli",
        "agustus",
        "september",
        "oktober",
        "november",
        "desember",
    ];
    let cleaned = text.trim().to_ascii_lowercase();
    let tokens: Vec<&str> = cleaned.split_whitespace().collect();
    if tokens.len() < 3 {
        return None;
    }
    let day: u32 = tokens[0].parse().ok()?;
    let token = tokens[1].trim_end_matches(',');
    let month = MONTHS.iter().position(|name| *name == token).or_else(|| {
        (token.len() >= 3)
            .then(|| MONTHS.iter().position(|name| name.starts_with(token)))
            .flatten()
    })? as u32
        + 1;
    let year: i32 = tokens[2].trim_end_matches(',').parse().ok()?;
    chrono::NaiveDate::from_ymd_opt(year, month, day)
}

/// Halaman episode: judul, nomor, embed default, tombol server, bootstrap AJAX.
pub fn parse_episode(html_text: &str, base: &Url, _source: &SourceId) -> SourceResult<EpisodePage> {
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

    let episode_number = select_first(&document, r#"[itemprop="episodeNumber"]"#)
        .map(|element| text(&element))
        .and_then(|value| util::parse_episode_number(&value))
        .or_else(|| util::parse_episode_number(&title));

    let default_embed = select_first(&document, "#player-frame-wrapper iframe")
        .and_then(|iframe| attr(&iframe, "src"))
        .and_then(|src| abs_url(base, &src));

    let servers = select(&document, ".player-server-btn")
        .iter()
        .map(|button| {
            let label = select_in(*button, "span")
                .first()
                .map(|element| text(element))
                .unwrap_or_else(|| text(button));
            ServerButton {
                resolution: util::parse_resolution(&label),
                label,
                post: attr(button, "data-post"),
                nume: attr(button, "data-nume"),
                data_type: attr(button, "data-type"),
                embed: attr(button, "data-embed").map(|value| crate::embed::unescape_html(&value)),
                data_url: attr(button, "data-url"),
            }
        })
        .collect::<Vec<_>>();

    Ok(EpisodePage {
        title,
        episode_number,
        default_embed,
        servers,
        samehada: parse_samehada_data(html_text),
    })
}

/// Ambil objek `var SamehadaData = { … }` dari HTML.
pub fn parse_samehada_data(html_text: &str) -> Option<SamehadaData> {
    #[derive(Deserialize)]
    struct Raw {
        #[serde(rename = "ajaxUrl", default)]
        ajax_url: String,
        #[serde(rename = "restUrl", default)]
        rest_url: String,
        #[serde(default)]
        nonce: String,
    }

    let start = html_text.find("var SamehadaData")?;
    let rest = &html_text[start..];
    let equals = rest.find('=')?;
    let json_start = rest[equals..].find('{')? + equals;
    let json_end = rest[json_start..].find("};")? + json_start + 1;
    let raw: Raw = serde_json::from_str(rest[json_start..json_end].trim()).ok()?;

    Some(SamehadaData {
        ajax_url: Url::parse(raw.ajax_url.trim()).ok()?,
        rest_url: Url::parse(raw.rest_url.trim()).ok()?,
        nonce: raw.nonce,
    })
}

/// Blok unduhan (`.download-eps`): format MKV/MP4/x265 × resolusi × host.
pub fn parse_downloads(html_text: &str, base: &Url) -> Vec<DownloadRow> {
    let document = Html::parse_document(html_text);
    let mut rows = Vec::new();

    for block in select(&document, "div.download-eps") {
        let format = select_in(block, "p b")
            .first()
            .map(|element| text(element))
            .unwrap_or_default();
        for item in select_in(block, "ul > li") {
            let label = select_in(item, "strong")
                .first()
                .map(|element| text(element))
                .unwrap_or_default();
            let resolution = util::parse_resolution(&label);
            let container = match format.to_ascii_lowercase().as_str() {
                "mkv" => Some(Container::Mkv),
                "mp4" => Some(Container::Mp4),
                _ => None,
            };
            let links = select_in(item, ".download-eps-links a")
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
                    format: format.clone(),
                    label,
                    resolution,
                    container,
                    links,
                });
            }
        }
    }
    rows
}

/// Ambil tipe media dari teks bebas (dipakai bila badge tidak ada).
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
        Url::parse("https://v2.samehadaku.how/").unwrap()
    }

    fn id() -> SourceId {
        SourceId::new(NAME)
    }

    #[test]
    fn search_rest_json_is_parsed() {
        let json = r#"[
          {"id":1,"slug":"naruto-kecil","link":"https://v2.samehadaku.how/anime/naruto-kecil/","title":{"rendered":"Naruto Kecil"}},
          {"id":2,"slug":"boruto","link":"https://v2.samehadaku.how/anime/boruto-naruto-next-generations/","title":{"rendered":"Boruto: Naruto Next Generations"}}
        ]"#;
        let results = parse_search_rest(json, &id()).unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].title, "Naruto Kecil");
        assert!(results[1].title.contains("Boruto"));
        assert_eq!(results[0].source.as_str(), NAME);
    }

    #[test]
    fn search_rest_rejects_wrong_shape() {
        let err = parse_search_rest("{\"data\":[]}", &id()).unwrap_err();
        assert!(matches!(err, SourceError::Parse { .. }));
    }

    #[test]
    fn grid_archive_is_parsed() {
        let results = parse_grid(
            &fixture("samehadaku/completed.html"),
            &base(),
            &id(),
            Some(AiringStatus::Completed),
        )
        .unwrap();
        assert_eq!(results.len(), 30);
        assert!(!results[0].title.is_empty());
        assert!(results[0]
            .url
            .as_str()
            .starts_with("https://v2.samehadaku.how/anime/"));
        assert!(results
            .iter()
            .all(|item| item.status == Some(AiringStatus::Completed)));
        assert!(results.iter().any(|item| item.kind == Some(MediaKind::Tv)));
    }

    #[test]
    fn search_grid_is_parsed() {
        let results = parse_grid(&fixture("samehadaku/search.html"), &base(), &id(), None).unwrap();
        assert_eq!(results.len(), 20);
        assert!(results[0].title.contains("Naruto"));
    }

    #[test]
    fn ongoing_widget_is_parsed() {
        let results = parse_ongoing(&fixture("samehadaku/ongoing.html"), &base(), &id()).unwrap();
        assert_eq!(results.len(), 14);
        assert_eq!(results[0].title, "Shuiro no Kamen");
        assert!(results[0].url.as_str().contains("/anime/"));
        assert!(results
            .iter()
            .all(|item| item.status == Some(AiringStatus::Ongoing)));
    }

    #[test]
    fn detail_page_is_parsed() {
        let detail = parse_detail(&fixture("samehadaku/detail.html"), &base(), &id()).unwrap();
        assert!(detail
            .title
            .contains("Chitose-kun wa Ramune Bin no Naka Cour 2"));
        assert_eq!(detail.status, AiringStatus::Ongoing);
        assert_eq!(detail.score, Some(7.62));
        assert_eq!(detail.total_episodes, Some(11));
        assert_eq!(detail.year, Some(2026));
        assert!(detail.genres.contains(&"Comedy".to_string()));
        assert!(detail.alt_titles.iter().any(|title| title.contains("千歳")));
        assert!(!detail.synopsis.is_empty());
        assert_eq!(detail.episodes.len(), 5);
        // descending di situs, ascending setelah parser
        assert_eq!(detail.episodes[0].number, Some(1));
        assert_eq!(detail.episodes[4].number, Some(5));
        assert!(detail.episodes[4].url.as_str().contains("episode-5"));
    }

    #[test]
    fn episode_page_servers_and_embed_are_parsed() {
        let page = parse_episode(&fixture("samehadaku/episode.html"), &base(), &id()).unwrap();
        assert!(page.title.contains("Episode 5"));
        assert_eq!(page.episode_number, Some(5));
        assert_eq!(page.servers.len(), 5);
        assert_eq!(page.servers[0].label, "Blogspot");
        assert_eq!(page.servers[0].resolution, None);
        assert_eq!(page.servers[2].label, "Mega 480p");
        assert_eq!(page.servers[2].resolution, Some(Resolution::P480));
        assert!(page.servers[0]
            .embed
            .as_deref()
            .is_some_and(|embed| embed.contains("<iframe")));
        assert!(page.default_embed.is_some());

        let data = page.samehada.expect("SamehadaData harus ada");
        assert!(data.ajax_url.as_str().ends_with("admin-ajax.php"));
        assert!(!data.nonce.is_empty());
    }

    #[test]
    fn download_rows_are_parsed_with_labels() {
        let rows = parse_downloads(&fixture("samehadaku/episode.html"), &base());
        assert!(
            rows.len() >= 3,
            "harus ada beberapa baris unduhan: {rows:?}"
        );
        assert!(rows.iter().any(|row| row.format == "MKV"));
        let first = &rows[0];
        assert!(first.resolution.is_some() || first.label.is_empty());
        assert!(first.links.iter().any(|link| link.host.contains("Gofile")));
        assert!(first
            .links
            .iter()
            .any(|link| link.url.host_str() == Some("pixeldrain.com")));
    }

    #[test]
    fn indonesian_dates_are_parsed() {
        assert_eq!(
            parse_indonesian_date("11 November 2026"),
            chrono::NaiveDate::from_ymd_opt(2026, 11, 11)
        );
        assert_eq!(
            parse_indonesian_date("4 Nov 2026"),
            chrono::NaiveDate::from_ymd_opt(2026, 11, 4)
        );
        assert!(parse_indonesian_date("kemarin").is_none());
    }
}
