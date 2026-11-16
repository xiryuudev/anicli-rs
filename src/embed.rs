//! Resolver embed/host yang dipakai lintas source (DESIGN §12.6–§12.7).
//!
//! Semua fungsi di sini **murni** (input HTML/URL → output URL/struct) supaya bisa
//! dites offline dengan fixture. Pemanggilan HTTP ada di adapter source.

use serde::Deserialize;
use url::Url;

use crate::model::{Container, ServerKind};

/// Satu entri `SOURCES` milik player IceFile (Samehadaku).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IcefileSource {
    pub kind: String,
    pub label: String,
    pub link: Url,
    pub resolution: Option<u16>,
}

#[derive(Debug, Deserialize)]
struct RawIcefileSource {
    #[serde(rename = "type", default)]
    kind: String,
    #[serde(default)]
    label: String,
    #[serde(default)]
    link: String,
    #[serde(rename = "resNum", default)]
    res_num: Option<u16>,
    #[serde(default)]
    res: Option<String>,
}

/// Ambil literal array JSON pertama setelah `marker`, dengan pencocokan bracket yang
/// sadar-string (array di halaman embed diikuti `,` atau `;`, bukan selalu `];`).
fn extract_js_array<'a>(html: &'a str, marker: &str) -> Option<&'a str> {
    let at = html.find(marker)?;
    let rest = &html[at..];
    let start = rest.find('[')?;
    let mut depth = 0_u32;
    let mut in_string = false;
    let mut escaped = false;

    for (offset, ch) in rest.char_indices().skip(start) {
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        match ch {
            '"' => in_string = true,
            '[' => depth += 1,
            ']' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(&rest[start..=offset]);
                }
            }
            _ => {}
        }
    }
    None
}

/// Ambil `src` dari iframe pertama di sebuah fragmen HTML.
pub fn iframe_src(html: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let iframe = lower.find("<iframe")?;
    let rest = &html[iframe..];
    let src_at = rest.to_ascii_lowercase().find("src=")?;
    let after = &rest[src_at + 4..];
    let after = after.trim_start();
    let quote = after.chars().next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    let value = &after[1..];
    let end = value.find(quote)?;
    let url = &value[..end];
    (!url.trim().is_empty()).then(|| unescape_html(url.trim()))
}

/// IceFile menyimpan daftar varian di `const SOURCES=[{...}]`.
pub fn icefile_sources(html: &str) -> Vec<IcefileSource> {
    let Some(json) = extract_js_array(html, "const SOURCES") else {
        return Vec::new();
    };
    let Ok(raw) = serde_json::from_str::<Vec<RawIcefileSource>>(json) else {
        return Vec::new();
    };

    raw.into_iter()
        .filter_map(|entry| {
            let link = Url::parse(entry.link.trim()).ok()?;
            let resolution = entry.res_num.or_else(|| {
                entry
                    .res
                    .as_deref()
                    .and_then(|res| res.trim_end_matches('p').parse().ok())
            });
            Some(IcefileSource {
                kind: entry.kind,
                label: entry.label,
                link,
                resolution,
            })
        })
        .collect()
}

/// Halaman embed `desustream` memberi URL media di `const videoURL = "..."` atau
/// `<video><source src="...">`. Keduanya didukung.
pub fn desustream_media(html: &str) -> Option<Url> {
    for marker in ["const videoURL", "videoURL"] {
        if let Some(at) = html.find(marker) {
            let rest = &html[at..];
            if let Some(quote_start) = rest.find('"') {
                let after = &rest[quote_start + 1..];
                if let Some(end) = after.find('"') {
                    let candidate = unescape_html(after[..end].trim());
                    if let Ok(url) = Url::parse(&candidate) {
                        return Some(url);
                    }
                }
            }
        }
    }

    // <source src="...">
    let lower = html.to_ascii_lowercase();
    if let Some(at) = lower.find("<source") {
        let rest = &html[at..];
        if let Some(src_at) = rest.to_ascii_lowercase().find("src=") {
            let after = rest[src_at + 4..].trim_start();
            let quote = after.chars().next()?;
            if quote == '"' || quote == '\'' {
                let value = &after[1..];
                if let Some(end) = value.find(quote) {
                    let candidate = unescape_html(value[..end].trim());
                    if let Ok(url) = Url::parse(&candidate) {
                        return Some(url);
                    }
                }
            }
        }
    }
    None
}

