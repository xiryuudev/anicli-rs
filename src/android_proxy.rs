//! Proxy HTTP lokal untuk pemutaran di Android/Termux.
//!
//! Intent `VIEW` tidak bisa membawa header HTTP, padahal beberapa sumber
//! membutuhkannya:
//! - **desustream** (Otakudesu) menolak tanpa `User-Agent` tertentu;
//! - **Blogger/googlevideo** (Oploverz, Anoboy, AWSubs) butuh
//!   `Referer: https://www.blogger.com/`;
//! - HLS (`.m3u8`) butuh rewrite URL segmen agar tetap lewat proxy.
//!
//! Solusinya: server kecil di `127.0.0.1` yang meneruskan permintaan ke upstream
//! dengan header yang benar, lalu mpv-android cukup membuka URL proxy.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use base64::Engine as _;
use reqwest::header::{
    HeaderValue, ACCEPT_ENCODING, ACCEPT_RANGES, CONTENT_RANGE, CONTENT_TYPE, RANGE, REFERER,
    USER_AGENT,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use url::Url;

use crate::model::RequestHeaders;

/// Header yang ikut dikirim ke upstream oleh proxy.
#[derive(Debug, Clone, Default)]
pub struct ProxyHeaders {
    pub referer: Option<String>,
    pub user_agent: Option<String>,
}

impl From<&RequestHeaders> for ProxyHeaders {
    fn from(headers: &RequestHeaders) -> Self {
        Self {
            referer: headers.referer.as_ref().map(ToString::to_string),
            user_agent: headers.user_agent.clone(),
        }
    }
}

/// Server proxy yang sedang berjalan di localhost.
#[derive(Debug, Clone)]
pub struct ProxyHandle {
    base: String,
    token: String,
}

static PROXY: OnceLock<Mutex<Option<ProxyHandle>>> = OnceLock::new();

fn slot() -> &'static Mutex<Option<ProxyHandle>> {
    PROXY.get_or_init(|| Mutex::new(None))
}

/// Mulai proxy sekali saja, kembalikan handle yang sudah berjalan.
///
/// Bind ke `127.0.0.1:0` (port acak) dan berjalan selama proses hidup; task
/// listener di-spawn ke runtime tokio saat ini.
pub fn start() -> Result<ProxyHandle, String> {
    let mut guard = slot().lock().map_err(|_| "lock proxy rusak".to_string())?;
    if let Some(handle) = guard.as_ref() {
        return Ok(handle.clone());
    }

    let listener = std::net::TcpListener::bind("127.0.0.1:0")
        .map_err(|err| format!("gagal bind proxy lokal: {err}"))?;
    listener
        .set_nonblocking(true)
        .map_err(|err| format!("gagal set nonblocking: {err}"))?;
    let address = listener
        .local_addr()
        .map_err(|err| format!("gagal membaca alamat proxy: {err}"))?;

    let token = random_token();
    let base = format!("http://{address}/{token}");
    let handle = ProxyHandle {
        base: base.clone(),
        token: token.clone(),
    };

    tokio::spawn(async move {
        let listener = match TcpListener::from_std(listener) {
            Ok(listener) => listener,
            Err(err) => {
                tracing::warn!(target: "anicli_rs::proxy", %err, "gagal memulai listener proxy");
                return;
            }
        };
        loop {
            let Ok((socket, _)) = listener.accept().await else {
                break;
            };
            let token = token.clone();
            tokio::spawn(async move {
                if let Err(err) = serve_connection(socket, &token).await {
                    tracing::debug!(target: "anicli_rs::proxy", %err, "koneksi proxy berakhir");
                }
            });
        }
    });

    *guard = Some(handle.clone());
    Ok(handle)
}

/// URL proxy untuk satu media (dengan header yang disisipkan).
pub fn proxy_url(
    handle: &ProxyHandle,
    url: &Url,
    headers: &ProxyHeaders,
) -> Result<String, String> {
    #[derive(serde::Serialize)]
    struct Payload<'a> {
        url: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        referer: Option<&'a str>,
        #[serde(skip_serializing_if = "Option::is_none")]
        user_agent: Option<&'a str>,
    }

    let payload = Payload {
        url: url.as_str(),
        referer: headers.referer.as_deref(),
        user_agent: headers.user_agent.as_deref(),
    };
    let json = serde_json::to_vec(&payload).map_err(|err| format!("payload proxy: {err}"))?;
    let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(json);
    Ok(format!("{}/s/{encoded}", handle.base))
}

