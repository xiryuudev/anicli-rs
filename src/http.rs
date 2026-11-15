//! HTTP client bersama + penegakan sopan santun (DESIGN §8).
//!
//! Satu `HttpClient` dipakai semua source (connection pooling), dengan kebijakan:
//! - **Rate limit per host**: request ke host yang sama diserialisasi + jeda
//!   `request_delay_ms` + jitter. Batas global `max_concurrent`.
//! - **Retry**: hanya untuk error jaringan/timeout/429/5xx, dengan backoff
//!   eksponensial; `Retry-After` dihormati (maks 30 detik).
//! - **Deteksi anti-bot**: 403/503 dengan marker challenge → `SourceError::Blocked`
//!   (bukan sekadar HTTP 403, karena Samehadaku membalas 403 untuk UA generik).
//! - **Dua client**: satu mengikuti redirect (default), satu tidak — dibutuhkan
//!   untuk mendeteksi soft-404 (Kusonime mengalihkan slug salah ke homepage).
//! - [`MirrorSet`]: probe domain utama + mirror, validasi **marker konten** (bukan
//!   hanya status 200), cache hasilnya TTL.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use reqwest::header::{HeaderMap, HeaderValue, REFERER, RETRY_AFTER};
use reqwest::{Client, StatusCode, Url};
use serde::de::DeserializeOwned;
use tokio::sync::{Mutex as AsyncMutex, Semaphore};

use crate::config::HttpConfig;
use crate::error::{SourceError, SourceResult};

/// Batas maksimum menghormati `Retry-After` supaya aplikasi tidak menggantung.
const MAX_RETRY_AFTER: Duration = Duration::from_secs(30);

/// Backoff dasar untuk retry (dikali 2^attempt).
const BASE_BACKOFF: Duration = Duration::from_millis(700);

/// Spesifikasi permintaan yang didukung klien.
#[derive(Debug)]
enum RequestSpec<'a> {
    Get {
        url: &'a Url,
        referer: Option<&'a Url>,
    },
    PostForm {
        url: &'a Url,
        referer: Option<&'a Url>,
        form: &'a [(&'a str, String)],
    },
    PostMultipart {
        url: &'a Url,
        referer: Option<&'a Url>,
        fields: &'a [(&'a str, String)],
    },
}

impl RequestSpec<'_> {
    fn url(&self) -> &Url {
        match self {
            Self::Get { url, .. }
            | Self::PostForm { url, .. }
            | Self::PostMultipart { url, .. } => url,
        }
    }

    fn referer(&self) -> Option<&Url> {
        match self {
            Self::Get { referer, .. }
            | Self::PostForm { referer, .. }
            | Self::PostMultipart { referer, .. } => *referer,
        }
    }
}

/// Client HTTP bersama.
#[derive(Debug, Clone)]
pub struct HttpClient {
    client: Client,
    client_no_redirect: Client,
    config: HttpConfig,
    global: Arc<Semaphore>,
    hosts: Arc<AsyncMutex<HashMap<String, Arc<AsyncMutex<()>>>>>,
    last_hit: Arc<std::sync::Mutex<HashMap<String, Instant>>>,
}

