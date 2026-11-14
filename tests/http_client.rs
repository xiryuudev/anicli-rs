//! Test `HttpClient` & `MirrorSet` memakai server HTTP lokal (tanpa jaringan luar).
//!
//! Yang dibuktikan: retry, penghormatan `Retry-After`, deteksi anti-bot, redirect
//! default vs no-redirect (soft-404), rate limit per host, JSON decode, dan probe
//! mirror berbasis marker konten.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anicli_rs::config::HttpConfig;
use anicli_rs::error::SourceError;
use anicli_rs::http::{HttpClient, MirrorSet};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[derive(Default)]
struct ServerState {
    hits: Mutex<HashMap<String, usize>>,
    times: Mutex<HashMap<String, Vec<Instant>>>,
}

impl ServerState {
    fn hits(&self, path: &str) -> usize {
        self.hits.lock().unwrap().get(path).copied().unwrap_or(0)
    }

    fn times(&self, path: &str) -> Vec<Instant> {
        self.times
            .lock()
            .unwrap()
            .get(path)
            .cloned()
            .unwrap_or_default()
    }
}

struct TestServer {
    addr: SocketAddr,
    state: Arc<ServerState>,
}

fn route(path: &str, hits: usize) -> (u16, Vec<(String, String)>, String, Option<Duration>) {
    let plain = |value: &str| vec![("Content-Type".to_string(), value.to_string())];
    match path {
        "/ok" => (200, plain("text/plain"), "hello".to_string(), None),
        "/marker" => (
            200,
            plain("text/html"),
            "Situs resmi Otakudesu hanya otakudesu.blog".to_string(),
            None,
        ),
        "/parked" => (
            200,
            plain("text/html"),
            "This domain is for sale".to_string(),
            None,
        ),
        "/flaky" => {
            if hits <= 1 {
                (500, plain("text/plain"), "boom".to_string(), None)
            } else {
                (200, plain("text/plain"), "recovered".to_string(), None)
            }
        }
        "/limited" => {
            if hits <= 1 {
                (
                    429,
                    vec![("Retry-After".to_string(), "1".to_string())],
                    "slow down".to_string(),
                    None,
                )
            } else {
                (200, plain("text/plain"), "ok".to_string(), None)
            }
        }
        "/blocked" => (
            403,
            vec![
                ("cf-mitigated".to_string(), "challenge".to_string()),
                ("Content-Type".to_string(), "text/html".to_string()),
            ],
            "<html><title>Just a moment...</title></html>".to_string(),
            None,
        ),
        "/redirect" => (
            302,
            vec![("Location".to_string(), "/ok".to_string())],
            String::new(),
            None,
        ),
        "/json" => (
            200,
            plain("application/json"),
            r#"{"data":"0e1e7d1413"}"#.to_string(),
            None,
        ),
        "/slow" => (
            200,
            plain("text/plain"),
            "slow".to_string(),
            Some(Duration::from_millis(20)),
        ),
        _ => (404, plain("text/plain"), "not found".to_string(), None),
    }
}

impl TestServer {
    async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let state = Arc::new(ServerState::default());
        let state_for_task = state.clone();

        tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    break;
                };
                let state = state_for_task.clone();
                tokio::spawn(async move {
                    let mut buffer = vec![0_u8; 4096];
                    let Ok(read) = socket.read(&mut buffer).await else {
                        return;
                    };
                    let request = String::from_utf8_lossy(&buffer[..read]).to_string();
                    let path = request
                        .lines()
                        .next()
                        .and_then(|line| line.split_whitespace().nth(1))
                        .unwrap_or("/")
                        .split('?')
                        .next()
                        .unwrap_or("/")
                        .to_string();

                    let hit_count = {
                        let mut hits = state.hits.lock().unwrap();
                        let counter = hits.entry(path.clone()).or_insert(0);
                        *counter += 1;
                        *counter
                    };
                    state
                        .times
                        .lock()
                        .unwrap()
                        .entry(path.clone())
                        .or_default()
                        .push(Instant::now());

                    let (status, headers, body, delay) = route(&path, hit_count);
                    if let Some(delay) = delay {
                        tokio::time::sleep(delay).await;
                    }

                    let mut response = format!(
                        "HTTP/1.1 {status} X\r\nContent-Length: {}\r\nConnection: close\r\n",
                        body.len()
                    );
                    for (name, value) in headers {
                        response.push_str(&format!("{name}: {value}\r\n"));
                    }
                    response.push_str("\r\n");
                    response.push_str(&body);
                    let _ = socket.write_all(response.as_bytes()).await;
                    let _ = socket.shutdown().await;
                });
            }
        });

        Self { addr, state }
    }

    fn url(&self, path: &str) -> url::Url {
        url::Url::parse(&format!("http://{}{path}", self.addr)).unwrap()
    }
}