/// Apakah URL ini butuh proxy di Android?
///
/// HLS butuh proxy untuk rewrite playlist; host yang butuh header juga butuh;
/// selain itu bisa langsung.
pub fn needs_proxy(url: &Url, headers: &RequestHeaders) -> bool {
    // Intent Android tidak membawa header apa pun: bila link butuh Referer atau
    // User-Agent khusus, harus lewat proxy.
    if headers.referer.is_some() || headers.user_agent.is_some() {
        return true;
    }
    let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
    if host.contains("googlevideo") || host.contains("blogspot") || host.contains("blogger") {
        return true;
    }
    url.path().to_ascii_lowercase().ends_with(".m3u8")
        || url.as_str().to_ascii_lowercase().contains(".m3u8")
}

fn random_token() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or(0);
    let pid = std::process::id() as u128;
    format!("{nanos:x}{pid:x}")
}

struct Request {
    method: String,
    target: String,
    headers: HashMap<String, String>,
}

impl Request {
    async fn read(socket: &mut tokio::net::TcpStream) -> Result<Self, String> {
        let mut buffer = Vec::with_capacity(4096);
        let mut chunk = [0_u8; 2048];
        loop {
            let read = socket
                .read(&mut chunk)
                .await
                .map_err(|err| format!("baca permintaan: {err}"))?;
            if read == 0 {
                break;
            }
            buffer.extend_from_slice(&chunk[..read]);
            if buffer.windows(4).any(|window| window == b"\r\n\r\n") || buffer.len() > 64 * 1024 {
                break;
            }
        }
        let text = String::from_utf8_lossy(&buffer);
        let mut lines = text.split("\r\n");
        let request_line = lines.next().unwrap_or_default();
        let mut parts = request_line.split_whitespace();
        let method = parts.next().unwrap_or_default().to_string();
        let target = parts.next().unwrap_or_default().to_string();
        let mut headers = HashMap::new();
        for line in lines {
            if line.is_empty() {
                break;
            }
            if let Some((name, value)) = line.split_once(':') {
                headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_string());
            }
        }
        Ok(Self {
            method,
            target,
            headers,
        })
    }
}

async fn serve_connection(mut socket: tokio::net::TcpStream, token: &str) -> Result<(), String> {
    let request = Request::read(&mut socket).await?;
    if request.method != "GET" && request.method != "HEAD" {
        return respond(&mut socket, 405, "text/plain", b"method not allowed").await;
    }
    let prefix = format!("/{token}/s/");
    let Some(encoded) = request.target.strip_prefix(&prefix) else {
        return respond(&mut socket, 404, "text/plain", b"not found").await;
    };

    let payload = decode_payload(encoded)?;
    forward(&mut socket, &payload, &request, token).await
}

#[derive(Debug, Clone)]
struct Payload {
    url: Url,
    referer: Option<String>,
    user_agent: Option<String>,
}

fn decode_payload(encoded: &str) -> Result<Payload, String> {
    let raw = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(encoded)
        .map_err(|err| format!("payload tidak valid: {err}"))?;
    #[derive(serde::Deserialize)]
    struct Raw {
        url: String,
        #[serde(default)]
        referer: Option<String>,
        #[serde(default)]
        user_agent: Option<String>,
    }
    let raw: Raw = serde_json::from_slice(&raw).map_err(|err| format!("json proxy: {err}"))?;
    Ok(Payload {
        url: Url::parse(&raw.url).map_err(|err| format!("url proxy tidak valid: {err}"))?,
        referer: raw.referer,
        user_agent: raw.user_agent,
    })
}