/// Halaman `blogger.com/video.g` menyembunyikan URL `googlevideo` di JSON
/// ter-escape (`\u003d`, `\u0026`, `\/`).
pub fn blogspot_media(html: &str) -> Option<Url> {
    // Halaman blogger memakai JSON ter-escape di dalam atribut HTML (`&quot;`),
    // jadi unescape HTML dulu, baru escape JS.
    let decoded = unescape_js(&unescape_html(html));
    let mut search_from = 0;

    // Cari kata kunci `videoplayback`, lalu ambil URL `https://…` yang memuatnya.
    // Jangan mulai dari "https://" pertama: halaman juga memuat URL pemutar
    // blogger.com yang teksnya sendiri mengandung JSON video.
    while let Some(relative) = decoded[search_from..].find("videoplayback") {
        let position = search_from + relative;
        let Some(start) = decoded[..position].rfind("https://") else {
            search_from = position + 1;
            continue;
        };
        let rest = &decoded[start..];
        let end = rest
            .find(['"', '\'', '\\', ' ', '\n', ')', '}', ']'])
            .unwrap_or(rest.len());
        let candidate = rest[..end].trim();
        if let Ok(url) = Url::parse(candidate) {
            let host = url.host_str().unwrap_or_default();
            if host.ends_with("googlevideo.com") {
                return Some(url);
            }
        }
        search_from = position + 1;
    }
    None
}

/// Resolusi dari URL googlevideo (itag 18 = 360p) — best effort.
pub fn googlevideo_resolution(url: &Url) -> Option<u16> {
    if url
        .query_pairs()
        .any(|(key, value)| key == "itag" && value == "18")
    {
        return Some(360);
    }
    None
}

/// `pixeldrain.com/u/<id>` → `pixeldrain.com/api/file/<id>` (bisa diputar mpv,
/// kecuali file yang membatasi hotlink → 403).
pub fn pixeldrain_direct(url: &Url) -> Option<Url> {
    let host = url.host_str()?;
    if host != "pixeldrain.com" {
        return None;
    }
    let mut segments = url.path_segments()?;
    if segments.next() != Some("u") {
        return None;
    }
    let id = segments.next()?;
    if id.is_empty() {
        return None;
    }
    Url::parse(&format!("https://pixeldrain.com/api/file/{id}")).ok()
}

/// Referer yang diwajibkan Blogger untuk `googlevideo` (`source=blogger`).
///
/// Tanpa header ini Google membalas 403; di desktop mpv/vlc bisa saja lolos
/// karena kebetulan, tetapi di Android intent tidak membawa header sama sekali.
pub const BLOGSPOT_REFERER: &str = "https://www.blogger.com/";

/// `Referer` Blogger sebagai `Url` (None kalau entah kenapa tidak valid).
pub fn blogspot_referer() -> Option<Url> {
    Url::parse(BLOGSPOT_REFERER).ok()
}

/// URL media presigned (mis. R2/S3 `?X-Amz-Expires=...`) di halaman embed Filedon.
///
/// Halaman filedon menyimpan URL dalam HTML ter-escape (`&quot;`, `\/`), jadi
/// unescape dulu lalu ambil URL pertama yang berakhiran `.mp4` (dengan query).
pub fn presigned_mp4(html: &str) -> Option<Url> {
    let decoded = unescape_js(&unescape_html(html));
    let mut search_from = 0;
    while let Some(relative) = decoded[search_from..].find("https://") {
        let start = search_from + relative;
        let rest = &decoded[start..];
        let end = rest
            .find(['"', '\'', '\\', ' ', '\n', '<', '>'])
            .unwrap_or(rest.len());
        let candidate = rest[..end].trim();
        if candidate.contains(".mp4") {
            if let Ok(url) = Url::parse(candidate) {
                if url.path().to_ascii_lowercase().ends_with(".mp4") {
                    return Some(url);
                }
            }
        }
        search_from = start + 8;
    }
    None
}