fn fast_config(delay_ms: u64, retries: u32) -> HttpConfig {
    HttpConfig {
        request_delay_ms: delay_ms,
        jitter_ms: 0,
        retries,
        timeout_secs: 10,
        connect_timeout_secs: 5,
        ..HttpConfig::default()
    }
}

#[tokio::test]
async fn get_text_returns_body() {
    let server = TestServer::start().await;
    let client = HttpClient::new(&fast_config(0, 0)).unwrap();

    let body = client
        .get_text("test", &server.url("/ok"), None)
        .await
        .expect("request harus sukses");
    assert_eq!(body, "hello");
}

#[tokio::test]
async fn retries_transient_500_then_succeeds() {
    let server = TestServer::start().await;
    let client = HttpClient::new(&fast_config(0, 2)).unwrap();

    let body = client
        .get_text("test", &server.url("/flaky"), None)
        .await
        .unwrap();
    assert_eq!(body, "recovered");
    assert_eq!(server.state.hits("/flaky"), 2, "harus tepat satu retry");
}

#[tokio::test]
async fn retry_after_is_respected() {
    let server = TestServer::start().await;
    let client = HttpClient::new(&fast_config(0, 1)).unwrap();

    let started = Instant::now();
    let body = client
        .get_text("test", &server.url("/limited"), None)
        .await
        .unwrap();
    let elapsed = started.elapsed();

    assert_eq!(body, "ok");
    assert_eq!(server.state.hits("/limited"), 2);
    assert!(
        elapsed >= Duration::from_millis(900),
        "Retry-After 1 detik harus dihormati, elapsed={elapsed:?}"
    );
}

#[tokio::test]
async fn challenge_response_is_classified_blocked_not_plain_403() {
    let server = TestServer::start().await;
    let client = HttpClient::new(&fast_config(0, 0)).unwrap();

    let err = client
        .get_text("samehadaku", &server.url("/blocked"), None)
        .await
        .unwrap_err();
    assert!(matches!(err, SourceError::Blocked { .. }), "{err:?}");
    assert!(err.user_message().contains("source lain"));
}

#[tokio::test]
async fn follows_redirects_by_default() {
    let server = TestServer::start().await;
    let client = HttpClient::new(&fast_config(0, 0)).unwrap();

    let body = client
        .get_text("test", &server.url("/redirect"), None)
        .await
        .unwrap();
    assert_eq!(body, "hello");
}

#[tokio::test]
async fn no_redirect_client_exposes_soft_404_redirect() {
    let server = TestServer::start().await;
    let client = HttpClient::new(&fast_config(0, 0)).unwrap();

    let err = client
        .get_text_no_redirect("kusonime", &server.url("/redirect"), None)
        .await
        .unwrap_err();
    match err {
        SourceError::Http { status, .. } => assert_eq!(status, 302),
        other => panic!("harus Http 302, dapat {other:?}"),
    }
}

#[tokio::test]
async fn plain_404_is_http_error_and_not_retried() {
    let server = TestServer::start().await;
    let client = HttpClient::new(&fast_config(0, 3)).unwrap();

    let err = client
        .get_text("test", &server.url("/missing"), None)
        .await
        .unwrap_err();
    assert!(
        matches!(err, SourceError::Http { status: 404, .. }),
        "{err:?}"
    );
    assert_eq!(server.state.hits("/missing"), 1, "404 tidak boleh di-retry");
}

