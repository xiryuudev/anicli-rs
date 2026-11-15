//! Parser murni KotakAnime.
//!
//! Dua komponen kartu: `article.animeseries` (home) dan `a.as-anime-card`
//! (`/anime/`, search). Player dimuat lewat AJAX `player_ajax` (multipart) dan
//! halaman embed menyembunyikan URL HLS dengan base64 + XOR; `decode_embed`
//! merekonstruksinya.

use scraper::Html;
use url::Url;

use crate::error::{SourceError, SourceResult};
use crate::model::{AiringStatus, AnimeDetail, AnimeSearchResult, Episode, MediaKind, SourceId};
use crate::sources::util;
use crate::sources::util::{abs_url, attr, select, select_first, select_in, text};

pub const NAME: &str = "kotakanime";
const PAGE_LIST: &str = "list";
const PAGE_DETAIL: &str = "detail";
const PAGE_EPISODE: &str = "episode";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerServer {
    pub label: String,
    pub post: String,
    pub nume: String,
    pub server_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AjaxConfig {
    pub ajax_url: Url,
    pub nonce: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EpisodePage {
    pub title: String,
    pub episode_number: Option<u32>,
    pub servers: Vec<PlayerServer>,
    pub ajax: Option<AjaxConfig>,
}

/// `a.as-anime-card` — dipakai `/anime/` dan halaman search.
pub fn parse_archive(
    html_text: &str,
    base: &Url,
    source: &SourceId,
) -> SourceResult<Vec<AnimeSearchResult>> {
    let document = Html::parse_document(html_text);
    let mut results = Vec::new();

    for card in select(&document, "a.as-anime-card[href]") {
        let Some(href) = attr(&card, "href") else {
            continue;
        };
        let Some(url) = abs_url(base, &href) else {
            continue;
        };
        let title = select_in(card, "h3.as-anime-title")
            .first()
            .map(|element| text(element))
            .or_else(|| {
                select_in(card, "h3.as-anime-title span")
                    .first()
                    .map(|element| text(element))
            })
            .unwrap_or_default();
        if title.is_empty() {
            continue;
        }
        let thumbnail = select_in(card, "div.as-card-thumbnail img")
            .first()
            .and_then(|image| attr(image, "src"))
            .and_then(|src| abs_url(base, &src));
        let score = select_in(card, "span.as-rating span.value")
            .first()
            .map(|element| text(element))
            .and_then(|value| util::parse_score(&value));
        let kind = select_in(card, "span.as-type")
            .first()
            .map(|element| util::media_kind(&text(element)));
        results.push(AnimeSearchResult {
            source: source.clone(),
            title,
            url,
            thumbnail,
            kind,
            status: None,
            score,
        });
    }

    if results.is_empty() && !html_text.contains("as-anime-card") {
        return Err(SourceError::parse(
            NAME,
            PAGE_LIST,
            "a.as-anime-card",
            "tidak ada kartu anime di halaman ini",
        ));
    }
    Ok(results)
}

/// `article.animeseries` — kartu di homepage.
pub fn parse_home(
    html_text: &str,
    base: &Url,
    source: &SourceId,
) -> SourceResult<Vec<AnimeSearchResult>> {
    let document = Html::parse_document(html_text);
    let mut results = Vec::new();

    for card in select(&document, "article.animeseries") {
        let Some(anchor) = select_in(card, "div.sera a[href]").first().copied() else {
            continue;
        };
        let Some(href) = attr(&anchor, "href") else {
            continue;
        };
        let Some(url) = abs_url(base, &href) else {
            continue;
        };
        let title = select_in(card, "h3.title span[data-title-default]")
            .first()
            .map(|element| text(element))
            .filter(|value| !value.is_empty())
            .or_else(|| {
                select_in(card, "h3.title")
                    .first()
                    .map(|element| text(element))
            })
            .unwrap_or_default();
        if title.is_empty() {
            continue;
        }
        let thumbnail = select_in(card, "img")
            .first()
            .and_then(|image| attr(image, "src"))
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

    if results.is_empty() && !html_text.contains("animeseries") {
        return Err(SourceError::parse(
            NAME,
            PAGE_LIST,
            "article.animeseries",
            "tidak ada kartu di halaman home",
        ));
    }
    Ok(results)
}

/// Detail `/anime/{slug}/`.
pub fn parse_detail(html_text: &str, base: &Url, source: &SourceId) -> SourceResult<AnimeDetail> {
    let document = Html::parse_document(html_text);
    let title = select_first(&document, "h1.entry-title.cs")
        .or_else(|| select_first(&document, "h1.entry-title"))
        .map(|element| text(&element))
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            SourceError::parse(NAME, PAGE_DETAIL, "h1.entry-title", "judul tidak ditemukan")
        })?;

    let poster = select_first(
        &document,
        "div.as-card-thumbnail img, .anime-card__poster img",
    )
    .and_then(|image| attr(&image, "src"))
    .and_then(|src| abs_url(base, &src));

    let score = select_first(&document, "div.anime-card__score span.value")
        .map(|element| text(&element))
        .and_then(|value| util::parse_score(&value));

    let genres = select(&document, "div.anime-card__genres a.genre-tag")
        .iter()
        .map(|anchor| text(anchor))
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();

    let synopsis = select(&document, "#tab-synopsis div.synopsis-prose p")
        .iter()
        .map(|paragraph| text(paragraph))
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");

    let mut episodes: Vec<Episode> = select(&document, "a.episode-item[data-episode-url]")
        .iter()
        .filter_map(|anchor| {
            let href = attr(anchor, "data-episode-url")?;
            let url = abs_url(base, &href)?;
            let label = select_in(*anchor, "span.ep-title")
                .first()
                .map(|element| text(element))
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| text(anchor));
            let number = util::parse_episode_number(&label)
                .or_else(|| util::parse_episode_number(url.as_str()));
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

    let status = select_first(
        &document,
        "div.anime-card__quick-info span.info-item.status-airing",
    )
    .map(|element| util::airing_status(&text(&element)))
    .unwrap_or(AiringStatus::Unknown);

    Ok(AnimeDetail {
        source: source.clone(),
        url: base.clone(),
        title,
        alt_titles: Vec::new(),
        synopsis,
        genres,
        kind: None,
        status,
        year: None,
        score,
        total_episodes: u32::try_from(episodes.len()).ok(),
        poster,
        episodes,
    })
}

/// Halaman episode: daftar server + konfigurasi AJAX (nonce).
pub fn parse_episode(html_text: &str, base: &Url, _source: &SourceId) -> SourceResult<EpisodePage> {
    let document = Html::parse_document(html_text);
    let title = select_first(&document, "h1.entry-title")
        .map(|element| text(&element))
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "Episode".to_string());
    let episode_number = util::parse_episode_number(&title);

    let servers: Vec<PlayerServer> = select(
        &document,
        "ul.tabs1.player li.kotak_player_option[data-post][data-nume]",
    )
    .iter()
    .map(|item| {
        let label = select_in(*item, "span")
            .first()
            .map(|element| text(element))
            .unwrap_or_default();
        PlayerServer {
            label,
            post: attr(item, "data-post").unwrap_or_default(),
            nume: attr(item, "data-nume").unwrap_or_default(),
            server_name: attr(item, "data-type").unwrap_or_default(),
        }
    })
    .filter(|server| !server.post.is_empty() && !server.nume.is_empty())
    .collect();

    if servers.is_empty() {
        return Err(SourceError::parse(
            NAME,
            PAGE_EPISODE,
            "li.kotak_player_option",
            "tidak ada server player di halaman episode",
        ));
    }

    Ok(EpisodePage {
        title,
        episode_number,
        servers,
        ajax: parse_ajax_config(html_text, base),
    })
}

/// Konfigurasi `var kotakajax = {...}` yang ditanam sebagai
/// `data:text/javascript;base64,...`.
fn matching_brace(text: &str, open: usize) -> Option<usize> {
    let mut depth = 0_i32;
    let mut in_string = false;
    let mut quote = ' ';
    let mut escaped = false;
    for (offset, ch) in text[open..].char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == quote {
                in_string = false;
            }
            continue;
        }
        match ch {
            '"' | '\'' => {
                in_string = true;
                quote = ch;
            }
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(open + offset);
                }
            }
            _ => {}
        }
    }
    None
}

