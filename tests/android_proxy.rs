//! Uji proxy header lokal (dipakai saat memutar di Android/Termux).

use std::sync::{Arc, Mutex};

use anicli_rs::android_proxy::{self, ProxyHeaders};
use anicli_rs::model::RequestHeaders;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use url::Url;

/// Server upstream palsu: mencatat header `referer` dan `user-agent` yang diterima.
async fn upstream() -> (String, Arc<Mutex<Vec<(String, String)>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let seen: Arc<Mutex<Vec<(String, String)>>> = Arc::new(Mutex::new(Vec::new()));
    let seen_clone = seen.clone();
    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            let seen = seen_clone.clone();
            tokio::spawn(async move {
                let mut buffer = [0_u8; 4096];
                let read = socket.read(&mut buffer).await.unwrap_or(0);
                let text = String::from_utf8_lossy(&buffer[..read]).to_string();
                let mut headers = Vec::new();
                for line in text.split("\r\n").skip(1) {
                    if let Some((name, value)) = line.split_once(':') {
                        headers.push((name.trim().to_ascii_lowercase(), value.trim().to_string()));
                    }
                }
                seen.lock().unwrap().extend(headers);
                let body = b"video-bytes";
                let head = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: video/mp4\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = socket.write_all(head.as_bytes()).await;
                let _ = socket.write_all(body).await;
                let _ = socket.flush().await;
            });
        }
    });
    (format!("http://{addr}"), seen)
}

#[tokio::test]
async fn proxy_injects_referer_and_user_agent() {
    let (base, seen) = upstream().await;
    let handle = android_proxy::start().expect("proxy mulai");

    let upstream_url = Url::parse(&format!("{base}/movie.mp4")).unwrap();
    let headers = ProxyHeaders {
        referer: Some("https://www.blogger.com/".to_string()),
        user_agent: Some("anicli-test/1.0".to_string()),
    };
    let proxied = android_proxy::proxy_url(&handle, &upstream_url, &headers).unwrap();

    let client = reqwest::Client::new();
    let response = client.get(&proxied).send().await.expect("permintaan proxy");
    assert!(
        response.status().is_success(),
        "status: {}",
        response.status()
    );
    let body = response.bytes().await.unwrap();
    assert_eq!(&body[..], b"video-bytes");

    let seen = seen.lock().unwrap();
    let referer = seen.iter().find(|(name, _)| name == "referer");
    assert_eq!(
        referer.map(|(_, value)| value.as_str()),
        Some("https://www.blogger.com/"),
        "header referer harus diteruskan: {seen:?}"
    );
    let ua = seen.iter().find(|(name, _)| name == "user-agent");
    assert_eq!(
        ua.map(|(_, value)| value.as_str()),
        Some("anicli-test/1.0"),
        "header user-agent harus diteruskan: {seen:?}"
    );
}

#[tokio::test]
async fn proxy_without_headers_forwards_plain_request() {
    let (base, seen) = upstream().await;
    let handle = android_proxy::start().expect("proxy mulai");
    let upstream_url = Url::parse(&format!("{base}/plain.mp4")).unwrap();
    let proxied =
        android_proxy::proxy_url(&handle, &upstream_url, &ProxyHeaders::default()).unwrap();

    anicli_rs::http::ensure_tls_provider();
    let client = reqwest::Client::new();
    let response = client.get(&proxied).send().await.expect("permintaan proxy");
    assert!(response.status().is_success());
    let seen = seen.lock().unwrap();
    assert!(
        !seen.iter().any(|(name, _)| name == "referer"),
        "tidak boleh menambah referer: {seen:?}"
    );
}

#[test]
fn proxy_requirement_policy() {
    let plain = RequestHeaders::default();
    let url = |raw: &str| Url::parse(raw).unwrap();

    // googlevideo/blogger & HLS selalu butuh proxy.
    assert!(android_proxy::needs_proxy(
        &url("https://rr1---sn-x.googlevideo.com/videoplayback"),
        &plain
    ));
    assert!(android_proxy::needs_proxy(
        &url("https://cdn.x/a.m3u8"),
        &plain
    ));
    // Header berarti butuh proxy.
    let with_ua = RequestHeaders {
        referer: None,
        user_agent: Some("UA".into()),
    };
    assert!(android_proxy::needs_proxy(
        &url("https://cdn.x/a.mp4"),
        &with_ua
    ));
    // mp4 tanpa header cukup langsung.
    assert!(!android_proxy::needs_proxy(
        &url("https://cdn.x/a.mp4"),
        &plain
    ));
}