async fn forward(
    socket: &mut tokio::net::TcpStream,
    payload: &Payload,
    request: &Request,
    token: &str,
) -> Result<(), String> {
    crate::http::ensure_tls_provider();
    let client = reqwest::Client::builder()
        .build()
        .map_err(|err| format!("client proxy: {err}"))?;
    let mut builder = client.get(payload.url.clone());
    if let Some(ua) = &payload.user_agent {
        if let Ok(value) = HeaderValue::from_str(ua) {
            builder = builder.header(USER_AGENT, value);
        }
    }
    if let Some(referer) = &payload.referer {
        if let Ok(value) = HeaderValue::from_str(referer) {
            builder = builder.header(REFERER, value);
        }
    }
    if let Some(range) = request.headers.get("range") {
        if let Ok(value) = HeaderValue::from_str(range) {
            builder = builder.header(RANGE, value);
        }
    }
    // Jangan minta kompresi: media sebaiknya diteruskan apa adanya.
    builder = builder.header(ACCEPT_ENCODING, "identity");

    let response = builder
        .send()
        .await
        .map_err(|err| format!("permintaan upstream: {err}"))?;
    let status = response.status();
    if !status.is_success() && status.as_u16() != 206 && status.as_u16() != 304 {
        return respond(
            socket,
            status.as_u16(),
            "text/plain",
            format!("upstream {status}").as_bytes(),
        )
        .await;
    }

    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("application/octet-stream")
        .to_string();
    let is_playlist = content_type.contains("mpegurl")
        || payload.url.path().to_ascii_lowercase().ends_with(".m3u8");

    if is_playlist {
        let body = response
            .text()
            .await
            .map_err(|err| format!("baca playlist: {err}"))?;
        let rewritten = rewrite_playlist(&body, &payload.url, &proxy_base_url(token))?;
        return respond(
            socket,
            200,
            "application/vnd.apple.mpegurl",
            rewritten.as_bytes(),
        )
        .await;
    }

    // Bila upstream memberi Content-Length (termasuk respons 206 untuk seeking),
    // teruskan angka itu tanpa chunked agar player bisa menghitung posisi.
    let content_length = response
        .headers()
        .get(reqwest::header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .map(ToString::to_string);
    let mut head = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\n",
        status.as_u16(),
        status.canonical_reason().unwrap_or("OK"),
        content_type
    );
    match &content_length {
        Some(length) => head.push_str(&format!("Content-Length: {length}\r\n")),
        None => head.push_str("Transfer-Encoding: chunked\r\n"),
    }
    for name in [CONTENT_RANGE, ACCEPT_RANGES] {
        if let Some(value) = response.headers().get(&name) {
            if let Ok(text) = value.to_str() {
                let label = if name == CONTENT_RANGE {
                    "Content-Range"
                } else {
                    "Accept-Ranges"
                };
                head.push_str(&format!("{label}: {text}\r\n"));
            }
        }
    }
    head.push_str("\r\n");
    socket
        .write_all(head.as_bytes())
        .await
        .map_err(|err| format!("tulis header: {err}"))?;

    let mut response = response;
    let chunked = content_length.is_none();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|err| format!("baca upstream: {err}"))?
    {
        if chunk.is_empty() {
            continue;
        }
        if chunked {
            let header = format!("{:x}\r\n", chunk.len());
            socket
                .write_all(header.as_bytes())
                .await
                .map_err(|err| format!("tulis chunk: {err}"))?;
        }
        socket
            .write_all(&chunk)
            .await
            .map_err(|err| format!("tulis body: {err}"))?;
        if chunked {
            socket
                .write_all(b"\r\n")
                .await
                .map_err(|err| format!("tulis chunk: {err}"))?;
        }
    }
    if chunked {
        socket
            .write_all(b"0\r\n\r\n")
            .await
            .map_err(|err| format!("tutup chunk: {err}"))?;
    }
    Ok(())
}

/// Tulis ulang playlist HLS: semua URI relatif/absolut diarahkan ke proxy.
///
/// `proxy_base` = basis proxy (mis. `http://127.0.0.1:PORT/token`).
pub fn rewrite_playlist(text: &str, base: &Url, proxy_base: &str) -> Result<String, String> {
    let mut output = String::with_capacity(text.len() + 256);
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            output.push('\n');
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix('#') {
            // Rewrite URI="..." di tag seperti EXT-X-KEY / EXT-X-MAP / EXT-X-MEDIA.
            if rest.contains("URI=\"") {
                let rewritten = rewrite_tag_uri(rest, base, proxy_base)?;
                output.push('#');
                output.push_str(&rewritten);
            } else {
                output.push_str(line);
            }
            output.push('\n');
            continue;
        }
        let absolute = base
            .join(trimmed)
            .map_err(|err| format!("URI playlist tidak valid: {err}"))?;
        let proxied = proxied_url(proxy_base, &absolute, &ProxyHeaders::default())?;
        output.push_str(&proxied);
        output.push('\n');
    }
    Ok(output)
}

fn proxy_base_url(token: &str) -> String {
    if let Ok(guard) = slot().lock() {
        if let Some(handle) = guard.as_ref() {
            if handle.token == token {
                return handle.base.clone();
            }
        }
    }
    format!("http://127.0.0.1/{token}")
}