/// Pasang provider TLS ring sebagai default proses (idempotent).
///
/// `reqwest` dibangun dengan `rustls-no-provider` agar tidak memaksa `aws-lc-rs`
/// (butuh cmake/NASM, menyulitkan build Termux/Android). Provider dipasang sebelum
/// client pertama dibuat.
pub fn ensure_tls_provider() {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        // Hanya gagal bila sudah ada provider lain terpasang — tidak fatal.
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

impl HttpClient {
    pub fn new(config: &HttpConfig) -> SourceResult<Self> {
        ensure_tls_provider();
        Ok(Self {
            client: build_client(config, true)?,
            client_no_redirect: build_client(config, false)?,
            config: config.clone(),
            global: Arc::new(Semaphore::new(config.max_concurrent.max(1))),
            hosts: Arc::new(AsyncMutex::new(HashMap::new())),
            last_hit: Arc::new(std::sync::Mutex::new(HashMap::new())),
        })
    }

    pub fn config(&self) -> &HttpConfig {
        &self.config
    }

    /// GET dan kembalikan body teks (redirect diikuti).
    pub async fn get_text(
        &self,
        source: &'static str,
        url: &Url,
        referer: Option<&Url>,
    ) -> SourceResult<String> {
        let spec = RequestSpec::Get { url, referer };
        let response = self.execute(source, &spec, true).await?;
        response
            .text()
            .await
            .map_err(|err| map_reqwest(source, "body", url, err))
    }

    /// GET tanpa mengikuti redirect — untuk mendeteksi soft-404 (DESIGN §9 poin 5).
    pub async fn get_text_no_redirect(
        &self,
        source: &'static str,
        url: &Url,
        referer: Option<&Url>,
    ) -> SourceResult<String> {
        let spec = RequestSpec::Get { url, referer };
        let response = self.execute(source, &spec, false).await?;
        response
            .text()
            .await
            .map_err(|err| map_reqwest(source, "body", url, err))
    }

    /// GET lalu deserialisasi JSON (endpoint AJAX/REST source).
    pub async fn get_json<T: DeserializeOwned>(
        &self,
        source: &'static str,
        url: &Url,
        referer: Option<&Url>,
    ) -> SourceResult<T> {
        let spec = RequestSpec::Get { url, referer };
        let response = self.execute(source, &spec, true).await?;
        response
            .json::<T>()
            .await
            .map_err(|err| map_reqwest(source, "json", url, err))
    }

    /// POST `application/x-www-form-urlencoded` lalu kembalikan body teks.
    ///
    /// Dipakai endpoint AJAX WordPress (Samehadaku `player_ajax`, Otakudesu nonce/mirror)
    /// yang membalas HTML atau JSON — bukan error HTTP untuk kegagalan logis.
    pub async fn post_form_text(
        &self,
        source: &'static str,
        url: &Url,
        form: &[(&str, String)],
        referer: Option<&Url>,
    ) -> SourceResult<String> {
        let spec = RequestSpec::PostForm { url, referer, form };
        let response = self.execute(source, &spec, true).await?;
        response
            .text()
            .await
            .map_err(|err| map_reqwest(source, "body", url, err))
    }

    /// POST `multipart/form-data` lalu kembalikan body teks (dipakai AJAX player KotakAnime).
    pub async fn post_multipart_text(
        &self,
        source: &'static str,
        url: &Url,
        fields: &[(&str, String)],
        referer: Option<&Url>,
    ) -> SourceResult<String> {
        let spec = RequestSpec::PostMultipart {
            url,
            referer,
            fields,
        };
        let response = self.execute(source, &spec, true).await?;
        response
            .text()
            .await
            .map_err(|err| map_reqwest(source, "body", url, err))
    }

    /// Inti permintaan: semaphore global → lock per host → throttle → retry.
    async fn execute(
        &self,
        source: &'static str,
        spec: &RequestSpec<'_>,
        follow_redirects: bool,
    ) -> SourceResult<reqwest::Response> {
        let url = spec.url();
        let host = url.host_str().unwrap_or("unknown").to_string();
        let _permit = self
            .global
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| SourceError::app("konkurensi HTTP ditutup"))?;

        let host_lock = self.host_lock(&host).await;
        let _host_guard = host_lock.lock_owned().await;

        let client = if follow_redirects {
            &self.client
        } else {
            &self.client_no_redirect
        };
        let attempts = 1 + self.config.retries;
        let mut last_error: Option<SourceError> = None;

        for attempt in 0..attempts {
            self.throttle(&host).await;

            match self.send_once(client, spec).await {
                Ok(response) => {
                    let status = response.status();
                    if status.is_success() {
                        return Ok(response);
                    }
                    let retry_after = parse_retry_after(response.headers());
                    let body = response.text().await.unwrap_or_default();
                    let error = classify_status(source, url, status, &body);
                    if error.is_retryable() && attempt + 1 < attempts {
                        tracing::debug!(
                            target: "anicli_rs::http",
                            source, %url, status = status.as_u16(), attempt,
                            "retry setelah status non-sukses"
                        );
                        last_error = Some(error);
                        tokio::time::sleep(retry_after.unwrap_or_else(|| backoff(attempt))).await;
                        continue;
                    }
                    return Err(error);
                }
                Err(error) => {
                    let error = map_reqwest(source, "request", url, error);
                    if error.is_retryable() && attempt + 1 < attempts {
                        tracing::debug!(
                            target: "anicli_rs::http",
                            source, %url, attempt, %error,
                            "retry setelah error jaringan"
                        );
                        last_error = Some(error);
                        tokio::time::sleep(backoff(attempt)).await;
                        continue;
                    }
                    return Err(error);
                }
            }
        }

        Err(last_error.unwrap_or_else(|| SourceError::app("permintaan gagal tanpa alasan")))
    }

    async fn send_once(
        &self,
        client: &Client,
        spec: &RequestSpec<'_>,
    ) -> reqwest::Result<reqwest::Response> {
        let mut request = match spec {
            RequestSpec::Get { url, .. } => client.get((*url).clone()),
            RequestSpec::PostForm { url, form, .. } => client.post((*url).clone()).form(form),
            RequestSpec::PostMultipart { url, fields, .. } => {
                let form = fields
                    .iter()
                    .fold(reqwest::multipart::Form::new(), |form, (name, value)| {
                        form.text((*name).to_string(), value.clone())
                    });
                client.post((*url).clone()).multipart(form)
            }
        }
        .header(reqwest::header::ACCEPT_LANGUAGE, "id-ID,id;q=0.9,en;q=0.8");
        if let Some(referer) = spec.referer() {
            request = request.header(REFERER, referer.as_str());
        }
        request.send().await
    }

    async fn host_lock(&self, host: &str) -> Arc<AsyncMutex<()>> {
        let mut hosts = self.hosts.lock().await;
        hosts
            .entry(host.to_string())
            .or_insert_with(|| Arc::new(AsyncMutex::new(())))
            .clone()
    }

    /// Jeda antar request ke host yang sama (DESIGN §8).
    async fn throttle(&self, host: &str) {
        let delay = Duration::from_millis(self.config.request_delay_ms) + self.jitter();
        let wait = {
            let mut map = match self.last_hit.lock() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };
            let now = Instant::now();
            let scheduled = map.get(host).map(|last| *last + delay).unwrap_or(now);
            map.insert(host.to_string(), now.max(scheduled));
            scheduled.saturating_duration_since(now)
        };
        if !wait.is_zero() {
            tokio::time::sleep(wait).await;
        }
    }

    fn jitter(&self) -> Duration {
        if self.config.jitter_ms == 0 {
            return Duration::ZERO;
        }
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|elapsed| elapsed.subsec_nanos())
            .unwrap_or(0);
        Duration::from_millis(u64::from(nanos) % self.config.jitter_ms.max(1))
    }
}