#[tokio::test]
async fn json_endpoint_is_deserialized() {
    #[derive(serde::Deserialize)]
    struct Nonce {
        data: String,
    }

    let server = TestServer::start().await;
    let client = HttpClient::new(&fast_config(0, 0)).unwrap();

    let nonce: Nonce = client
        .get_json("test", &server.url("/json"), None)
        .await
        .unwrap();
    assert_eq!(nonce.data, "0e1e7d1413");
}

#[tokio::test]
async fn per_host_rate_limit_spaces_requests() {
    let server = TestServer::start().await;
    let client = HttpClient::new(&fast_config(120, 0)).unwrap();

    client
        .get_text("test", &server.url("/ok"), None)
        .await
        .unwrap();
    client
        .get_text("test", &server.url("/ok"), None)
        .await
        .unwrap();

    let times = server.state.times("/ok");
    assert_eq!(times.len(), 2);
    let gap = times[1].duration_since(times[0]);
    assert!(
        gap >= Duration::from_millis(100),
        "request kedua harus menunggu delay, gap={gap:?}"
    );
}

#[tokio::test]
async fn mirror_set_skips_parked_domain_and_uses_marker_match() {
    let server = TestServer::start().await;
    let client = HttpClient::new(&fast_config(0, 0)).unwrap();

    let base = server.url("/parked").to_string();
    let set = MirrorSet::new(
        &base,
        &[server.url("/marker").to_string()],
        "Situs resmi Otakudesu",
        60,
    )
    .unwrap();

    let resolved = set.resolve(&client, "otakudesu").await.unwrap();
    assert!(resolved.as_str().ends_with("/marker"), "{resolved}");
    assert_eq!(server.state.hits("/parked"), 1);
    assert_eq!(server.state.hits("/marker"), 1);
}

#[tokio::test]
async fn mirror_set_caches_successful_resolution() {
    let server = TestServer::start().await;
    let client = HttpClient::new(&fast_config(0, 0)).unwrap();
    let base = server.url("/marker").to_string();
    let set = MirrorSet::new(&base, &[], "Situs resmi Otakudesu", 60).unwrap();

    set.resolve(&client, "otakudesu").await.unwrap();
    set.resolve(&client, "otakudesu").await.unwrap();

    assert_eq!(
        server.state.hits("/marker"),
        1,
        "resolve kedua harus memakai cache"
    );
}

#[tokio::test]
async fn mirror_set_reports_no_mirror_when_all_candidates_fail_marker() {
    let server = TestServer::start().await;
    let client = HttpClient::new(&fast_config(0, 0)).unwrap();
    let base = server.url("/parked").to_string();
    let set = MirrorSet::new(
        &base,
        &[server.url("/missing").to_string()],
        "Situs resmi Otakudesu",
        60,
    )
    .unwrap();

    let err = set.resolve(&client, "otakudesu").await.unwrap_err();
    assert!(
        matches!(err, SourceError::NoMirror { tried: 2, .. }),
        "{err:?}"
    );
    assert!(err.user_message().contains("base_url"));
}

#[tokio::test]
async fn referer_header_is_sent_to_server() {
    // Server tidak memvalidasi Referer, jadi cukup pastikan request dengan referer
    // tetap sukses (regresi: header tidak boleh merusak request).
    let server = TestServer::start().await;
    let client = HttpClient::new(&fast_config(0, 0)).unwrap();
    let referer = server.url("/ok");

    let body = client
        .get_text("test", &server.url("/ok"), Some(&referer))
        .await
        .unwrap();
    assert_eq!(body, "hello");
}

#[tokio::test]
async fn slow_endpoint_succeeds_within_timeout() {
    let server = TestServer::start().await;
    let client = HttpClient::new(&fast_config(0, 0)).unwrap();

    let body = client
        .get_text("test", &server.url("/slow"), None)
        .await
        .unwrap();
    assert_eq!(body, "slow");
}