fn proxied_url(proxy_base: &str, url: &Url, headers: &ProxyHeaders) -> Result<String, String> {
    let handle = ProxyHandle {
        base: proxy_base.to_string(),
        token: String::new(),
    };
    proxy_url(&handle, url, headers)
}

fn rewrite_tag_uri(tag: &str, base: &Url, proxy_base: &str) -> Result<String, String> {
    let Some(start) = tag.find("URI=\"") else {
        return Ok(tag.to_string());
    };
    let value_start = start + 5;
    let Some(end) = tag[value_start..].find('"') else {
        return Ok(tag.to_string());
    };
    let value = &tag[value_start..value_start + end];
    let absolute = base
        .join(value)
        .map_err(|err| format!("URI tag tidak valid: {err}"))?;
    let proxied = proxied_url(proxy_base, &absolute, &ProxyHeaders::default())?;
    let mut result = String::with_capacity(tag.len() + 64);
    result.push_str(&tag[..value_start]);
    result.push_str(&proxied);
    result.push_str(&tag[value_start + end..]);
    Ok(result)
}

async fn respond(
    socket: &mut tokio::net::TcpStream,
    status: u16,
    content_type: &str,
    body: &[u8],
) -> Result<(), String> {
    let reason = match status {
        200 => "OK",
        206 => "Partial Content",
        404 => "Not Found",
        405 => "Method Not Allowed",
        _ => "Error",
    };
    let head = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    socket
        .write_all(head.as_bytes())
        .await
        .map_err(|err| format!("tulis header: {err}"))?;
    socket
        .write_all(body)
        .await
        .map_err(|err| format!("tulis body: {err}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(raw: &str) -> Url {
        Url::parse(raw).unwrap()
    }

    #[test]
    fn playlist_keeps_tags_and_proxies_uris() {
        let base = url("https://cdn.example/hls/master.m3u8");
        let text = "#EXTM3U\n#EXT-X-VERSION:3\n#EXT-X-STREAM-INF:BANDWIDTH=1\n720p/index.m3u8\n";
        let rewritten = rewrite_playlist(text, &base, "http://127.0.0.1:9/tok").unwrap();
        assert!(rewritten.contains("#EXTM3U"));
        assert!(rewritten.contains("#EXT-X-STREAM-INF:BANDWIDTH=1"));
        assert!(
            rewritten.contains("/s/"),
            "URI playlist harus lewat proxy: {rewritten}"
        );
        assert!(!rewritten.contains("\n720p/index.m3u8"));
    }

    #[test]
    fn key_uri_is_rewritten() {
        let base = url("https://cdn.example/hls/index.m3u8");
        let text = "#EXTM3U\n#EXT-X-KEY:METHOD=AES-128,URI=\"key.bin\"\n#EXTINF:4,\nseg1.ts\n";
        let rewritten = rewrite_playlist(text, &base, "http://127.0.0.1:9/tok").unwrap();
        assert!(rewritten.contains("URI=\""), "{rewritten}");
        assert!(rewritten.contains("/s/"), "{rewritten}");
        assert!(!rewritten.contains("\nseg1.ts") && rewritten.contains("/s/"));
    }

    #[test]
    fn proxy_needed_for_headers_and_hls() {
        let plain = RequestHeaders::default();
        assert!(needs_proxy(&url("https://cdn.example/a.m3u8"), &plain));
        assert!(needs_proxy(
            &url("https://rr1---sn.googlevideo.com/videoplayback"),
            &plain
        ));
        assert!(!needs_proxy(&url("https://cdn.example/a.mp4"), &plain));
        let with_referer = RequestHeaders {
            referer: Some(url("https://samehadaku.example/")),
            user_agent: None,
        };
        assert!(needs_proxy(
            &url("https://cdn.example/a.mp4"),
            &with_referer
        ));
    }

    #[test]
    fn payload_roundtrip() {
        let handle = ProxyHandle {
            base: "http://127.0.0.1:9/tok".into(),
            token: "tok".into(),
        };
        let headers = ProxyHeaders {
            referer: Some("https://blogger.com/".into()),
            user_agent: Some("UA".into()),
        };
        let proxied = proxy_url(&handle, &url("https://cdn.example/a.mp4"), &headers).unwrap();
        let encoded = proxied.rsplit('/').next().unwrap();
        let payload = decode_payload(encoded).unwrap();
        assert_eq!(payload.url.as_str(), "https://cdn.example/a.mp4");
        assert_eq!(payload.referer.as_deref(), Some("https://blogger.com/"));
        assert_eq!(payload.user_agent.as_deref(), Some("UA"));
    }
}