/// Konfigurasi TLS rustls dengan root CA statis (`webpki-roots`).
///
/// Sengaja tidak memakai verifier bawaan reqwest: di Android verifier itu
/// (`rustls-platform-verifier`) butuh JNI/`Context` dan akan panic pada binary
/// Termux yang bukan aplikasi JNI. Root statis bekerja sama baik di desktop
/// maupun Android.
pub fn tls_config() -> rustls::ClientConfig {
    ensure_tls_provider();
    let mut roots = rustls::RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let mut config = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    // ALPN untuk HTTP/2 (reqwest mengaktifkan http2).
    config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
    config
}

fn build_client(config: &HttpConfig, follow_redirects: bool) -> SourceResult<Client> {
    let mut builder = Client::builder()
        .user_agent(config.user_agent.clone())
        .timeout(Duration::from_secs(config.timeout_secs.max(1)))
        .connect_timeout(Duration::from_secs(config.connect_timeout_secs.max(1)))
        .gzip(true)
        .brotli(true)
        .cookie_store(true)
        .use_preconfigured_tls(tls_config());

    if !follow_redirects {
        builder = builder.redirect(reqwest::redirect::Policy::none());
    }

    builder
        .build()
        .map_err(|err| SourceError::app(format!("gagal membuat HTTP client: {err}")))
}

fn map_reqwest(
    source: &'static str,
    step: &'static str,
    url: &Url,
    error: reqwest::Error,
) -> SourceError {
    if error.is_timeout() {
        return SourceError::Timeout { site: source, step };
    }
    SourceError::Network {
        site: source,
        url: url.to_string(),
        detail: error.to_string(),
    }
}