pub fn parse_ajax_config(html_text: &str, base: &Url) -> Option<AjaxConfig> {
    // Halaman memuat beberapa script `data:text/javascript;base64,...`; hanya
    // salah satunya berisi `var kotakajax = {...}`.
    let marker = "data:text/javascript;base64,";
    for (index, _) in html_text.match_indices(marker) {
        let start = index + marker.len();
        let encoded: String = html_text[start..]
            .chars()
            .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '+' || *ch == '/' || *ch == '=')
            .collect();
        let Some(decoded) = util::b64decode_text(&encoded) else {
            continue;
        };
        let Some(json_start) = decoded.find("var kotakajax") else {
            continue;
        };
        let Some(brace) = decoded[json_start..].find('{') else {
            continue;
        };
        let brace = json_start + brace;
        let Some(brace_end) = matching_brace(&decoded, brace) else {
            continue;
        };
        let json = &decoded[brace..=brace_end];

        #[derive(serde::Deserialize)]
        struct Raw {
            #[serde(default)]
            url: String,
            #[serde(default)]
            nonce: String,
        }
        if let Ok(raw) = serde_json::from_str::<Raw>(json) {
            if !raw.url.trim().is_empty() && !raw.nonce.trim().is_empty() {
                return Some(AjaxConfig {
                    ajax_url: Url::parse(raw.url.trim()).unwrap_or_else(|_| base.clone()),
                    nonce: raw.nonce,
                });
            }
        }
    }
    None
}