/// Klasifikasi host → [`ServerKind`] (dipakai badge UI & pemilihan player).
pub fn classify_server(url: &Url) -> ServerKind {
    let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
    match host {
        h if h.contains("icefile") => ServerKind::Icefile,
        h if h.contains("desustream") => ServerKind::Desustream,
        h if h.contains("odcloud") => ServerKind::Odcloud,
        h if h.contains("blogger") || h.contains("blogspot") => ServerKind::Blogspot,
        h if h.contains("pixeldrain") => ServerKind::Pixeldrain,
        h if h.contains("gofile") => ServerKind::Gofile,
        h if h.contains("mega.nz") => ServerKind::Mega,
        h if h.contains("filedon") => ServerKind::Filedon,
        h if h.contains("acefile") => ServerKind::Acefile,
        h if h.contains("mediafire") => ServerKind::Mediafire,
        h if h.contains("zippy") => ServerKind::Zippy,
        h if h.contains("vidhide") || h.contains("odvidhide") => {
            ServerKind::Other("VidHide".into())
        }
        h if h.contains("kraken") => ServerKind::Other("Krakenfiles".into()),
        h if h.contains("wibufile") => ServerKind::Other("Wibufile".into()),
        h if h.contains("limewire") => ServerKind::Other("File.io".into()),
        h if h.contains("mir.cr") => ServerKind::Other("Mirrored".into()),
        _ => ServerKind::Mirror,
    }
}

/// Container dari tipe IceFile ("hls" | "mp4") atau ekstensi URL.
pub fn container_for(kind: &str, url: &Url) -> Option<Container> {
    match kind.to_ascii_lowercase().as_str() {
        "mp4" => return Some(Container::Mp4),
        "hls" | "m3u8" => return None, // HLS bukan container file tunggal
        _ => {}
    }
    let path = url.path().to_ascii_lowercase();
    if path.ends_with(".mp4") {
        Some(Container::Mp4)
    } else if path.ends_with(".mkv") {
        Some(Container::Mkv)
    } else {
        None
    }
}

/// Decode entity HTML (`&lt;`, `&quot;`, `&#038;`, `&amp;`).
pub fn unescape_html(text: &str) -> String {
    html_escape::decode_html_entities(text).into_owned()
}