fn classify_status(source: &'static str, url: &Url, status: StatusCode, body: &str) -> SourceError {
    let code = status.as_u16();
    if (code == 403 || code == 503) && is_challenge(body) {
        return SourceError::Blocked {
            site: source,
            url: url.to_string(),
        };
    }
    SourceError::Http {
        site: source,
        url: url.to_string(),
        status: code,
    }
}

/// Marker challenge Cloudflare/anti-bot di body (DESIGN §8.2).
fn is_challenge(body: &str) -> bool {
    let lowered = body.to_ascii_lowercase();
    lowered.contains("just a moment")
        || lowered.contains("challenge-platform")
        || lowered.contains("__cf_chl")
        || lowered.contains("cf-mitigated")
        || lowered.contains("enable javascript and cookies to continue")
}

fn parse_retry_after(headers: &HeaderMap) -> Option<Duration> {
    let value = headers.get(RETRY_AFTER)?.to_str().ok()?.trim();
    let seconds: u64 = value.parse().ok()?;
    Some(Duration::from_secs(seconds).min(MAX_RETRY_AFTER))
}

fn backoff(attempt: u32) -> Duration {
    let factor = 1_u32.checked_shl(attempt.min(6)).unwrap_or(64);
    BASE_BACKOFF * factor
}

// =============================================================================
// MirrorSet
// =============================================================================

#[derive(Debug)]
struct CachedMirror {
    url: Url,
    at: Instant,
}

/// Kumpulan domain kandidat untuk satu source + validasi marker konten.
///
/// Alasan: domain situs anime sering berpindah dan domain lama bisa menjadi parkir
/// (contoh nyata: `otakudesu.cam` → "This domain is for sale", `samehadaku.care` →
/// portal JS). Status 200 saja tidak cukup, karena itu `resolve` memeriksa marker.
#[derive(Debug)]
pub struct MirrorSet {
    candidates: Vec<Url>,
    marker: String,
    ttl: Duration,
    cache: std::sync::Mutex<Option<CachedMirror>>,
}

impl MirrorSet {
    /// `base_url` wajib; `mirrors` opsional. Marker = teks yang harus muncul di
    /// halaman home untuk membuktikan domain itu benar.
    pub fn new(
        base_url: &str,
        mirrors: &[String],
        marker: impl Into<String>,
        ttl_secs: u64,
    ) -> SourceResult<Self> {
        let base = Url::parse(base_url.trim())
            .map_err(|err| SourceError::app(format!("base_url tidak valid {base_url:?}: {err}")))?;
        let mut candidates = vec![base];
        for mirror in mirrors {
            let trimmed = mirror.trim();
            if trimmed.is_empty() {
                continue;
            }
            let Ok(url) = Url::parse(trimmed) else {
                tracing::warn!(
                    target: "anicli_rs::http",
                    mirror = trimmed,
                    "mirror bukan URL valid, diabaikan"
                );
                continue;
            };
            if !candidates.contains(&url) {
                candidates.push(url);
            }
        }

        Ok(Self {
            candidates,
            marker: marker.into(),
            ttl: Duration::from_secs(ttl_secs.max(1)),
            cache: std::sync::Mutex::new(None),
        })
    }

    pub fn candidates(&self) -> &[Url] {
        &self.candidates
    }

    pub fn marker(&self) -> &str {
        &self.marker
    }

    /// URL hasil resolve terakhir, kalau masih segar.
    pub fn cached(&self) -> Option<Url> {
        let guard = self.cache.lock().ok()?;
        let cached = guard.as_ref()?;
        (cached.at.elapsed() < self.ttl).then(|| cached.url.clone())
    }

    pub fn invalidate(&self) {
        if let Ok(mut guard) = self.cache.lock() {
            *guard = None;
        }
    }