/// Decode halaman embed KotakAnime (base64 + XOR) menjadi URL HLS.
///
/// Format: `_k26b594()` menghasilkan key 16 byte dari daftar angka
/// (`(n - 65*(i+3)) & 255`), blob base64 di-`atob`, lalu di-XOR dengan key
/// (siklik), hasilnya JS yang berisi `file:"https://…/index.m3u8"`.
pub fn decode_embed(html_text: &str) -> Option<String> {
    let key = decode_key(html_text)?;
    let raw = decode_atob_blob(html_text)?;
    if raw.is_empty() {
        return None;
    }
    let bytes: Vec<u8> = raw
        .iter()
        .enumerate()
        .map(|(index, byte)| byte ^ key[index % key.len()])
        .collect();
    let js = String::from_utf8_lossy(&bytes);
    extract_m3u8(&js).or_else(|| extract_m3u8(html_text))
}

fn decode_key(html_text: &str) -> Option<Vec<u8>> {
    let start = html_text.find("var s='")? + "var s='".len();
    let end = html_text[start..].find('\'')? + start;
    let numbers = &html_text[start..end];

    // Rumus key bervariasi per render: `-65*(j+3)`, `-64*(j+3)`, dst.
    let context_start = start.saturating_sub(200);
    let context_end = (end + 420).min(html_text.len());
    let context = &html_text[context_start..context_end];
    let (multiplier, offset) = parse_key_formula(context).unwrap_or((65, 3));

    let key: Vec<u8> = numbers
        .split(',')
        .enumerate()
        .map(|(index, raw)| {
            let value: u16 = raw.trim().parse().ok()?;
            let shift = multiplier.wrapping_mul(index as u16 + offset);
            Some(((value.wrapping_sub(shift)) & 0xFF) as u8)
        })
        .collect::<Option<Vec<u8>>>()?;
    (!key.is_empty()).then_some(key)
}

/// Cari pola `-<N>*(j+<M>)` di sekitar array key (tanpa regex crate).
fn parse_key_formula(context: &str) -> Option<(u16, u16)> {
    let bytes = context.as_bytes();
    for (index, byte) in bytes.iter().enumerate() {
        if *byte != b'-' {
            continue;
        }
        let mut cursor = index + 1;
        let digits_start = cursor;
        while cursor < bytes.len() && bytes[cursor].is_ascii_digit() {
            cursor += 1;
        }
        if cursor == digits_start {
            continue;
        }
        let Some(multiplier) = context[digits_start..cursor].parse::<u16>().ok() else {
            continue;
        };
        while cursor < bytes.len() && bytes[cursor] == b' ' {
            cursor += 1;
        }
        if cursor >= bytes.len() || bytes[cursor] != b'*' {
            continue;
        }
        cursor += 1;
        while cursor < bytes.len() && bytes[cursor] == b' ' {
            cursor += 1;
        }
        if !context[cursor..].starts_with("(j+") {
            continue;
        }
        cursor += 3;
        let offset_start = cursor;
        while cursor < bytes.len() && bytes[cursor].is_ascii_digit() {
            cursor += 1;
        }
        if cursor == offset_start {
            continue;
        }
        let Some(offset) = context[offset_start..cursor].parse::<u16>().ok() else {
            continue;
        };
        return Some((multiplier, offset));
    }
    None
}