/// Decode escape JSON/JS yang umum di halaman embed (`\u003d`, `\u0026`, `\/`).
pub fn unescape_js(text: &str) -> String {
    let mut out = text
        .replace("\\u003d", "=")
        .replace("\\u0026", "&")
        .replace("\\u003D", "=")
        .replace("\\u0026", "&")
        .replace("\\/", "/")
        .replace("\\\"", "\"");
    // Sisa escape unicode 4 digit: decode seperlunya untuk karakter ASCII.
    while let Some(at) = out.find("\\u00") {
        let end = at + 6;
        if end > out.len() {
            break;
        }
        let hex = &out[at + 4..end];
        let Ok(code) = u8::from_str_radix(hex, 16) else {
            break;
        };
        out.replace_range(at..end, &(code as char).to_string());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(path: &str) -> String {
        let full = format!("{}/tests/fixtures/{path}", env!("CARGO_MANIFEST_DIR"));
        std::fs::read_to_string(&full).unwrap_or_else(|err| panic!("baca {full}: {err}"))
    }

    #[test]
    fn iframe_src_extracts_and_unescapes() {
        let html = r#"<div><iframe width="420" src="https://example.test/embed?x=1&amp;y=2"></iframe></div>"#;
        let src = iframe_src(html).unwrap();
        assert_eq!(src, "https://example.test/embed?x=1&y=2");

        let escaped = "&lt;iframe src=&quot;https://example.test/e2&quot;&gt;&lt;/iframe&gt;";
        let unescaped = unescape_html(escaped);
        assert_eq!(
            iframe_src(&unescaped).as_deref(),
            Some("https://example.test/e2")
        );
        assert!(iframe_src("<div>tidak ada</div>").is_none());
    }

    #[test]
    fn icefile_sources_are_parsed_from_fixture() {
        let html = fixture("samehadaku/icefile_embed.html");
        let sources = icefile_sources(&html);
        assert!(sources.len() >= 3, "harus ada beberapa varian: {sources:?}");
        let resolutions: Vec<u16> = sources.iter().filter_map(|s| s.resolution).collect();
        assert!(resolutions.contains(&1080));
        assert!(resolutions.contains(&720));
        assert!(resolutions.contains(&360));
        assert!(sources.iter().any(|s| s.kind == "hls"));
        assert!(sources.iter().all(|s| s.link.as_str().contains("icefile")));
    }

    #[test]
    fn icefile_sources_absent_returns_empty() {
        assert!(icefile_sources("<html>tidak ada player</html>").is_empty());
    }

    #[test]
    fn desustream_media_supports_source_tag_and_video_url() {
        let source_html = fixture("otakudesu/embed-desustream.html");
        let media = desustream_media(&source_html).expect("harus menemukan media");
        assert!(media.as_str().contains("googlevideo") || media.as_str().contains(".mp4"));

        let lazy = r#"<script>const videoURL = "https://cdn.odcloud.net/anime/x.mp4";</script>"#;
        assert_eq!(
            desustream_media(lazy).map(|u| u.to_string()).as_deref(),
            Some("https://cdn.odcloud.net/anime/x.mp4")
        );
        assert!(desustream_media("<html></html>").is_none());
    }

    #[test]
    fn blogspot_media_is_extracted_from_escaped_json() {
        let html = fixture("samehadaku/blogspot_embed.html");
        let media = blogspot_media(&html).expect("harus menemukan videoplayback");
        assert!(media.as_str().contains("videoplayback"), "{media}");
        assert_eq!(googlevideo_resolution(&media), Some(360));
    }

    #[test]
    fn pixeldrain_direct_rewrites_share_links() {
        let url = Url::parse("https://pixeldrain.com/u/JDGqpwN9").unwrap();
        assert_eq!(
            pixeldrain_direct(&url).map(|u| u.to_string()),
            Some("https://pixeldrain.com/api/file/JDGqpwN9".to_string())
        );
        let not_pd = Url::parse("https://example.test/u/abc").unwrap();
        assert!(pixeldrain_direct(&not_pd).is_none());
    }

    #[test]
    fn server_classification_covers_known_hosts() {
        let cases = [
            ("https://beta.icefile.my.id/embed/x", ServerKind::Icefile),
            ("https://desustream.net/dstream/x", ServerKind::Desustream),
            ("https://cdn.odcloud.net/a.mp4", ServerKind::Odcloud),
            (
                "https://www.blogger.com/video.g?token=x",
                ServerKind::Blogspot,
            ),
            ("https://pixeldrain.com/u/x", ServerKind::Pixeldrain),
            ("https://mega.nz/embed/x", ServerKind::Mega),
            ("https://filedon.co/embed/x", ServerKind::Filedon),
            ("https://example.test/x", ServerKind::Mirror),
        ];
        for (raw, expected) in cases {
            let url = Url::parse(raw).unwrap();
            assert_eq!(classify_server(&url), expected, "{raw}");
        }
    }

    #[test]
    fn container_for_uses_kind_then_extension() {
        let mp4 = Url::parse("https://x.test/a.mp4").unwrap();
        let mkv = Url::parse("https://x.test/a.mkv").unwrap();
        let hls = Url::parse("https://x.test/a.m3u8").unwrap();
        assert_eq!(container_for("mp4", &hls), Some(Container::Mp4));
        assert_eq!(container_for("", &mp4), Some(Container::Mp4));
        assert_eq!(container_for("", &mkv), Some(Container::Mkv));
        assert_eq!(container_for("hls", &hls), None);
    }

    #[test]
    fn js_unescape_handles_common_sequences() {
        assert_eq!(unescape_js(r"a\u003db\u0026c\/d"), "a=b&c/d");
        assert_eq!(unescape_js(r"x\u0041y"), "xAy");
    }
}