    /// Probe kandidat berurutan sampai ada yang membalas 200 **dan** memuat marker.
    pub async fn resolve(&self, http: &HttpClient, source: &'static str) -> SourceResult<Url> {
        if let Some(url) = self.cached() {
            return Ok(url);
        }

        let mut failures: Vec<String> = Vec::new();
        for candidate in &self.candidates {
            match http.get_text(source, candidate, None).await {
                Ok(body) if body.contains(&self.marker) => {
                    if let Ok(mut guard) = self.cache.lock() {
                        *guard = Some(CachedMirror {
                            url: candidate.clone(),
                            at: Instant::now(),
                        });
                    }
                    tracing::info!(
                        target: "anicli_rs::http",
                        source, mirror = %candidate,
                        "domain aktif ditemukan"
                    );
                    return Ok(candidate.clone());
                }
                Ok(_) => failures.push(format!("{candidate}: 200 tanpa marker {:?}", self.marker)),
                Err(err) => failures.push(format!("{candidate}: {err}")),
            }
        }

        for failure in &failures {
            tracing::warn!(target: "anicli_rs::http", source, failure, "kandidat mirror gagal");
        }
        Err(SourceError::NoMirror {
            site: source,
            tried: self.candidates.len(),
        })
    }
}

/// Header bantu: `Referer` untuk player (dipakai nanti di `player.rs`).
pub fn referer_header(url: &Url) -> Option<HeaderValue> {
    HeaderValue::from_str(url.as_str()).ok()
}

#[cfg(test)]
mod tests {

    #[test]
    fn tls_config_sets_alpn_and_static_roots() {
        let config = super::tls_config();
        assert!(config.alpn_protocols.contains(&b"h2".to_vec()));
        assert!(config.alpn_protocols.contains(&b"http/1.1".to_vec()));
    }
    use super::*;

    #[test]
    fn challenge_detection() {
        assert!(is_challenge("Just a moment...</title>"));
        assert!(is_challenge(
            "<script src=\"/cdn-cgi/challenge-platform/...\">"
        ));
        assert!(is_challenge("enable JavaScript and cookies to continue"));
        assert!(!is_challenge("<html><body>anime biasa</body></html>"));
    }

    #[test]
    fn retry_after_is_capped() {
        let mut headers = HeaderMap::new();
        headers.insert(RETRY_AFTER, HeaderValue::from_static("5"));
        assert_eq!(parse_retry_after(&headers), Some(Duration::from_secs(5)));

        headers.insert(RETRY_AFTER, HeaderValue::from_static("120"));
        assert_eq!(
            parse_retry_after(&headers),
            Some(MAX_RETRY_AFTER),
            "Retry-After besar harus dipotong"
        );

        headers.insert(RETRY_AFTER, HeaderValue::from_static("bukan-angka"));
        assert_eq!(parse_retry_after(&headers), None);
    }

    #[test]
    fn backoff_grows_exponentially() {
        assert_eq!(backoff(0), Duration::from_millis(700));
        assert_eq!(backoff(1), Duration::from_millis(1400));
        assert_eq!(backoff(3), Duration::from_millis(5600));
    }

    #[test]
    fn mirror_set_rejects_invalid_base_url() {
        let err = MirrorSet::new("bukan url", &[], "marker", 60).unwrap_err();
        assert!(err.to_string().contains("base_url"));
    }

    #[test]
    fn mirror_set_dedupes_candidates_and_keeps_order() {
        let mirrors = vec![
            "https://b.test/".to_string(),
            "https://a.test/".to_string(),
            "https://a.test/".to_string(),
            String::new(),
            "bukan url".to_string(),
        ];
        let set = MirrorSet::new("https://a.test/", &mirrors, "marker", 60).unwrap();
        assert_eq!(
            set.candidates(),
            &[
                Url::parse("https://a.test/").unwrap(),
                Url::parse("https://b.test/").unwrap()
            ]
        );
    }

    #[test]
    fn cached_mirror_expires() {
        let set = MirrorSet::new("https://a.test/", &[], "marker", 1).unwrap();
        {
            let mut guard = set.cache.lock().unwrap();
            *guard = Some(CachedMirror {
                url: Url::parse("https://a.test/").unwrap(),
                at: Instant::now() - Duration::from_secs(5),
            });
        }
        assert!(set.cached().is_none(), "cache harus kedaluwarsa");
    }

    #[test]
    fn referer_header_roundtrip() {
        let url = Url::parse("https://example.test/ep/1").unwrap();
        let header = referer_header(&url).unwrap();
        assert_eq!(header.to_str().unwrap(), url.as_str());
    }
}