fn decode_atob_blob(html_text: &str) -> Option<Vec<u8>> {
    use base64::Engine as _;
    // Cari pemanggilan `atob(...)` (bisa dipecah `'at'+'ob'`).
    let marker = html_text
        .find("at'+'ob")
        .or_else(|| html_text.find("atob"))?;
    let open = html_text[marker..].find('(')? + marker;
    let mut depth = 0_i32;
    let mut in_string = false;
    let mut quote = ' ';
    let mut escaped = false;
    let mut literals = String::new();
    let mut current = String::new();

    for ch in html_text[open..].chars() {
        if in_string {
            if escaped {
                current.push(ch);
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == quote {
                in_string = false;
                literals.push_str(&current);
                current.clear();
            } else {
                current.push(ch);
            }
            continue;
        }
        match ch {
            '\'' | '"' => {
                in_string = true;
                quote = ch;
            }
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            _ => {}
        }
    }

    if literals.is_empty() {
        return None;
    }
    base64::engine::general_purpose::STANDARD
        .decode(literals.trim())
        .ok()
}

fn extract_m3u8(text: &str) -> Option<String> {
    let mut search_from = 0;
    while let Some(relative) = text[search_from..].find("https://") {
        let start = search_from + relative;
        let rest = &text[start..];
        let end = rest
            .find(['"', '\'', ' ', '\n', '<', '>'])
            .unwrap_or(rest.len());
        let candidate = rest[..end].trim();
        if candidate.contains(".m3u8") {
            return Some(candidate.to_string());
        }
        search_from = start + 8;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(path: &str) -> String {
        let full = format!("{}/tests/fixtures/{path}", env!("CARGO_MANIFEST_DIR"));
        std::fs::read_to_string(&full).unwrap_or_else(|err| panic!("baca {full}: {err}"))
    }

    fn base() -> Url {
        Url::parse("https://s13.nontonanimeid.boats/").unwrap()
    }

    fn id() -> SourceId {
        SourceId::new(NAME)
    }

    #[test]
    fn archive_list_is_parsed() {
        let items = parse_archive(&fixture("kotakanime/anime_list.html"), &base(), &id()).unwrap();
        assert!(!items.is_empty());
        assert!(items[0].url.as_str().contains("/anime/"));
        assert!(!items[0].title.is_empty());
    }

    #[test]
    fn detail_parses_episodes() {
        let detail =
            parse_detail(&fixture("kotakanime/detail_kashita.html"), &base(), &id()).unwrap();
        assert!(detail.title.to_lowercase().contains("kashita"));
        assert!(!detail.episodes.is_empty());
        assert!(detail
            .episodes
            .iter()
            .any(|episode| episode.number == Some(1)));
        assert!(!detail.genres.is_empty());
    }

    #[test]
    fn episode_page_extracts_servers_and_nonce() {
        let page = parse_episode(&fixture("kotakanime/episode.html"), &base(), &id()).unwrap();
        assert!(!page.servers.is_empty());
        assert!(page.servers.iter().all(|server| !server.post.is_empty()));
        let ajax = page.ajax.expect("konfigurasi kotakajax harus ada");
        assert!(ajax.ajax_url.as_str().contains("admin-ajax.php"));
        assert!(!ajax.nonce.is_empty());
    }

    #[test]
    fn embed_decode_yields_hls_url() {
        let hls = decode_embed(&fixture("kotakanime/embed.html")).expect("harus ter-decode");
        assert!(hls.contains("index.m3u8"), "{hls}");
        assert!(hls.contains("cdn2.kotakanimeid.link"), "{hls}");
    }

    #[test]
    fn embed_decode_handles_different_key_multiplier() {
        // Halaman live memakai rumus `-64*(j+3)`, fixture lama `-65*(j+3)`.
        let hls = decode_embed(&fixture("kotakanime/embed_live.html")).expect("harus ter-decode");
        assert!(hls.contains("index.m3u8"), "{hls}");
    }

    #[test]
    fn ajax_fragment_contains_iframe() {
        let html = fixture("kotakanime/ajax_player.html");
        let src = crate::embed::iframe_src(&html).expect("iframe");
        assert!(src.contains("video-embed"), "{src}");
    }
}
