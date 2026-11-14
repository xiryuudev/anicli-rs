# DESIGN.md — `anicli-rs`

**Status:** Draf v0.1 — Deliverable #1 (menunggu review)
**Tanggal:** 2026-11-14
**Pemilik:** Farrel Zacky Rahmanda (`github.com/xiryuudev`)
**Lisensi:** MIT (lihat §19)

Dokumen ini adalah kontrak desain. Semua deliverable berikutnya (#2–#10) harus konsisten
dengan isi dokumen ini; kalau ada perubahan desain saat implementasi, dokumen ini yang
diubah dulu, baru kode.

---

## 1. Ringkasan

`anicli-rs` adalah aplikasi **TUI Rust** untuk mencari, memilih episode, memilih
resolusi, dan memutar anime subtitle Indonesia lewat player eksternal (`mpv`, fallback
`vlc`). Ini adalah penulisan ulang `anicli-indo` (Python, deprecated) dengan
arsitektur modular: setiap situs sumber adalah adapter terpisah di balik satu trait,
sehingga source bisa ditambah/dimatikan/diurutkan tanpa menyentuh UI.

Prinsip yang dipegang:

1. **Adapter tipis, inti pintar.** Source hanya tahu cara mem-_parse_; logika fallback,
   pemilihan resolusi, retry, dan error UX ada di inti.
2. **Parser = fungsi murni.** `fn(&str) -> Result<T, ParseError>` terpisah dari HTTP,
   sehingga bisa dites offline dengan fixture HTML.
3. **Tidak ada HTTP di thread UI.** Semua I/O berjalan di task `tokio`, hasilnya masuk
   ke UI lewat channel.
4. **Gagal dengan informasi, bukan panic.** Semua jalur error punya tipe, pesan, dan
   saran aksi.

---

## 2. Keputusan desain (decision log)

Keputusan dari sesi klarifikasi 2026-11-14:

| # | Topik | Keputusan | Alasan |
|---|-------|-----------|--------|
| D1 | Nama proyek | crate `anicli-rs`, binary **`anicli`** | Konsisten dengan `anicli-indo`, nama binary pendek |
| D2 | Player | **mpv** default, **vlc** fallback | mpv sudah ada di mesin target (`/usr/bin/mpv` v0.41) |
| D3 | Download offline | **Tidak di v1**; hook `DownloadBackend` disiapkan | Fokus TUI + streaming dulu |
| D4 | Source prioritas | **Samehadaku, Otakudesu, Kusonime** lebih dulu | Struktur relatif stabil; sisanya menyusul |
| D5 | Format config | **TOML** | Bisa dikomentari, nested, serde/toml matang |
| D6 | Image preview | **Tidak di v1**, disiapkan di balik cargo feature `image-preview` | TUI tetap ringan & kompatibel semua terminal |
| D7 | Toolchain | **Rust 1.88+, edition 2021**, Windows `x86_64-pc-windows-msvc` | ⚠️ lihat catatan di bawah |

### ⚠️ Penyimpangan dari jawaban klarifikasi (butuh persetujuanmu)

Waktu klarifikasi kamu memilih *"Rust 1.85+ / edition 2021"*. Setelah verifikasi
metadata crates.io (2026-11-14):

- `ratatui 0.30.2` → `rust-version = 1.88.0`
- `ratatui 0.29.0` → `rust-version = 1.74.0`

Jadi **MSRV 1.88** kalau kita mau `ratatui 0.30` (versi stabil terbaru, API & docs
terkini). Dua opsi:

- **Opsi A (rekomendasi):** MSRV **1.88**, `ratatui 0.30` + `crossterm 0.29`. Rentang
  toolchain-mu sekarang (rustc 1.99.1) jauh di atas ini, jadi tidak ada biaya nyata.
- **Opsi B:** tetap MSRV 1.85 dengan `ratatui 0.29` + `crossterm 0.28` (pin versi lama,
  harus hati-hati saat baca docs).

Semua sisa dokumen memakai **Opsi A**. Kalau kamu mau B, bilang di review dan saya
ubah tabel dependensi + contoh kode.

### Keputusan teknis lain yang saya ambil (bisa dibantah saat review)

| # | Keputusan | Alasan |
|---|-----------|--------|
| D8 | Async trait pakai `async-trait` (`#[async_trait]`), bukan RPITIT | Trait dipakai sebagai `dyn AnimeSource` di registry; `async-trait` masih paling sederhana & kompatibel |
| D9 | Source memakai `Arc<dyn AnimeSource>` dengan `&self` (bukan `&mut self`) | State internal pakai `RwLock`/`AtomicU64`; memungkinkan request paralel lintas source |
| D10 | Fallback lintas-source memakai **re-resolusi berbasis identitas** (judul + nomor episode), bukan reuse URL | URL tiap source berbeda; ini inti desain §10 |
| D11 | `reqwest 0.13` dengan fitur TLS `rustls` (**bukan** `rustls-tls` lagi), plus `webpki-roots` | Fitur `rustls-tls` sudah tidak ada di 0.13. **Koreksi setelah build nyata:** `rustls` di reqwest 0.13 = provider **aws-lc-rs** + `rustls-platform-verifier`, jadi bukan lagi "pure Rust": `aws-lc-sys` meng-compile kode C saat build pertama (butuh `cmake`; di Windows MSVC butuh cmake + NASM, atau feature `prebuilt-nasm`). Alternatif bila itu mengganggu: `rustls-no-provider` + ring, atau `default-tls` (SChannel di Windows). Untuk v1 kita pakai `rustls` dan mendokumentasikan prasyarat build di README |
| D12 | Domain/mirror source **tidak** di-hardcode; disimpan di config + auto-probe | Domain situs anime sering berpindah (§8) |
| D13 | Parser & HTTP dipisah per modul (`parse.rs` vs `client.rs`) | Bisa tes parser dengan fixture, tanpa jaringan |
| D14 | Player dijalankan **detached** (`--no-terminal`) supaya TUI tidak freeze; mode foreground opsional | UX: TUI tetap hidup, progress tetap tercatat |
| D15 | Ada mode `--mock` (fixture) untuk demo & CI | Situs bisa down/Cloudflare; demo & tes tetap jalan |
| D16 | `crossterm` diakses lewat re-export `ratatui::crossterm`, **bukan** dependency terpisah (kecuali untuk fitur `event-stream`) | Terverifikasi: `ratatui 0.30` mengekspor `pub use ratatui_crossterm::crossterm` dan default-nya sudah `crossterm 0.29`. Menambah `crossterm = "0.29"` hanya untuk fitur `event-stream`; Cargo menyatukan fitur pada versi yang sama (tidak dobel) |
| D17 | Pakai helper `ratatui::init()` / `ratatui::restore()` (bukan setup raw-mode manual) | Helper resmi menangani alternate screen + raw mode + panic hook, jadi TUI tidak merusak terminal saat error |
| D18 | Crate dipecah: **library `anicli_rs`** + binary tipis `anicli` (+ nanti `sourcecheck`) | Semua logika bisa dites tanpa proses TUI; integration test menjalankan binary lewat `CARGO_BIN_EXE_anicli`; binary bantu memakai library yang sama |

### Verifikasi dependensi (sudah dilakukan, bukan asumsi)

Per 2026-11-14 seluruh set dependensi Tahap 2 **sudah dicoba di-build** di mesin ini
(rustc 1.99.1, Arch Linux) dengan `cargo build` dan berhasil. Versi yang ter-resolve:

`ratatui 0.30.2`, `ratatui-crossterm 0.1.2`, `crossterm 0.29.0`, `reqwest 0.13.1`,
`tokio 1.54.1`, `scraper 0.29.0`, `toml 1.2.1`, `clap 4.6.7`, `directories 6.0.0`,
`tracing-subscriber 0.3.23`.

Tiga temuan penting yang sudah dikoreksi di dokumen ini:

1. `reqwest 0.13` **tidak lagi punya fitur `rustls-tls`**; namanya sekarang `rustls`
   (+ `webpki-roots` / `rustls-native-certs`, `__rustls-aws-lc-rs` untuk provider).
   Fitur `default-tls`/`native-tls` masih ada tapi tidak kita pakai.
2. `ratatui 0.30` memecah backend ke crate terpisah (`ratatui-crossterm`), sehingga
   `CrosstermBackend` dan `crossterm` diimpor dari `ratatui`, bukan dari crate crossterm
   langsung — menghindari dua versi crossterm di graph.
3. `rustls` di reqwest 0.13 menarik **aws-lc-rs** (bukan ring): `aws-lc-sys` mem-build
   kode C, sehingga build pertama jauh lebih lama dan Windows butuh MSVC Build Tools +
   cmake. Ini sudah dicatat sebagai prasyarat (lihat D11 dan README Tahap 10).

Status gate Tahap 2 (2026-11-14): `cargo check` **hijau** (exit 0), `cargo check
--features image-preview` hijau, `cargo fmt --check` bersih, `cargo build` sukses, dan
binary `target/debug/anicli` berjalan (exit 0). 515 paket ter-lock,
`resolver = "3"` sehingga pemilihan versi sadar-MSRV. Build pertama memakan puluhan
menit karena kompilasi C `aws-lc-sys`; build incremental berikutnya normal (±2–3 menit).
`ldd` binary hanya `libc`/`libgcc` — tidak ada dependensi OpenSSL saat runtime.

---

## 3. Scope

### 3.1 Masuk v1.0

- Pencarian paralel ke semua source aktif, hasil digabung + dedupe.
- Browse: ongoing & completed (+genres bila source mendukung).
- Detail anime: sinopsis, genre, status, tahun, skor, daftar episode.
- Pemilihan episode, pemilihan resolusi (240p–1080p), preferensi default resolusi.
- Ganti source default + urutan fallback; fallback otomatis saat source gagal.
- Putar ke `mpv` (default) / `vlc` dengan `Referer` & `User-Agent` yang benar.
  Jalur langsung yang terbukti di v1: Pixeldrain (Samehadaku) dan desustream →
  googlevideo (Otakudesu). Embed yang tidak bisa di-resolve memakai
  `yt-dlp`/browser sesuai `player.embed_strategy` (§12.6) — batasan ini nyata dan
  didokumentasikan, bukan disembunyikan.
- Riwayat tonton (posisi terakhir per anime) + bookmark.
- Config TOML persisten, logging ke file, TUI responsif.

### 3.2 Tidak masuk v1.0 (sengaja ditunda)

- Download offline (hook disiapkan; lihat §12.4).
- Image/poster preview di terminal (di balik feature flag; §13.8).
- Bypass Cloudflare/anti-bot aktif (batasan etis & teknis; lihat §19).
- Sinkronisasi akun, notifikasi jadwal rilis, subtitle download terpisah.
- Source situs yang butuh login/berlangganan.

---

## 4. Arsitektur keseluruhan

```
┌──────────────────────────────────────────────────────────────────┐
│                          main.rs (clap)                          │
│  --config, --source, --resolution, --player, --mock, --log-level │
└───────────────────────────────┬──────────────────────────────────┘
                                │
        ┌───────────────────────▼───────────────────────┐
        │              ui::app::App (state)             │
        │  screens + nav stack + job registry + keymap  │
        └──────┬────────────────────────────▲───────────┘
               │ AppCommand (mpsc)          │ AppEvent (mpsc)
               │                            │
        ┌──────▼────────────────────────────┴───────────┐
        │            services::PlaybackService          │
        │   cari → detail → episode → streams (async)    │
        │   fallback orkestrasi + pemilihan resolusi     │
        └──────┬────────────────────────────┬───────────┘
               │                            │
     ┌─────────▼─────────┐        ┌─────────▼──────────┐
     │ sources::Registry │        │  player::Player    │
     │ Arc<dyn AnimeSource>│      │  mpv / vlc spawn   │
     └─────────┬─────────┘        └────────────────────┘
               │
   ┌───────────┴──────────────────────────────────────┐
   │ per source: client.rs (HTTP+mirror) + parse.rs   │
   └──────────────────────────────────────────────────┘
```

Lapisan dan tanggung jawab:

| Lapisan | Modul | Boleh tahu apa | Tidak boleh |
|---------|-------|----------------|-------------|
| Domain | `model.rs` | Tipe data murni | I/O, UI, HTTP |
| Adapter | `sources/*` | HTML situs + HTTP | UI, config global, player |
| Layanan | `fallback.rs`, `resolve.rs` | Trait source, config | Render TUI |
| Presentasi | `ui/*` | State + channel | HTTP langsung, `await` request |
| Infra | `http.rs`, `player.rs`, `logging.rs`, `paths.rs`, `config.rs` | OS, proses, file | Logika bisnis anime |

Alur data satu request (contoh: user menekan Enter di hasil pencarian):

```
UI thread ──AppCommand::OpenDetail{source,url}──▶ Job runner (tokio::spawn)
                                                       │
                        job = service.detail_and_episodes(source, url, timeout)
                                                       │
                          ┌────────────────────────────┴──────────────┐
                          │  ok  → AppEvent::DetailLoaded{..}         │
                          │  err → AppEvent::JobFailed{..} (terklasifikasi)
                          └────────────────────────────┬──────────────┘
                                                       ▼
UI loop: select! { keyboard, app_event, tick } → update state → redraw
```

---

## 5. Model domain (`src/model.rs`)

Tipe inti; semua `Serialize`/`Deserialize` supaya gampang di-log & diuji.

```rust
pub struct SourceId(pub &'static str);          // "samehadaku", "otakudesu", ...
pub struct Resolution(pub u16);                 // tinggi: 240, 360, 480, 720, 1080
                                                // Ord by height; Display "720p"

pub struct AnimeSearchResult {
    pub source: SourceId,
    pub title: String,
    pub url: Url,
    pub thumbnail: Option<Url>,
    pub kind: Option<MediaKind>,                // Tv | Movie | Ova | Special | Unknown
    pub status: Option<AiringStatus>,           // Ongoing | Completed | Unknown
    pub score: Option<f32>,
}

pub struct AnimeDetail {
    pub source: SourceId,
    pub url: Url,
    pub title: String,
    pub alt_titles: Vec<String>,                // judul Jepang/English untuk matching fallback
    pub synopsis: String,
    pub genres: Vec<String>,
    /// Tipe media (TV/Movie/OVA) bila situs mencantumkannya (ditambahkan Tahap 5).
    pub kind: Option<MediaKind>,
    pub status: AiringStatus,
    pub year: Option<u16>,
    pub score: Option<f32>,
    pub total_episodes: Option<u32>,
    pub poster: Option<Url>,
    pub episodes: Vec<Episode>,                 // sudah terurut
}

pub struct Episode {
    pub number: Option<u32>,                    // 12, 12.5 (OVA) → None kalau "Batch"
    pub label: String,                          // "Ep 12", "Batch", "Movie"
    pub title: Option<String>,
    pub url: Url,
    pub released: Option<chrono::NaiveDate>,
}

pub struct StreamLink {
    pub source: SourceId,
    pub resolution: Option<Resolution>,
    pub label: String,                          // "Mp4 720p (ZippyShare)"
    pub server: ServerKind,                     // Mirror | Zippy | Pixeldrain | Gofile | Mega | Embed | Other(String)
    pub container: Option<Container>,           // mp4 | mkv | unknown
    pub url: Url,
    pub headers: RequestHeaders,                // referer + user-agent wajib untuk player
    pub stream_kind: StreamKind,                // Direct | Embed | DownloadOnly
}

pub struct RequestHeaders { pub referer: Option<Url>, pub user_agent: Option<String> }

pub struct PlaybackTarget {                     // identitas lintas-source (D10)
    pub anime_title: String,
    pub alt_titles: Vec<String>,
    pub episode_number: Option<u32>,
    pub episode_label: String,
    pub origin: SourceId,
    pub anime_url: Url,
    pub episode_url: Url,
}
```

Aturan: `StreamLink.headers` **selalu** diisi layer source (bukan ditebak player),
karena banyak host menolak tanpa `Referer`.

---

## 6. Trait `AnimeSource`

```rust
// src/sources/mod.rs
#[async_trait]
pub trait AnimeSource: Send + Sync {
    /// Nama stabil untuk config, log, dan UI ("samehadaku").
    fn name(&self) -> &'static str;

    /// Fitur yang didukung source ini; UI menyembunyikan tombol yang tidak ada.
    fn capabilities(&self) -> Capabilities;

    async fn search(&self, query: &str) -> Result<Vec<AnimeSearchResult>, SourceError>;
    async fn ongoing(&self) -> Result<Vec<AnimeSearchResult>, SourceError>;
    async fn completed(&self) -> Result<Vec<AnimeSearchResult>, SourceError>;
    async fn detail(&self, url: &str) -> Result<AnimeDetail, SourceError>;
    async fn episodes(&self, anime_url: &str) -> Result<Vec<Episode>, SourceError>;
    async fn streams(&self, episode_url: &str) -> Result<Vec<StreamLink>, SourceError>;
}
```

```rust
#[derive(Clone, Copy, Debug, Default)]
pub struct Capabilities {
    pub search: bool,
    pub ongoing: bool,
    pub completed: bool,
    pub genres: bool,
    pub detail: bool,
    pub episodes: bool,
    pub streams: bool,        // false = download-only (mis. situs batch)
    pub batch_download: bool, // hanya untuk hook download masa depan
}
```

Keputusan penting:

- **`Capabilities` adalah kontrak runtime.** Kalau `streams == false`, orchestrator
  langsung melewati source itu di jalur stream (bukan menunggu error), dan UI memberi
  label "download-only".
- Method yang tidak didukung **wajib** mengembalikan `SourceError::Unsupported` —
  bukan `Ok(vec![])`, supaya tidak menyamar sebagai "hasil kosong" (beda UX-nya besar:
  "tidak ada hasil" vs "source tidak menyediakan fitur ini").
- Trait `Send + Sync` + `Arc<dyn AnimeSource>`: satu instance per source per proses,
  dapat dipanggil paralel (D9). Semua state mutable internal pakai `Mutex`/`Atomic*`.
- Semua method **tidak menerima timeout**; timeout & cancellation diatur layer atas
  (`tokio::time::timeout` + `AbortHandle`) supaya adapter tetap sederhana.

Alternatif yang ditolak: trait dengan `fn browse(&self, kind) -> Result<Page>` (lebih
kecil, tapi keluar dari spesifikasi yang kamu minta); trait generic per-source (tidak
bisa disimpan dalam satu `Vec` heterogen).

**Implementasi Tahap 4:** [`factories()`] sengaja masih kosong. `SourceRegistry::build`
menerima daftar factory (bisa disuntik di test), dan source yang `enabled = true` tetapi
tidak punya factory dilaporkan sebagai `missing` + warning (bukan error fatal) — supaya
config yang menyebut source masa depan tetap valid saat binary belum
mengimplementasikannya.

### 6.1 Struktur internal tiap source

```
src/sources/samehadaku/
├── mod.rs       # impl AnimeSource: orkestrasi (base_url → request → parse)
├── client.rs    # URL builder + endpoint khusus situs (ajax/search/pagination)
└── parse.rs     # fungsi murni: parse_home, parse_search, parse_detail, ...
```

`parse.rs` **tidak boleh** memanggil jaringan dan **tidak boleh** `unwrap()`. Helper
defensif bersama (`src/sources/util.rs`):

```rust
pub fn sel_text(doc: &Html, css: &str) -> Option<String>;   // trim + collapse whitespace
pub fn sel_all(doc: &Html, css: &str) -> Vec<ElementRef>;
pub fn attr(el: &ElementRef, name: &str) -> Option<String>;
pub fn abs_url(base: &Url, href: &str) -> Option<Url>;
pub fn parse_resolution(label: &str) -> Option<Resolution>; // "720p"/"Mp4 720p"/"HD"
pub fn parse_episode_number(label: &str) -> Option<u32>;    // "Ep 12", "Episode 12 Sub Indo"
```

Semua kegagalan selector → `ParseError { page, field, hint }`, bukan panic.

---

## 7. Registry & lifecycle

```rust
pub struct SourceRegistry {
    sources: Vec<Arc<dyn AnimeSource>>,   // urutan = urutan config
    by_name: HashMap<&'static str, usize>,
}
impl SourceRegistry {
    pub fn from_config(cfg: &SourcesConfig) -> Result<Self>; // hanya enabled=true
    pub fn get(&self, name: &str) -> Option<Arc<dyn AnimeSource>>;
    pub fn names(&self) -> Vec<&'static str>;
    pub fn ordered(&self, order: &[String]) -> Vec<Arc<dyn AnimeSource>>;
}
```

Registry dibangun sekali saat startup. Source yang `enabled=false` tidak di-instansiasi
(tidak ada request sama sekali). Nama source tidak dikenal di config → warning log +
diabaikan (config lama tetap valid).

---

## 8. HTTP & layer mirror (`src/http.rs`)

Satu `reqwest::Client` dibagi seluruh source (connection pooling), dibungkus
`HttpClient` yang menegakkan kebijakan:

```rust
pub struct HttpClient {
    client: reqwest::Client,
    cfg: HttpConfig,                        // UA, timeout, retry, delay
    host_limiter: Mutex<HashMap<String, Arc<Semaphore>>>, // 1 in-flight per host
    last_hit: Mutex<HashMap<String, Instant>>,           // delay antar request/host
}
impl HttpClient {
    pub async fn get_html(&self, url: Url, referer: Option<Url>) -> Result<String, SourceError>;
    pub async fn get_json<T: DeserializeOwned>(&self, url: Url) -> Result<T, SourceError>;
    pub async fn probe(&self, url: Url) -> ProbeOutcome;  // HEAD/GET kecil untuk health
}
```

- TLS: `rustls` + `webpki-roots` (tanpa OpenSSL → cross-compile Windows bersih).
- Default UA: UA browser wajar (konfigurabel); `Accept-Language: id-ID,id;q=0.9,en;q=0.8`.
- `gzip` + `brotli` aktif; `http2` aktif.
- **Rate limit:** default `request_delay_ms = 500` + jitter 0–400 ms per host, dan
  semaphore per host = 1 (serial per domain), global `max_concurrent = 6`.
- **Retry:** hanya GET idempoten, maks 2 percobaan, backoff 700 ms + jitter, hanya
  untuk error jaringan/5xx/429 (429 menghormati `Retry-After`).
- **Timeout:** connect 8 s, total per request 20 s (konfigurabel).
- **Cookie jar** aktif per source (beberapa situs butuh cookie sesi untuk halaman
  episode), tidak dipakai untuk login.

### 8.1 Rotasi domain / mirror (`MirrorSet`)

Fakta lapangan (dicek 2026-11-14): domain situs anime sering berpindah dan banyak
domain mati/menjadi parkir (contoh: `samehadaku.email` sekarang redirect ke situs
travel; `otakudesu.cloud` → HTTP 523). Karena itu:

```rust
pub struct MirrorSet { pub candidates: Vec<Url> }
impl MirrorSet {
    /// Urut kandidat, probe sampai ada yang "sehat" (status 200 + marker situs),
    /// cache hasilnya 1 jam di memori (opsional persistent cache di data dir).
    pub async fn resolve(&self, http: &HttpClient, marker: &str) -> Option<Url>;
}
```

Aturan:

1. Config boleh menaruh `base_url` + `mirrors = [...]`.
2. Saat source pertama kali dipakai: probe kandidat berurutan (timeout 5 s), cek
   marker di HTML untuk menghindari domain parkir/clone — mis. Samehadaku: `title`
   mengandung "Samehadaku" **dan** ada `div.announcement-box`/daftar anime; Otakudesu:
   ada `div.venser` + `themes/Animestream`, dan teks `.infoupdate` di home menyebut
   domain resmi.
   **Peringatan:** `samehadaku.care` membalas 200 tetapi isinya portal JS yang baru
   redirect ke v2 setelah hitungan 10 detik via `window.location.replace` (tanpa
   redirect HTTP) → status 200 saja tidak membuktikan apa-apa; marker wajib dicek.
3. Hasil sehat di-cache 1 jam; kalau request berikutnya gagal network/5xx, probe ulang
   sekali lalu retry request.
4. Semua kegagalan probe masuk log (source, URL, status, alasan).

### 8.2 Anti-bot / Cloudflare

Kebijakan jujur: **tidak** mengimplementasikan bypass challenge (turnstile/JS
challenge). Pengukuran 2026-11-14: Otakudesu berada di belakang Cloudflare tetapi
**hanya proxy, tanpa bot check** — plain curl tanpa UA pun dapat 200 + halaman lengkap
(40 KB, `content-encoding: zstd`, latency ~0,3 s, `robots.txt` allow-all). Jadi
`Blocked` hanya diklasifikasikan kalau benar-benar ada marker challenge (403 +
`cf-mitigated`, halaman "Just a moment…", `__cf_chl`). **Tapi hati-hati false
positive:** Samehadaku memakai Cloudflare **UA-gated** — UA kosong atau UA generik
(`curl/8.5`, `Mozilla/5.0`) dibalas 403 "Just a moment…", sedangkan UA Chrome lengkap
dapat 200 (tanpa JS challenge / Turnstile). Karena itu aturan retry: pada 403 pertama,
**retry sekali dengan UA browser lengkap** sebelum menyimpulkan `Blocked`. Kalau itu
muncul:

- `SourceError::Blocked { source, url }` → UI menampilkan "source memblokir permintaan
  otomatis", lalu **fallback ke source lain**.
- Opsi lanjutan (v1.1, opsional di config): `[http] cookie_header = "..."` hasil copy
  manual dari browser, supaya kamu bisa pakai sesi sendiri tanpa automation bypass.

Implementasi Tahap 4 (`src/http.rs`): deteksi challenge dari body (`Just a moment`,
`challenge-platform`, `__cf_chl`, `cf-mitigated`) → `SourceError::Blocked`; 403 tanpa
marker tetap `Http { status: 403 }`. Ada juga dua client (redirect & no-redirect)
untuk mendeteksi soft-404 (§9 poin 5) dan pengaman `Retry-After` maksimum 30 detik.

---

## 9. Parsing yang defensif

Aturan wajib untuk semua `parse.rs`:

1. **Tidak ada `unwrap`/`expect`/indexing mentah.** `Vec::get(i)` + `ok_or`.
2. Setiap field punya **fallback selector** (array kandidat) karena tema situs kadang
   berubah sebagian:

   ```rust
   const SEARCH_ITEM: &[&str] = &[".postbody ul li", "article.animpost", ".chivsrc li"];
   ```
3. Nilai opsional (skor, tanggal, jumlah episode) → `None`, bukan error. Hanya field
   kunci (judul, URL) yang boleh menggagalkan parse.
4. Teks dibersihkan: trim, collapse whitespace, decode entity (`html-escape`),
   buang prefix "Sub Indo", "Nonton", dsb. lewat `normalize_title()`. Perbandingan
   label/ekstensi **case-insensitive** (`720p` vs `720P`), dan label `.info` di-trim
   (bisa ada spasi ekor).
5. **Anti soft-404:** source yang mengalihkan URL tidak dikenal ke homepage (Kusonime)
   harus di-fetch dengan `redirect = none` **atau** hasilnya divalidasi dengan marker
   halaman (mis. `h1.jdlz`). Kalau tidak, "episode tidak ada" akan tampak seperti sukses
   berisi halaman lain — ini penyebab bug paling sulit dilacak.
6. Setiap parser mengembalikan `ParseError` dengan nama page + selector yang gagal,
   sehingga log langsung menunjuk titik rapuh.
7. Fixture HTML nyata disimpan di `tests/fixtures/<source>/`, dan ada test "kontrak
   selector": parse fixture → assert jumlah item & field kunci. Kalau situs berubah,
   refresh fixture → test merah → perbaiki parser (bukan sebaliknya).

---

## 10. Orkestrasi fallback (`src/fallback.rs`)

### 10.1 Masalah inti: URL tidak portabel

URL anime di Samehadaku berbeda dengan Otakudesu. Jadi fallback tidak bisa sekadar
"coba URL yang sama di source lain". Yang portabel adalah **identitas**:
`(judul anime, nomor episode)`.

### 10.2 Algoritma

```
resolve_streams(target: PlaybackTarget, cfg) -> ResolvedStreams | FallbackError

urutan = [target.origin] ++ cfg.fallback_order (tanpa duplikat, hanya enabled & streams)
budget_total = cfg.timeouts.playback_total_secs   // default 90 s

for source in urutan:
    if !source.capabilities().streams: catat Skip(Unsupported); continue
    hasil = tokio::time::timeout(per_attempt, coba_source(source, target))
    match hasil:
      Ok(Ok(links)) if !links.is_empty() -> return ResolvedStreams { links, source_used, attempts }
      Ok(Ok(_))            -> catat Empty
      Ok(Err(e))           -> catat Failed(e)          // lanjut
      Err(_)               -> catat Timeout             // lanjut
    // UI tetap responsif: progress per source dikirim sebagai AppEvent

return FallbackError { attempts }   // UI tampilkan rantai percobaan + tombol retry/ganti source

coba_source(source, target):
    if source == target.origin && target.episode_url masih valid:
        links = source.streams(target.episode_url)        // jalur cepat
        if ok -> return
    // jalur re-resolusi (source berbeda / URL basi)
    hasil = source.search(target.anime_title)             // + alt_titles
    anime = pick_best_match(hasil, &target)               // §10.3
    detail = source.detail(anime.url)
    ep = match_episode(&detail.episodes, target.episode_number, target.episode_label)
    source.streams(ep.url)
```

### 10.3 Pencocokan judul & episode

`pick_best_match` skor berjenjang (ambil tertinggi; tolak kalau < ambang):

1. Normalisasi: lowercase, buang tahun `(2024)`, buang kata "sub indo", "nonton",
   "batch", tanda baca → token set.
2. Skor: `1.0` jika token set identik; `0.85` jika salah satu subset penuh; selain itu
   Jaccard token; bonus `+0.05` kalau tahun sama; `+0.05` kalau tipe media sama.
3. Ambang terima `0.72`; kalau tidak ada yang lewat → `NotFound` (jangan asal pilih
   anime yang salah — ini lebih berbahaya daripada gagal).

`match_episode`: cocokkan `number` dulu; kalau `None`/batch, cocokkan label
("Batch", "Movie", "Ep 12.5"). Kalau nomor tidak ada di source tujuan → `NotFound`
dengan pesan "episode 12 tidak tersedia di source X (tersedia 1–10)".

### 10.4 Anggaran waktu & pembatalan

- Per source: `per_attempt_secs = 25` (search+detail+episodes+streams dalam satu source
  bisa 3–4 request; delay 500 ms tetap dihormati).
- Total: 90 s; kalau habis → `FallbackError::BudgetExhausted` + attempts.
- Job punya `AbortHandle`; user pindah layar / tekan Esc → job dibatalkan, tidak ada
  request zombie. State UI: `Loading { source, step }` sehingga terlihat "mencoba
  Otakudesu (3/4)…".

### 10.5 Jejak percobaan (untuk UI & log)

```rust
pub struct Attempt { pub source: SourceId, pub outcome: AttemptOutcome, pub ms: u64 }
pub enum AttemptOutcome { Success, Empty, Unsupported, Failed(SourceError), Timeout, Skipped }
pub struct FallbackError { pub attempts: Vec<Attempt>, pub last: Option<SourceError> }
```

UI menampilkan ringkas: `samehadaku ✗ 403 · otakudesu ✗ timeout · kusonime ✗ unsupported`.
Semua attempt juga ditulis ke log (`tracing::warn!`).

### 10.6 Fan-out pencarian paralel (bukan fallback)

Pencarian **berbeda** dari fallback: semua source aktif dicoba **bersamaan** dan
hasilnya digabung, karena user butuh pilihan seluas mungkin.

```rust
pub struct SearchGroup {                    // satu baris di UI
    pub title: String,
    pub normalized: String,                 // untuk dedupe
    pub kind: Option<MediaKind>,
    pub year: Option<u16>,
    pub score: Option<f32>,
    pub hits: Vec<AnimeSearchResult>,       // dari source berbeda
}
pub struct SearchOutcome {
    pub groups: Vec<SearchGroup>,
    pub errors: Vec<(SourceId, SourceError)>,   // ditampilkan sebagai chip/peringatan
}
```

Aturan:

1. `JoinSet` satu task per source enabled (`capabilities().search`), **stagger 300–500 ms**
   antar spawn supaya tidak menembak banyak domain sekaligus, timeout 20 s/source.
2. Hasil dikirim **progresif**: UI menampilkan hasil yang sudah datang + indikator
   "mencari di otakudesu… (2/4)". User tidak menunggu source terlambat.
3. **Dedupe**: normalisasi judul (§10.3), kunci `(normalized_title, year, kind)`; item
   yang sama digabung jadi satu `SearchGroup` dengan beberapa `hits` (badge source di
   kanan). Judul dengan tahun berbeda **tidak** digabung.
4. Kalau source gagal, pencarian tetap sukses selama minimal satu source mengembalikan
   hasil; error ditampilkan non-blocking. Semua gagal → layar error dengan retry.
5. Query baru / user keluar layar → semua task dibatalkan (`JoinSet::abort_all`) supaya
   tidak ada request basi yang menghabiskan kuota/rate limit.

Browse (`ongoing`/`completed`) memakai pola yang sama tetapi **tanpa** dedupe lintas
source secara default (user memilih sumber lewat source picker), kecuali config
`ui.merge_browse = true`.

---

## 11. Pemilihan resolusi (`src/resolve.rs`)

```rust
pub fn select_stream<'a>(links: &'a [StreamLink], pref: Resolution, policy: PreferPolicy)
    -> Option<&'a StreamLink>;
```

Peringkat (kecil = lebih dipilih):

| Kondisi | Rank |
|---------|------|
| `res == pref` | `(0, 0)` |
| `res > pref` (terdekat di atas) | `(1, res - pref)` |
| `res < pref` (terdekat di bawah) | `(2, pref - res)` |
| resolusi tidak diketahui | `(3, u16::MAX)` |

Default policy = `nearest_higher` (sesuai spesifikasi: lebih tinggi dulu, lalu lebih
rendah). Tie-break tambahan, berurutan: `Container` (mkv diprefer kalau config
`prefer_mkv`), `StreamKind` (Direct > Embed), lalu urutan kemunculan di halaman.

Untuk sumber batch/download-only, `StreamKind::DownloadOnly` tidak pernah dipilih
otomatis oleh pemutar; diekspos di UI sebagai badge "hanya download" (v1.1).

Implementasi Tahap 7 (`src/resolve.rs`): `select_stream`/`ranked_streams`/`available_resolutions`
memakai kunci `(Direct>Embed, kelas resolusi, prefer_mkv, delta)`; UI menampilkan pilihan
default dan tombol `r` meng-cycle resolusi yang tersedia.

UI: setelah daftar stream ada, tampil **picker** berisi semua resolusi (dengan tanda
✓ pada pilihan default). Tombol `p` langsung memutar pilihan default; `r` membuka
picker untuk override manual. Pilihan manual berlaku untuk sesi itu (opsional
"simpan sebagai default" → tulis config).

---

## 12. Player (`src/player.rs`)

### 12.1 Deteksi binary

Urutan resolusi: config `player.path` → `PATH` (`mpv`, `mpv.exe`) → lokasi umum
(`/usr/bin/mpv`, `/snap/bin/mpv`, `C:\Program Files\mpv\mpv.exe`, scoop `~scoop\shims`).
Validasi dengan `mpv --version` (timeout 3 s). Kalau tidak ada → error startup yang
ramah, bukan panic: "mpv tidak ditemukan. Install: `sudo pacman -S mpv` / `winget install mpv`".

### 12.2 Perintah mpv

```
mpv --no-terminal --force-media-title="<Anime> - Ep <n> (<res>)" \
    --referrer="<referer>" --user-agent="<ua>" \
    --cache=yes --demuxer-max-bytes=64MiB \
    [--start=<detik resume>] [--hwdec=auto-safe] \
    "<stream-url>"
```

- `--no-terminal` → TUI tetap hidup (D14). Player ditutup → child reaped → UI update
  "selesai diputar" + simpan progres (lihat §12.3).
- HLS (`.m3u8`) diputar langsung oleh mpv lewat ffmpeg yang sudah ter-bundle di build
  standar — tidak ada dependensi runtime tambahan; `.m3u8` bisa `302` ke CDN, mpv
  mengikuti redirect.
- `--user-agent` **wajib** untuk media googlevideo (UA kosong → 403) dan aman dipakai
  untuk semua host; `--referrer` hanya diisi kalau source menentukan (banyak host
  menerima tanpa referer). Header dikirim dari `StreamLink.headers`, bukan hardcode.
- Mode `foreground = true` (config): TUI di-suspend (`LeaveAlternateScreen`,
  `disable_raw_mode`), mpv jalan di terminal, lalu TUI dipulihkan. Berguna kalau user
  mau kontrol keyboard mpv langsung.

### 12.3 Riwayat & resume

Saat player sukses di-spawn: catat `history { anime_title, episode, source, resolution,
url, played_at }`. Untuk resume presisi diperlukan progress waktu; v1 menyimpan
"episode terakhir" + timestamp, dan `--start` hanya diisi kalau user memilih "lanjutkan"
dari data yang ada (v1.1: baca posisi via IPC socket mpv `--input-ipc-server`).

### 12.4 Hook download (masa depan)

```rust
pub trait DownloadBackend { async fn download(&self, link: &StreamLink, dest: &Path) -> Result<()>; }
```

v1 hanya mendefinisikan trait + `Unsupported`, tidak ada implementasi. Dipakai nanti
untuk `ffmpeg -headers` / `aria2c --header` kalau kamu mau download offline.

### 12.5 VLC fallback

`vlc --play-and-exit --http-referrer=<referer> --http-user-agent=<ua> <url>`.
Catatan jujur: sebagian host menganalisis header lebih dalam dan VLC bisa gagal
walaupun mpv berhasil → pesan error menyebut "coba mpv" (§17).

### 12.6 Menangani link `Embed` (temuan recon, penting)

Sebagian source tidak memberi URL file langsung, melainkan iframe player pihak
ketiga. Recon 2026-11-14 menemukan bahwa **tidak semua embed butuh browser**: embed
`desustream` (dipakai Otakudesu) ternyata halaman statis 947 B berisi
`<video><source src="https://…googlevideo.com/videoplayback?…">`. Jadi kebijakan
dibagi tiga tingkat:

1. **Resolvable embed → `Direct`.** Kalau `embed.rs` punya aturan untuk host embed
   itu (v1: `desustream`), ambil URL media dari halaman embed, hasilkan
   `ResolvedMedia { url, headers, expires_in }`, lalu putar dengan mpv. Dua pola yang
   terverifikasi di halaman embed desustream:
   (a) `<video><source src="…">` (googlevideo, **UA wajib**, terikat IP), dan
   (b) `const videoURL = "…"` (CDN `cdn.odcloud.net/…mp4`, **tanpa UA/referer**).
   Uji akhir pada egress mesin ini: `#pembed` → embed `desustream.net` → `const
   videoURL` → `206 video/mp4` (143 MB) → **`mpv --frames=1` exit 0**. Resolver
   mencoba pola (a), lalu (b), lalu gagal dengan `ParseError` yang jelas.
2. **Unresolvable embed → `ytdl`/`browser`.** Contoh `mega.nz`/`odvidhide`.
   `player.embed_strategy = "ytdl"` (default bila `yt-dlp` ada) menjalankan
   `mpv --ytdl=yes <embed-url>`; `"browser"` membuka di browser; `"skip"` tidak
   menawarkan. Picker memberi badge `[embed]` dan tidak memilih ini sebagai default.
3. **`DownloadOnly`** (halaman host file-sharing yang butuh langkah JS, mis.
   Acefile/Gofile/Mediafire) tidak ditawarkan untuk play di v1.

Aturan cache (penting): URL media googlevideo **terikat IP egress dan kedaluwarsa
~6 jam**. Karena itu yang boleh di-cache hanyalah URL halaman embed + parameter mirror;
URL media di-resolve **tepat sebelum** memutar (cache singkat mis. 60 detik untuk
putar-ulang). Kalau mpv mengembalikan 403/410, resolve ulang sekali.

Konsekuensi pengujian: URL media **harus** dihasilkan oleh `HttpClient` aplikasi
(egress mesin user) — URL yang diambil lewat proxy/browser tool lain akan 403 karena
parameter `ip=` tidak cocok. Ini sudah dibuktikan saat recon: media URL dari fixture
(yang di-fetch lewat proxy alat riset, `ip=176.97.117.248`) ditolak `403` dari mesin
ini (egress `157.85.206.96`), meski UA browser dipakai. Jadi tes "putar nyata" harus
memakai jalur aplikasi sendiri, bukan fixture URL.

`embed.rs` (v1) hanya berisi aturan yang terbukti (desustream) + test dari fixture
`embed-desustream.html`; aturan host lain ditambah bertahap dengan fixture masing-masing.
Ini bukan bypass DRM — hanya membaca URL media yang memang dikirim ke browser.

Deteksi `yt-dlp`: `yt-dlp --version` (timeout 3 s). Kalau tidak ada, `embed_strategy`
otomatis turun ke `"browser"` + info sekali di UI.

### 12.7 Resolusi host file-sharing (`src/hosts.rs`, v1 minimal)

Banyak source hanya memberi link **halaman** host file-sharing, bukan file. Recon
mengukur hal berikut (2026-11-14):

| Host | Pola link | Bisa jadi URL putar langsung? |
|------|-----------|-------------------------------|
| Pixeldrain | `pixeldrain.com/u/<id>` | ⚠️ *kadang* bisa: `https://pixeldrain.com/api/file/<id>` → `206`, `Content-Type: video/matroska`; **diuji `mpv --frames=1` exit 0** pada file video Samehadaku. Tapi file lain (batch Kusonime) → `403 file_rate_limited_captcha_required` (hotlink hanya pelanggan) → perlakukan 403 sebagai `DownloadOnly`, jangan hardcode "pixeldrain selalu direct" |
| Acefile | `acefile.co/f/<id>/...` | ❌ halaman butuh JS/langkah unduh |
| Gofile | `gofile.io/d/<id>` | ❌ butuh API token/website token |
| Mediafire / Filedon | halaman file | ❌ butuh scrape langkah tambahan |
| Icefile (embed) | iframe `beta.icefile.my.id/embed/<token>` | ✅ `const SOURCES=[…]` → HLS **dan** MP4 1080p/720p/360p + original MKV; `m3u8` 302→CDN (tanpa UA/referer), `mp4` 206; mpv exit 0. ⚠️ segmen HLS dinamai `1.png`/`2.png` dengan `application/octet-stream` → jangan filter berdasarkan ekstensi |
| Blogspot (embed) | `blogger.com/video.g` | ⚠️ menghasilkan URL `googlevideo` `itag=18` yang IP-bound & kedaluwarsa → perlakukan sebagai media dinamis (§12.6); belum diuji sendiri, verifikasi di Tahap 8 |
| Odcloud CDN | hasil `const videoURL` di embed desustream | ✅ `cdn.odcloud.net/…mp4` (`206 video/mp4`, 143 MB; tanpa UA/referer, mpv exit 0) |
| Desustream (embed) | iframe `desustream.net/dstream/…?id=…` | ✅ halaman embed statis → dua pola: `<source src=googlevideo…>` (UA wajib, IP-bound) atau `const videoURL` = `cdn.odcloud.net/…mp4` (`206 video/mp4`, tanpa UA) |

`hosts.rs` = fungsi murni `fn resolve(link: &StreamLink) -> Option<StreamCandidate>`
dengan tabel aturan + test offline dari fixture. Aturan v1 hanya yang terbukti:
pixeldrain + URL yang sudah berakhiran `.mp4/.mkv/.m3u8`. Host lain tetap
`DownloadOnly` (ditandai jelas, tidak ditawarkan putar). Ini menjaga janji "mpv
bekerja" tanpa mengklaim lebih dari yang terbukti.

---

## 13. TUI (`src/ui/`)

### 13.1 Layar

| Layar | Isi | Aksi utama |
|-------|-----|------------|
| `Browse` | Tab: Ongoing / Completed / Genre, list hasil | `Enter` detail, `Tab` ganti tab, `s` source picker |
| `Search` | Input query + hasil (per source, dikelompokkan) | ketik, `Enter` cari, `Enter` buka detail |
| `Detail` | Sinopsis, genre, status, skor, daftar episode | `Enter` episode, `b` bookmark, `Esc` kembali |
| `Episodes` | Daftar episode + status riwayat | `Enter` pilih stream, `r` resume |
| `Streams` | Kandidat stream (source, resolusi, host) + jejak fallback | `p` play, `r` ganti resolusi |
| `Settings` | Form config (source, fallback order, resolusi, player, delay) | `Enter` simpan |
| `History` | Riwayat tonton | `Enter` lanjut, `d` hapus |
| `Bookmarks` | Favorit | `Enter` detail, `d` hapus |
| `Logs` | Ring buffer log in-app (debug) | scroll |
| `Help` | Keymap | `Esc` |
| `PlayerStatus` | Overlay "sedang diputar" + tombol stop | `x` stop |

### 13.2 State machine (transisi)

```
                 ┌──────────┐  Tab   ┌──────────┐
   startup ────▶ │  Browse  │◀──────▶│  Search  │
                 └────┬─────┘        └────┬─────┘
                      │ Enter             │ Enter
                      ▼                   ▼
                 ┌──────────────────────────┐
                 │         Detail           │◀── b (bookmark toggle)
                 └────┬─────────────────────┘
                      │ Enter / e
                      ▼
                 ┌──────────┐   Enter    ┌──────────┐   p    ┌──────────────┐
                 │ Episodes │───────────▶│ Streams  │───────▶│ PlayerStatus │
                 └──────────┘            └────┬─────┘        └──────┬───────┘
                      ▲                       │ r (picker)          │ child exit
                      └───────────────────────┴─────────────────────┘
   Esc selalu = pop nav stack. `q` = quit (dengan konfirmasi kalau ada job berjalan).
   `s` di layar mana pun membuka SourcePicker modal (ganti source aktif).
   `?` = Help modal.
```

### 13.3 Model event loop (tanpa freeze)

```rust
let (tx, mut rx) = mpsc::channel::<AppEvent>(256);
let mut events = crossterm::event::EventStream::new();
let mut ticker = tokio::time::interval(Duration::from_millis(100));

loop {
    terminal.draw(|f| render(f, &app))?;                 // sinkron, cepat (< 5 ms)
    tokio::select! {
        Some(Ok(ev))      = events.next() => app.on_input(ev, &tx),
        Some(msg)         = rx.recv()     => app.on_event(msg),
        _                 = ticker.tick() => app.on_tick(),
    }
    if app.should_quit { break; }
}
```

- `on_input` hanya mengubah state & **memicu** job (`tokio::spawn`) — tidak pernah
  `.await` HTTP.
- `EventStream` berasal dari `crossterm` (butuh fitur `event-stream`; lihat D16) dan
  tipe event-nya sama dengan `ratatui::crossterm::event`.
- `AppEvent` = `JobDone { id, result }`, `Progress { id, step }`, `PlayerExited { id }`,
  `ConfigSaved`. Job lama dibatalkan saat navigasi (`abort_handle.abort()`), dan hasil
  job basi diabaikan (`if id != app.current_job { return; }`).
- Render throttled: redraw hanya saat ada input/event/tick (tick 100 ms untuk spinner).

### 13.4 Keymap default (bisa diubah nanti)

```
q quit · ? help · Esc back · Tab next pane · j/k atau ↓/↑ navigasi ·
Enter buka/pilih · / search · s source · r resolusi · p play · b bookmark ·
h history · g settings · l logs · x stop player
```

### 13.5 State per layar

```rust
pub struct ListState { pub items: Vec<ListRow>, pub selected: usize, pub scroll: u16 }
pub struct Async<T> { pub state: LoadState, pub value: Option<T> }
pub enum LoadState { Idle, Loading { step: String }, Ready, Failed(Arc<SourceError>) }
```

### 13.6 Layout (ASCII, 80×24)

```
┌ anicli-rs ────────────────────────────────────── samehadaku ▾ ┐
│ Search  Browse  History  Bookmarks                             │
├────────────────────────────────────────────────────────────────┤
│ > Search: one piece                                  [3/4 src] │
│                                                                │
│ ▸ One Piece                              TV · Ongoing · 8.7    │
│   One Piece Film: Red                    Movie · Completed     │
│   ...                                                          │
├────────────────────────────────────────────────────────────────┤
│ Enter open · s source · ? help · q quit                        │
└────────────────────────────────────────────────────────────────┘
```

### 13.7 Error UX di TUI

- Banner kuning non-blocking untuk error ringan (source gagal, fallback jalan).
- Layar `Failed` untuk kegagalan total: pesan manusiawi + rantai attempt + pilihan
  `[R]etry` / `[S]ource lain` / `[Esc] Kembali`.
- Tidak pernah menutup TUI karena satu request gagal.

### 13.9 Implementasi Tahap 6 (TUI)

- `src/ui/app.rs` — state machine + spawn job + `AppEvent`; hasil job basi (nomor job
  berbeda) diabaikan sehingga user bisa pindah layar tanpa hasil nyasar.
- `src/ui/render.rs` — fungsi render murni untuk Search/Detail/Episodes/Streams/Help,
  spinner, error banner, status fallback. Dites lewat `TestBackend` (tanpa TTY).
- `src/ui/theme.rs` — warna/style terpusat.
- Layar Stream: `p`/`Enter` = pilih link terbaik lalu putar (via `player.rs`), `r` =
  cycle resolusi; status menampilkan source yang dipakai + jejak fallback.
- Saat stdin/stdout bukan terminal (pipe/CI/SSH), `anicli` mencetak ringkasan, bukan
  memaksa TUI.

### 13.8 Feature `image-preview` (di luar v1)

Kalau fitur cargo `image-preview` aktif: pakai `ratatui-image` (Kitty/iTerm2/Sixel
deteksi otomatis), decode poster `reqwest → image` di task terpisah, cache LRU di
memori. Default build: **off**, jadi dependency image tidak masuk binary normal.

---

## 14. Konfigurasi (`src/config.rs`)

### 14.1 Lokasi

| OS | Config | Data (history, cache, logs) |
|----|--------|----------------------------|
| Linux | `$XDG_CONFIG_HOME/anicli-rs/config.toml` (default `~/.config/anicli-rs/`) | `$XDG_DATA_HOME/anicli-rs/` (default `~/.local/share/anicli-rs/`) |
| Windows | `%APPDATA%\anicli-rs\config.toml` | `%LOCALAPPDATA%\anicli-rs\` |
| macOS | `~/Library/Application Support/anicli-rs/` | sama |

Implementasi: `directories` untuk root XDG/known-folder, lalu `paths.rs` menambahkan
subfolder `anicli-rs` secara eksplisit (untuk menjamin path Windows persis seperti
spesifikasi, karena `ProjectDirs` menyisipkan komponen org/qualifier).

### 14.2 Skema TOML (ringkas — file lengkap & berkomentar ada di `config.example.toml` / Deliverable #2)

```toml
config_version = 1

[general]
default_source = "samehadaku"
fallback_order = ["samehadaku", "otakudesu", "kusonime"]
language       = "id"

[playback]
player              = "mpv"          # mpv | vlc
default_resolution  = "720p"
resolution_policy   = "nearest_higher"  # nearest_higher | nearest_lower | exact_only
prefer_mkv          = false
resume_last_episode = true
embed_strategy      = "ytdl"        # ytdl | browser | skip  (§12.6)
foreground          = false         # true = mpv pakai terminal, TUI suspend

[http]
request_delay_ms     = 500
jitter_ms            = 300
max_concurrent       = 6
connect_timeout_secs = 8
timeout_secs         = 20
retries              = 2
mirror_cache_ttl_secs = 3600
user_agent           = "Mozilla/5.0 ..."

[sources.samehadaku]
enabled  = true
base_url = "https://samehadaku.care"
mirrors  = ["https://v2.samehadaku.how"]

[sources.otakudesu]
enabled  = true
base_url = "https://otakudesu.cam"

[sources.kusonime]
enabled  = true
base_url = "https://kusonime.com"

[storage]
history_enabled = true
max_history     = 500
max_bookmarks   = 200
data_dir        = ""       # kosong = default OS

[ui]
theme          = "default"
mouse          = false
merge_browse   = false
image_preview  = false     # hanya berefek kalau binary dibuild dengan feature image-preview

[logging]
level    = "info"          # trace|debug|info|warn|error
max_days = 7
```

### 14.3 Aturan config

- Precedence: **CLI flag > env (`ANICLI_*`) > file > default bawaan**.
- File hilang → dibuat otomatis dari template + komentar, lalu info di UI.
- `config_version` untuk migrasi; versi lebih baru dari yang didukung → error jelas.
- Validasi saat load: nama source dikenal, `fallback_order` ⊇ subset source enabled,
  resolusi valid, `player` dikenal. Error validasi menunjuk key TOML yang salah.
- `anicli config init [--force]` = tulis template (menolak menimpa tanpa `--force`);
  `anicli config path` = cetak path config/data/log/history/bookmark;
  `anicli config show` = cetak config efektif (setelah override CLI) sebagai TOML.
- File yang hilang **dibuat otomatis** dari template saat load; user tidak pernah
  disambut error "config tidak ada". File ditulis atomik (`config.toml.tmp` → rename)
  + mode `0600` di Unix (config bisa memuat cookie header).
- Validasi mengumpulkan **semua** error sekaligus, masing-masing menyebut key TOML
  (mis. `general.default_source`, `http.user_agent`), supaya sekali edit bisa beres.
- Field/`[sources.*]` yang tidak dikenal **diabaikan** (toleran versi), nilai yang
  salah tipe/enum tetap error. `config_version` lebih baru dari binary → error jelas.

### 14.4 CLI (Tahap 3)

```
anicli [OPTIONS] [COMMAND]

OPTIONS
  --config <FILE>      file config (env ANICLI_CONFIG)
  --data-dir <DIR>     direktori data (env ANICLI_DATA_DIR)
  --source <NAME>      override source sesi ini (env ANICLI_SOURCE)
  --resolution <RES>   override resolusi, mis. 1080p (env ANICLI_RESOLUTION)
  --player <mpv|vlc>   override player (env ANICLI_PLAYER)
  --log-level <LEVEL>  level/direktif log (env ANICLI_LOG)
  --no-tui             cetak ringkasan saja (aman untuk CI/pipe/SSH)
  --mock <DIR>         mode fixture, tanpa jaringan (Tahap 5+)

COMMAND
  config init [--force]    tulis config.toml dari template
  config path              cetak lokasi config/data/log
  config show              cetak config efektif (TOML)

EXIT CODE
  0 sukses · 1 error runtime (I/O, logging, player) · 2 config/CLI tidak valid
```

Presedensi nilai: **flag CLI > environment `ANICLI_*` > file config > default**.
Level log: `--log-level` > `RUST_LOG` > `logging.level` di config.

---

## 15. Persistensi (`src/history.rs`, `src/bookmarks.rs`)

Lokasi: `data_dir/history.json`, `data_dir/bookmarks.json` (keduanya sudah
diimplementasikan di `src/history.rs` / `src/bookmarks.rs`; mirror-cache menyusul).

Implementasi: tulis atomik (`tmp` → rename), muat toleran (file rusak → kosong +
warning log), history dedupe per `(judul, episode)` dan dipotong ke `max_history`;
bookmark dedupe per URL anime dan dipotong ke `max_bookmarks`; catatan riwayat dibuat
otomatis saat episode benar-benar diputar (`storage.history_enabled`).

```json
{
  "version": 1,
  "entries": [
    { "anime_title": "One Piece", "source": "samehadaku", "episode_number": 1085,
      "episode_label": "Ep 1085", "anime_url": "...", "episode_url": "...",
      "resolution": "720p", "played_at": "2026-11-14T20:30:00+07:00",
      "position_secs": null }
  ]
}
```

- Tulis atomik: `tmp` + `fs::rename`; `flock`-free (satu proses pengguna).
- Dedupe: satu entri per `(anime_title, episode_number)`; update timestamp.
- Batas `max_history = 500` entri (trim tertua).
- Bookmark menyimpan `{title, source, url, poster, added_at}`.

---

## 16. Logging & diagnosa (`src/logging.rs`)

- `tracing` + `tracing-subscriber`; layer file `tracing-appender` rolling harian ke
  `data_dir/logs/anicli-rs.YYYY-MM-DD.log`, timestamp **lokal** (`+07:00`).
- Ring buffer `LogBuffer` kapasitas 1000 baris dipakai layar `Logs`; field sensitif
  (`cookie`, `authorization`, `token`, `password`, `secret`, `api_key`) otomatis
  diredaksi menjadi `[REDACTED]`.
- File log lebih tua dari `logging.max_days` (default 7) dihapus saat startup
  (best-effort; kegagalan tidak pernah menggagalkan aplikasi).
- Saat TUI aktif **tidak** menulis ke stdout/stderr (merusak layar); log console hanya
  saat mode `--no-tui` atau level `trace` dengan `--log-to-stderr`.
- Ring buffer `VecDeque<LogLine>` (kapasitas 1000) dibagikan ke layar `Logs` via
  custom `Layer` → memudahkan debug tanpa keluar TUI.
- Format: `2026-11-14T20:31:02+07:00 WARN samehadaku: probe https://... status=523 step=mirror`.
- Redaksi: header `Cookie`/`Authorization` tidak pernah di-log.
- `RUST_LOG` tetap dihormati untuk `--no-tui`.

---

## 17. Model error & copy UX (`src/error.rs`)

```rust
pub enum SourceError {                       // implementasi nyata: src/error.rs
    Network { site: &'static str, url: String, detail: String },
    Timeout { site: &'static str, step: &'static str },
    Http { site: &'static str, url: String, status: u16 },
    Blocked { site: &'static str, url: String },       // challenge anti-bot
    Parse { site: &'static str, page: &'static str, field: &'static str, hint: String },
    NotFound { site: &'static str, what: String },
    Unsupported { site: &'static str, feature: &'static str },
    NoMirror { site: &'static str, tried: usize },     // semua domain gagal probe
    App(String),                                        // config/player, bukan source
}
impl SourceError {
    pub fn is_retryable(&self) -> bool;   // Network/Timeout/429/5xx saja
    pub fn is_source_fatal(&self) -> bool; // Unsupported/App: fallback sia-sia
    pub fn source_name(&self) -> Option<&'static str>;
    pub fn user_message(&self) -> String;  // bahasa Indonesia, actionable
}
```

Catatan: field sengaja bernama `site`, bukan `source`. `thiserror` memperlakukan field
bernama `source` sebagai error-source sehingga tipenya harus implement
`std::error::Error` — `&'static str` tidak. Aksesornya `source_name()`.

Contoh copy:

- HTTP 523 → "Otakudesu sedang down (HTTP 523). Mencoba source berikutnya…"
- Parse → "Struktur halaman Samehadaku berubah (field `episode_list`). Pakai source lain sementara; laporkan di GitHub."
- Unsupported → "Kusonime tidak menyediakan streaming langsung (hanya batch/download)."
- Player → "mpv tidak ditemukan di PATH. Install mpv atau set `playback.mpv_path` di config."

Kebijakan: **tidak ada `panic!` di jalur runtime data**. Satu-satunya `panic` yang
diizinkan adalah invariant internal di `main` sebelum TUI aktif (mis. terminal init gagal),
dan itu pun dibungkus pesan jelas.

---

## 18. Strategi pengujian & verifikasi

| Level | Isi | Alat |
|-------|-----|------|
| Config & CLI | template=default, validasi multi-error, migrasi versi, atomic save, override, exit code, auto-create, file log | `cargo test` + `cargo test --test cli_smoke` |
| HTTP client | server lokal: retry 5xx, `Retry-After`, challenge→`Blocked`, redirect vs no-redirect, 404 tidak di-retry, rate limit per host, JSON, `MirrorSet` marker+cache | `cargo test --test http_client` |
| Parser source (offline) | fixture HTML/JSON nyata: search, detail, episode, mirror (`data-content` base64), unduhan, embed (icefile `SOURCES`, desustream, blogspot), soft-404 | `cargo test --lib sources::` |
| Registry | dispatch `dyn AnimeSource`, urutan, filter `streams`, factory & `missing` | `cargo test --test registry` |
| Fallback & resolusi | fallback identitas (empty/error/unsupported/no-match), fan-out search + dedupe, ranking resolusi | `cargo test --lib fallback:: resolve::` |
| Player | argumen mpv/vlc, cari binary, plan spawn/browser/refuse | `cargo test --lib player::` |
| TUI | render 4 layar + help + error + keymap + job basi | `cargo test --test tui_render` |
| Unit parser | fixture HTML → assert field; 1 test per fungsi parse | `cargo test` (offline, cepat) |
| Kontrak selector | tiap fixture punya test jumlah item & field wajib | `cargo test` |
| Resolusi & matching | tabel kasus (exact/higher/lower/unknown; judul mirip/beda tahun) | `cargo test` |
| Fallback orkestrasi | source palsu (`MockSource`) yang error/timeout/empty → assert urutan attempt | `cargo test` |
| Host/embed resolver | fixture link host → assert URL putar/tidak (`pixeldrain`, file langsung, halaman JS) dan fixture `embed-desustream.html` → assert `<source src>` | `cargo test` |
| TUI smoke | render di `TestBackend` (ratatui) per layar & state error | `cargo test` |
| Live check | `anicli sourcecheck --source X --query "one piece"` (binary terpisah) | manual/CI opsional |
| End-to-end | `--mock` membaca fixture → Search→Play (player di-stub) | manual |
| Lint | `cargo fmt --check`, `cargo clippy -- -D warnings` | CI |
| Matrix build | Linux x86_64 + Windows `x86_64-pc-windows-msvc` | GitHub Actions |

Simulasi fallback (kriteria sukses #4) dilakukan di test: `MockSource` diurutkan
`[fail, timeout, ok]` → assert hasil dari source ketiga dan jejak attempt benar.
Simulasi nyata: `anicli --source samehadaku --no-tui --dry-run` sambil domain diblokir.

### 18.1 Mode `--mock`

`--mock <dir>` membaca fixture `tests/fixtures/` dan mengganti layer HTTP dengan
pembaca file, sehingga TUI bisa didemokan/CI-tes walau internet diblokir atau situs
mati. Ini juga alat debugging parser.

---

## 19. Legal, etika, dan batasan

- Proyek ini **alat pribadi untuk edukasi**; tidak mem-bypass DRM, tidak
  mendistribusikan konten, tidak menyimpan video (v1 tanpa download).
- Semua konten milik masing-masing penyedia; README memuat disclaimer eksplisit.
- Menghormati `robots.txt` (hasil cek per source ada di recon §22; Otakudesu
  allow-all; Kusonime allow-all dan Cloudflare-nya pasif; Samehadaku diverifikasi saat
  Tahap 5), delay antar request, dan tidak ada request agresif/paralel per host.
- Tidak ada login/akun berbayar, tidak ada API key atau kredensial di repo.
- Domain/mirror di config, bukan di kode, agar kamu bisa menyesuaikan sendiri.
- Kalau situs mengubah struktur atau memblokir, itu **diperlakukan sebagai kondisi
  normal** (error + fallback), bukan sesuatu yang harus diakali dengan bypass.

---

## 20. Risiko & mitigasi

| Risiko | Dampak | Mitigasi |
|--------|--------|----------|
| Domain pindah / mati | Semua request gagal | `MirrorSet` + probe + cache + config override |
| Cloudflare/bot check | 403/503 | Klasifikasi `Blocked`, fallback, opsi cookie manual (§8.2) |
| Selector berubah | ParseError | fallback selector, fixture test, pesan jelas |
| Host video butuh header | mpv 403 | `StreamLink.headers` wajib; referer dari halaman episode |
| `ratatui` versi baru butuh Rust > MSRV | build gagal | MSRV 1.88 dipatok + CI `--locked` |
| Crate `reqwest 0.13` API baru | compile error | sudah diverifikasi fitur `rustls`/`webpki-roots` (§2 D11) |
| Contention/rate limit | situs membalas 429 | serial per host + delay + backoff |
| Env build read-only (`CARGO_HOME` sandbox) | build lokal gagal | `CARGO_HOME` diarahkan ke dalam workspace saat build dev (§21.1) |

---

## 21. Roadmap bertahap (sesuai Deliverable)

Setiap tahap berhenti untuk **review kamu** sebelum lanjut.

| Tahap | Deliverable | Isi | Gate (lulus jika) |
|-------|-------------|-----|-------------------|
| 1 ✅ | `DESIGN.md` (dokumen ini) | arsitektur, trait, state machine, config, fallback, best practices | kamu setujui keputusan §2 |
| 2 ✅ | `Cargo.toml` + `config.example.toml` (+ `LICENSE`, `rust-toolchain.toml`, `assets/logo.txt`, stub `main.rs`) | dependensi terkunci (resolver 3, fitur `image-preview` opsional), contoh config berkomentar lengkap | `cargo check` hijau — **lulus** (515 paket ter-lock) |
| 3 ✅ | `lib.rs` + `main.rs` + `config.rs` + `paths.rs` + `logging.rs` + `model.rs` | CLI clap (`--config/--data-dir/--source/--resolution/--player/--log-level/--no-tui/--mock`, subcommand `config init|path|show`), load/validasi/migrasi/simpan atomik, logging file harian + ring buffer, path XDG/AppData | binary jalan; config dibuat otomatis lalu terbaca ulang; file log muncul — **lulus**: 41 test (30 unit + 11 CLI smoke), clippy `-D warnings` bersih, smoke: config auto-create, timestamp log lokal `+07:00`, error config exit code 2 |
| 4 ✅ | `sources/{mod,util,mock}.rs` + `model.rs` + `error.rs` + `http.rs` | trait `AnimeSource` + `Capabilities`, registry + factory (kosong sampai Tahap 5), model lengkap, `SourceError`, HTTP client (rate limit/retry/Retry-After), `MirrorSet` marker, util parsing defensif, `MockSource` | **lulus**: 100 test (69 unit + 11 CLI + 14 HTTP lokal + 6 registry), clippy `-D warnings` bersih; registry melaporkan 3 source enabled sebagai `missing` sampai Tahap 5 |
| 5 ✅ | Source #1–3 (Samehadaku, Otakudesu, Kusonime) + `bin/sourcecheck` | `{mod,client,parse}.rs` per source, fixture di `tests/fixtures/` (1,1 MB), resolver embed (icefile/desustream/blogspot/pixeldrain), parse REST search, soft-404 Kusonime, helper base64/HTML-unescape | **lulus**: 134 test (103 unit + 11 CLI + 14 HTTP + 6 registry), clippy `-D warnings` bersih; live check: Samehadaku 3 hasil seri + 33 link Direct (icefile HLS/MP4 1080/720/360 + blogspot googlevideo), Otakudesu desustream → `cdn.odcloud.net` MP4 Direct, Kusonime metadata + download-only (pesan `Unsupported` rapi) |
| 6 ✅ | TUI Search→Detail→Episodes→Streams + Help | `src/ui/{app,render,theme}.rs`, event loop `crossterm EventStream` + channel job, 10 test render/navigasi `TestBackend` | **lulus**: 10 test TUI (render 4 layar, error banner, help, keymap, job basi diabaikan), semua tes lama tetap hijau |
| 7 ✅ | `src/fallback.rs` + `src/resolve.rs` | fallback berbasis identitas (judul+episode) dengan timeout per source & jejak `Attempt`, **pencarian paralel fan-out** + dedupe, pemilihan resolusi (exact/nearest/policy/prefer_mkv) | **lulus**: 18 test (fallback 11 + resolve 8), termasuk skip source download-only, dedupe lintas source, dan ranking resolusi |
| 8 ✅ | `src/player.rs` (+ `embed.rs` dari Tahap 5) | deteksi mpv/vlc/yt-dlp, argumen mpv/vlc (title, UA, referer, start, cache, `--ytdl`), `PlaybackPlan` (spawn/browser/refuse), spawn detached/foreground, buka browser | **lulus**: 9 test player (argumen, cari binary, embed→ytdl/browser, download-only ditolak); live: URL stream nyata diputar mpv `--frames=1` exit 0 |
| 9 ✅ | Layar Browse, History, Bookmarks, Logs, Settings (+ Help dari Tahap 6) | `src/history.rs`, `src/bookmarks.rs` (JSON atomik, dedupe, batas entri), integrasi TUI: `l` browse, `H` riwayat (Enter putar ulang), `b`/`B` bookmark, `L` log, `g` setting baca-saja, `d` hapus | **lulus**: 18 unit test storage + 7 test render layar baru; restart → history/bookmark terbaca ulang; putar episode mencatat riwayat otomatis |
| 10 ✅ | `README.md` + `.github/workflows/ci.yml` | build Linux & Windows (msvc + cmake/NASM), prasyarat, keymap, config, cara tambah source, troubleshooting, batasan hukum; CI matrix + job MSRV 1.88 | dokumentasi lengkap; CI disiapkan (fmt, clippy, test, release, MSRV) |
| 11 ⏳ | Source #4–11 + hardening + live smoke | **Selesai:** Anoboy, Animeindo, Oploverz (factory + test fixture). **Recon selesai:** KotakAnime (playable, AJAX+XOR), AWSubs (batch), Doronime (download-only/robots), Neonime (alias katalog), Hunter No Sekai (mati) | 6 source terpasang & tiap source punya fixture + capability jujur; 5 source sisanya terdokumentasi di `research/` untuk implementasi berikutnya |

### 21.1 Catatan environment build (ditemukan saat verifikasi)

`~/.cargo/registry` di mesin ini read-only bagi proses sandbox, sehingga build harus
memakai `CARGO_HOME` di dalam workspace. Build pertama juga lama (kompilasi C
`aws-lc-sys`); build incremental setelahnya normal:

```bash
export CARGO_HOME="$PWD/.cargo-home"
cargo build --release
```

Dependensi tetap di-commit sebagai `Cargo.lock`; build normal di mesinmu (tanpa
sandbox) tidak terpengaruh.

---

## 22. Recon source (bukti lapangan, 2026-11-14)

Ringkasan eksekusi recon untuk 3 source prioritas. Detail lengkap + fixture ada di
`research/<source>.md` dan `research/raw/<source>/`.

### 22.1 Matriks kapabilitas (hasil cek langsung, 2026-11-14)

| Source | Host aktif | Stack | Search | Browse | Detail | Episode | Stream langsung | Kesulitan |
|--------|-----------|-------|--------|--------|--------|---------|-----------------|-----------|
| Samehadaku | **hanya `v2.samehadaku.how`** yang asli (`samehadaku.care` = portal JS yang redirect 10 detik via `window.location.replace`, tanpa redirect HTTP → status 200 tapi nol konten anime; `.how`/`v1.*` 301 ke v2; `.email` mati/hijack) | WordPress; HTML + REST `wp-json/apk`; Cloudflare **UA-gated** | ✅ HTML `/?s=` (20/hal) + AJAX `samehada_live_search` (⚠️ cap 6, hasil episode bukan seri) | ✅ ongoing/completed/daftar anime + REST `/apk/latest`, `/apk/schedule`; ⚠️ `/genre/<slug>/` rusak → pakai `/daftar-anime-2/?genre[]=slug` | ✅ | ✅ | ✅ **playable**: player AJAX → **icefile** `SOURCES` (HLS 1080/720/360) atau **Blogspot** → googlevideo; Mp4/Mega/Gofile/Filedon tidak usable | Sedang–sulit (host bervariasi per episode) |
| Otakudesu | **`otakudesu.blog`** = situs asli (`.cam` ternyata domain parkir "domain for sale", `.cloud` 523, `.buzz/.today/.dev` clone, `.digital` app Next.js terpisah) | WordPress 5.6 + theme `Animestream` | ✅ HTML (cap 12 hasil, wajib `post_type=anime`) | ✅ ongoing/completed/genre (popular ❌, jadwal rilis mati) | ✅ | ✅ | ✅ **bisa diputar**: `#pembed` → desustream → `const videoURL` = MP4 CDN (206 `video/mp4`, mpv exit 0; mirror AJAX alternatif sebagian JS) | Sedang (AJAX 2 langkah + resolver desustream) |
| Kusonime | `kusonime.com` (200; Cloudflare **pasif**, tanpa challenge) | WordPress + REST API (`/wp-json/wp/v2/posts`) | ✅ REST JSON (0,8 s, `X-WP-Total: 5199`, 52 halaman) | ✅ kategori; indeks penuh 1 baris `anime-list-bd` (1.226 seri, 497 KB) | ✅ | ❌ **tidak ada daftar episode** (1 halaman = 1 seri, hanya batch `.rar`) | ❌ tidak ada stream sama sekali (0 `<video>`/iframe/`.mp4`; pixeldrain batch = `application/vnd.rar`) | Mudah untuk metadata/katalog; **tidak untuk streaming** |

### 22.2 Temuan per source (bukti)

**Samehadaku** — jalur streaming justru paling bersih dari ketiganya (hasil replay
di mesin ini):

1. Halaman episode menyimpan `var SamehadaData = { ajaxUrl, restUrl:
   ".../wp-json/apk", nonce, turnstileSiteKey }` dan tombol server
   `div.player-server-btn` / `.east_player_option` dengan `data-post`, `data-nume`,
   `data-type`, dan kadang `data-embed`.
2. **`data-embed` punya 3 bentuk** (parser harus menangani ketiganya): (a) iframe
   HTML **ter-escape** (`&lt;iframe …&gt;`) untuk host seperti IceFile, (b) iframe
   dengan `src` biasa, (c) **shortcode belum ter-resolve** seperti
   `[vidlion id=...]` — `player_ajax` hanya menggemakan shortcode itu dan theme tidak
   punya handler → server tersebut **mati** (3 dari 11 server di satu episode sampel).
   Kalau tidak ada `data-embed`, POST `action=player_ajax` + `nonce`/`post`/`nume`/
   `type=urliframe` → respons **HTML** (bukan JSON), di-inject ke
   `#player-frame-wrapper`/`#pembed`.
3. Dari semua server, **hanya dua jalur yang benar-benar bisa diputar**:
   - **IceFile "VIP HD"** → `beta.icefile.my.id/embed/<token>` → berisi
     **`const SOURCES=[{"type":"hls","label":"Stream (1080p)","link":"…m3u8","resNum":1080}, …]`**
     dengan HLS (dan MP4) 1080p/720p/360p. `m3u8` → `302` ke CDN re-signed; **tanpa
     Referer, tanpa UA**; segmennya dinamai `1.png`, `2.png` dengan
     `Content-Type: application/octet-stream` → **jangan filter berdasarkan ekstensi**.
     Diuji: `mp4` 360p `206` (71,6 MB), `m3u8` 360p, keduanya `mpv --frames=1` exit 0.
   - **Blogspot** → `blogger.com/video.g` → URL `googlevideo` `itag=18` (IP-bound &
     kedaluwarsa) → perlakukan seperti media dinamis §12.6.
   - **Mega** butuh SDK (tidak usable), **Gofile** API `401 error-token`,
     **Filedon** embed hanya berisi route table tanpa m3u8 literal → ketiganya
     `Unsupported`/`DownloadOnly`, bukan error fatal.

   Label resolusi hanya ada di teks bebas tombol ("Mega 1080p", "VIP x265",
   "Server 2 720p", "Blogspot"); jumlah server bervariasi 3–11 per episode.
   → `StreamLink.resolution` diisi dari label (parser toleran) dan/atau `resNum` di
   `SOURCES`, sehingga §11 benar-benar punya data resolusi.

Discovery juga terstruktur: REST `GET /wp-json/apk/latest?page=N` → JSON 20 item
(`id, title, series_id, episode, type, url, thumb, time`) dan `/wp-json/apk/schedule`
untuk jadwal; pencarian punya AJAX `?action=samehada_live_search&s=<q>`. Halaman
pencarian server-rendered tetap ada (hasil `.anmsa`, `h2` ber-`title`, badge
`.type`/`.type-badge`, skor `.skor`/`.score`).

**Jebakan parser Samehadaku:**

- **Dua komponen kartu yang tidak kompatibel:** `article.animepost` (halaman search,
  `/daftar-anime-2/`, `/daftar-batch/`) vs `div.post-show li[itemscope]`
  (`/anime-terbaru/`, `/top10/`, `/genre/*`). Satu selector saja → 0 item di separuh
  halaman. Implementasikan keduanya.
- `.type` bermakna 3 hal: `.content-thumb .type` = jumlah episode **atau** tipe media;
  `.data .type` = umur relatif **atau** status. Yang andal hanya
  `.stooltip .type-badge`.
- `div.custom-scrollbar` cocok dengan **dua** elemen di halaman detail; `[0]` adalah
  sidebar tanpa anak → daftar episode kosong tanpa error. Pakai
  `div.custom-scrollbar.space-y-2`.
- `/genre/<slug>/` menghasilkan **nol kartu** di semua slug yang diuji (template
  arsip rusak) → gunakan `/daftar-anime-2/?genre[]=<slug>`. `/anime-movie/` stub
  kosong.
- Daftar episode **terpotong tepat 100 baris** tanpa load-more (One Piece: 1180 → 1081)
  → UI harus jujur menandai "100 episode pertama ditampilkan".
- Cloudflare **UA-gated**: tanpa UA atau UA generik (`curl/8.5`, `Mozilla/5.0`) → 403
  "Just a moment…"; UA Chrome lengkap → 200. Tidak ada JS challenge/Turnstile
  (`turnstileSiteKey: ""`), `robots.txt` Allow `/` termasuk `admin-ajax.php`.
  → retry sekali dengan UA lengkap **sebelum** mengklasifikasikan `Blocked` (§8.2).

Bagian unduhan `.download-eps-links` berisi **Gofile, Filedon, Acefile, Pixeldrain,
Mediafire** (MKV/MP4/x265 × hingga 6 resolusi, 15 link untuk satu episode sampel).
Pixeldrain kadang memberi file langsung (`206 video/matroska`) tapi file lain bisa
`403` (§12.7); sisanya `DownloadOnly`. **Konsekuensi:** adapter mem-parse tombol
server + daftar unduhan, `embed.rs` menangani icefile/blogspot, `hosts.rs` menangani
pixeldrain.

**Otakudesu** — situs asli `otakudesu.blog` (WordPress 5.6, theme `Animestream`,
markup `div.venser`/`detpost`/`epz`). Situs **mendeklarasikan domain resminya sendiri**
di home: `<div class="infoupdate">Situs resmi Otakudesu hanya otakudesu.blog…</div>`
(banner hanya ada di `/`) — jadi probe domain bisa memakai teks ini. Clone terdeteksi
dari: halaman "domain for sale", status 523/401/`?tkn=`, tidak ada `div.venser` +
`themes/Animestream`, route MAL-ID `/anime/\d+-`, atau slug bersufiks acak `-[a-z0-9]{7}/`.

Ekstraksi stream **2 tingkat, keduanya tanpa JS**:

1. **Player default statis** — `#pembed iframe[src]` sudah ada di HTML episode
   (embed `desustream.com`). Halaman embed itu hanya 947 B dan berisi
   `<video><source src="https://rr4---sn-….googlevideo.com/videoplayback?…">` tanpa
   obfuscation. Diuji: `206 video/mp4`, **UA browser wajib** (UA kosong → 403),
   **referer tidak diperlukan** → langsung bisa diputar `mpv --user-agent=…` (§12.6).
   Catatan: URL media **terikat IP** dan kedaluwarsa ~6 jam → jangan di-cache; cache
   URL halaman embed-nya, resolusi media dilakukan tepat sebelum play.
2. **Server alternatif** via AJAX (plain curl, tanpa cookie): POST
   `https://otakudesu.blog/wp-admin/admin-ajax.php`
   - `action=aa1208d27f29ca340c92c66d1926f13f` → `{"data":"<nonce>"}`
   - `action=2a3505c93b0035d3f455df82bf976b84` + `id`, `i`, `q`, `nonce` →
     `{"data":"<HTML base64>"}` → decode → parse `<iframe src>`.
     Parameter `id`/`i`/`q` diambil dari atribut `data-content` (base64) pada
     masing-masing link mirror.
   Error tetap **HTTP 200** dengan `{"message":"Not found."}` / `{"message":"Forbidden."}`
   → parser harus memeriksa isi JSON, bukan status code.

Catatan dari replay langsung di mesin ini (bukan hanya recon): server alternatif hasil
AJAX bisa menunjuk host yang **tidak** bisa di-resolve statis (mis. `filedon.co/embed/…`,
mega) — embed-nya JS. Jalur yang andal adalah **player default `#pembed`** (desustream)
yang memberi MP4 CDN langsung (lihat `const videoURL` di atas). Jadi urutan yang benar:
`#pembed` dulu → AJAX mirror hanya kalau host-nya punya aturan di `embed.rs`.

**Konsekuensi desain:** Otakudesu adalah source pertama yang bisa "end-to-end ke mpv"
tanpa browser. Yang dibutuhkan: `embed.rs` aturan `desustream` (dua pola di atas) dan
`StreamLink.headers.user_agent` terisi.

**Jebakan parser Otakudesu (dari recon):**

- Link mirror punya `href="#"`; payload asli ada di `data-content` (base64) + AJAX —
  parser lama yang mengambil `a[href]` akan dapat `#` semua. `.mirrorstream` sekarang
  **di luar** `#pembed`.
- `ul.m720p` tidak pernah ditutup `</ul>` (ditutup `</div></div>`) → jangan mengandalkan
  struktur sibling yang well-formed (html5ever tetap memulihkan).
- `.epz`/`.epztipe` ambigu: di halaman ongoing = nomor episode terbaru + hari
  ("Episode 6"/"Jumat"); di completed = total episode + skor ("12 Episode"/"8.20").
- Label unduhan bervariasi: `360p`, `Mp4 360p`, `MKV 1080p`, batch `MP4 360p` → parse
  case-insensitive `^(mp4|mkv)?\s*(\d{3,4})p$`; kalau hanya cocok "Mp4 720p", separuh
  episode menghasilkan nol link.
- Label detail vs batch berbeda: `Tipe/Type`, `Skor/Rating`, `Total Episode/Episodes`,
  `Durasi/Duration`, `Studio/Studios`.
- Halaman genre/A-Z memakai layout ketiga (`.col-anime-*`), bukan `.detpost`;
  `img[alt]` di kartu genre kadang salah → pakai `h2.jdlflm`/`.col-anime-title a`.
- Slug episode/batch adalah singkatan uploader, **tidak bisa dibangun** dari slug anime
  (`kusuriya-hitorigoto-s3-sub-indo` → `knh-s3-episode-5-sub-indo`) → selalu ambil
  dari `href`, jangan konstruksi URL.
- Search dibatasi 12 hasil dan **wajib** `post_type=anime` (tanpa itu 5 hasil → 2).
  `/anime-populer/` hanya interstitial redirect; `/jadwal-rilis/` basi (Spring 2023) →
  tidak diimplementasikan.

**Kusonime** — **download-only, bukan streaming.** Satu halaman = satu *seri*; tidak
ada daftar episode per-episode (0 kemunculan `eplister`/`episodelist`/`listlink` di 12
post sampel 2017–2026), tidak ada `<video>`/iframe player, dan 0 URL `.mp4/.mkv/.m3u8`
di halaman detail maupun halaman One Piece 268 KB — semuanya batch `.rar`.

REST API bekerja penuh dan ini jalur terbaik untuk discovery:
`GET /wp-json/wp/v2/posts?search=<q>` → `200` (~0,8 s) dengan `X-WP-Total: 5199`,
`X-WP-TotalPages: 52`, mendukung `orderby/order/categories/_fields`; query kosong → `[]`.
**Tapi** `content.rendered` hanya berisi sinopsis (~1,4 KB) — tabel unduhan **tidak ada**
di REST, jadi metadata boleh via REST sedangkan kotak host harus dari HTML detail
(`#dl` → `div.smokeddlrh` → `div.smokeurlrh`). Halaman `/anime-list-bd/` memuat semua
1.226 seri dalam satu halaman 497 KB (tanpa paginasi) dan `a.kmz[rel]` membawa post ID.
Home punya 520 halaman; pencarian halaman-2 = `/page/2/?s=`.

Jebakan parser Kusonime:

- **Soft-404 (paling berbahaya):** slug tidak dikenal → `404` + `Location: /` yang
  di-follow jadi **200 berisi homepage**. Parser wajib mematikan redirect untuk fetch
  detail, atau minimal memverifikasi `h1.jdlz` ada.
- `<body>` tidak punya class → selector `body.single` gagal diam-diam; scope ke
  `#venkonten`/`.venser`.
- Sinopsis tidak bertag: dibatasi `.clear` setelah `.info` → parse sibling `p` sampai
  `.dlbodz`.
- Kartu list tidak membawa tipe/status/episode/skor; "Released on 12:00 pm" hanya jam
  (tanpa tanggal). Label `.info` punya spasi ekor (`"Genre "`), `Total Episode` bisa
  `"?"`, dan `720p` vs `720P` tidak konsisten.
- Host yang benar-benar ada: `acefile.co`, Google Drive (2 bentuk host), `pixeldrain`,
  terabox, `mega.nz`, `megaup`, `itsfileditchfiles.st`, `lbx.to`, `hxfile.co`, `mir.cr`
  (tidak ada Zippy/GoFile/Krakenfiles — jangan dikodekan).
- "Google Sharer" di label **selalu** `acefile.co` (cocokkan host, bukan teks anchor).
  Post terbaru menyembunyikan host di rantai 4 hop `s.id` → `duniakode.com/raw` →
  `srnky.com` → reCAPTCHA/Turnstile; 6 label host hanya berbagi 2 token `s.id`, jadi
  teks anchor tidak bisa dipercaya.
- Unduhan langsung: Acefile `/d/<token>` → `307 login` walau pakai Referer+UA;
  Pixeldrain `/api/file/<id>` untuk file Kusonime → `403 file_rate_limited_captcha_required`
  (hotlink hanya untuk pelanggan); hanya Fileditch memberi `.rar` HTTP langsung.
  → jalur download hanya layak untuk v1.1 (butuh browser/manual).

**Konsekuensi desain:** `capabilities.streams = false`, `batch_download = true`;
orchestrator fallback **melewati** Kusonime di jalur stream (tidak menunggu error),
dan UI memberi label "download-only". Kusonime dipakai untuk pencarian/browse metadata
dan untuk hook download mendatang.

### 22.3 Bukti domain berpindah (justifikasi `MirrorSet`)

- `samehadaku.email` → redirect ke `expatriate.travel/country-guides/saint_lucia/…`
  (domain lama sudah diambil alih/parkir).
- `samehadaku.care` → 200 penuh, tetapi hanyalah **portal JS** yang menghitung 10
  detik lalu `window.location.replace` ke `v2.samehadaku.how` (tanpa redirect HTTP) →
  terlihat sukses, isinya nol anime. Marker konten wajib dicek (§8.1).
- `otakudesu.cloud` → HTTP 523 (origin down).
- `otakudesu.cam` → **200 tetapi domain parkir** ("This domain is for sale", 6.990 B,
  tanpa markup Otakudesu). Ini bukti bahwa status 200 saja tidak cukup.
- Banyak domain "otakudesu.*" adalah clone/SEO dengan layout menyerupai aslinya
  (`.buzz` rebrand, `.today/.dev` route MAL-ID, `.digital` app Next.js) → probe harus
  memvalidasi **marker situs** (`.infoupdate` di home, `div.venser`, theme
  `Animestream`) dan menolak pola clone.

Semua klaim di atas berasal dari fetch langsung + fixture yang disimpan di
`research/raw/<source>/`. Detail selector lengkap ditulis di `research/<source>.md`
(ketiga source prioritas sudah selesai: Samehadaku 681 baris, Otakudesu 43 KB,
Kusonime 347 baris) dan parser dibuat langsung dari fixture tersebut.

### 22.4 Bukti playability (diuji dari mesin ini, 2026-11-14)

| Source | Rantai ekstraksi | Hasil uji |
|--------|------------------|-----------|
| Samehadaku | episode → `data-embed`/`player_ajax` → iframe `beta.icefile.my.id` → `const SOURCES=[…]` | `m3u8` 360p → `302`; `mp4` 360p → `206` (`71.642.163` byte); **mpv `--frames=1` exit 0** untuk keduanya; varian tersedia 1080p/720p/360p |
| Otakudesu | episode → `#pembed` iframe `desustream.net` → `const videoURL` | `https://cdn.odcloud.net/…_720p.mp4` → `206 video/mp4` (`143.460.803` byte); **mpv `--frames=1` exit 0** (dengan & tanpa UA) |
| Otakudesu (mirror AJAX) | `action=2a3505…` → base64 → `<iframe>` | host hasil bisa `filedon.co/embed/…` (JS, tidak resolvable) → diperlakukan `Unsupported`/`DownloadOnly` |
| Kusonime | — | tidak ada URL media sama sekali; `pixeldrain` batch = `.rar` (`403` bila hotlink) → `streams=false` |

Perintah utama yang dipakai (mpv build Arch, tanpa config):

```bash
mpv --no-config --no-audio --frames=1 --vo=null --user-agent="$UA" "<url>"
```

Catatan: tes ini **tidak** memakai proxy alat riset — URL media diambil ulang lewat
`curl`/egress mesin sendiri, sesuai aturan §12.6 (media URL terikat IP egress).
Untuk IceFile, bukti tambahan: segmen HLS diberi nama `1.png`, `2.png` dengan
`Content-Type: application/octet-stream` — karena itu `hosts.rs`/`embed.rs` **tidak
boleh** memfilter URL berdasarkan ekstensi `.ts/.mp4`; validasi cukup dari label
`SOURCES` (`type: hls|mp4`).

---

### 22.5 Verifikasi live Tahap 5 (`bin/sourcecheck`)

Dijalankan langsung ke jaringan (bukan fixture), 2026-11-15:

| Source | Perintah | Hasil |
|--------|----------|-------|
| Samehadaku | `sourcecheck --source samehadaku --query naruto` | 3 hasil **seri** (Naruto Kecil, Boruto, Shippuuden); detail lengkap (status, tahun, skor, genre, 100 episode); episode 6 → **33 link Direct**: IceFile HLS+MP4 1080/720/360 + `Original Quality` MKV, plus Blogspot `googlevideo` (`itag=18`, IP egress sendiri) |
| Otakudesu | `sourcecheck --source otakudesu --query kaguya` | 5 hasil; detail (12 episode); episode 6 → desustream default → **`https://cdn.odcloud.net/...720p.mp4` Direct**; 43 link termasuk download redirector `link.desustream.com` (ditandai DownloadOnly) |
| Kusonime | `sourcecheck --source kusonime --query ameku` | 3 hasil (REST); detail lengkap (status/skor/genre/sinopsis); `streams` & `episodes` menjawab `Unsupported` dengan copy yang rapi — sesuai desain download-only |

Catatan: `sourcecheck` juga dipakai sebagai alat debugging parser; kalau situs berubah,
jalankan ulang dan lihat field mana yang gagal (`SourceError::Parse` menyebutnya).

### 22.6 Verifikasi player & TUI (Tahap 6–8)

| Uji | Hasil |
|-----|-------|
| URL stream nyata (Samehadaku episode 6, IceFile 720p HLS) diputar `mpv --no-config --frames=1` | **exit 0** (player path terbukti, bukan hanya unit test) |
| TUI dijalankan di pty (`script -qec`, 100×30) | header, tab, footer, dan hint render; `Esc` lalu `q` → **exit 0** tanpa panic |
| TUI saat stdin/stdout bukan terminal (pipe/CI) | otomatis jatuh ke mode ringkasan, tidak memaksa TUI |
| 10 test `TestBackend` (4 layar + help + error + keymap + job basi + Ctrl+C) | hijau |

Catatan UX: `q` sengaja **tidak** keluar saat sedang mengetik di input pencarian
(huruf q adalah teks). `Esc` dulu untuk melepas fokus, atau `Ctrl+C` kapan saja.

### 22.7 Recon source tambahan (2026-11-15)

Ringkasan status 8 source roadmap (detail + fixture di `research/<source>.md`):

| Source | Domain aktif | Playable? | Catatan penting |
|--------|--------------|-----------|-----------------|
| Oploverz | `oploverz.diy` | ✅ | API JSON `backapi.oploverz.ac` (search `q`, detail dibungkus `{data}`, episodes 10/hal, `streamUrl`); filedon memberi MP4 presigned (~1 jam); live: detail lengkap + 1 stream Direct |
| Animeindo | `anime-indo.lol` | ✅ | `a.server[data-video]` → xtwap → googlevideo MP4 (`206`); jangan konstruksi URL episode |
| Anoboy | `anoboy.id` (+`anoboy.nl`) | ⚠️ sebagian | iframe Blogger → MP4 `googlevideo`; gdplayer JS, turbovid mati; wajib `?type=anime` di `/series/` |
| KotakAnime | `s13.nontonanimeid.boats` (rotasi) via portal `kotakanimeid.link` | ✅ HLS | AJAX `player_ajax` (nonce, data-post/nume) → embed base64+XOR → `cdn2…/index.m3u8` 480/720/1080; host `s<N>` berotasi |
| AWSubs | `www.awsub.web.id` | ❌ | batch/download-only (Blogger), stream hanya di `puristream.blogspot.com` (Referer blogger, IP-bound) |
| Doronime | `doronime.id` | ❌ | download-only dengan safelink/CSRF; `robots.txt` melarang `/download` & `/go/` → **tidak diotomasi** |
| Neonime | alias `/neonime/` di `animase.xyz` | ❌ | katalog sama dengan Muvipro (Anoboy/Animeindo); player butuh AJAX + JS |
| Hunter No Sekai | tidak ada | ❌ | domain mati/landing parking; konten arsip mayoritas dewasa → tidak diimplementasikan |

Keputusan: source yang tidak playable tetap didokumentasikan tetapi **tidak** dipasang
sebagai adapter streaming; capability matrix di TUI harus jujur (`streams=false`).

## 23. Cara menambah source baru (checklist)

1. Riset: endpoint search/browse, pola URL, selector, cara ekstraksi stream, header.
   Simpan fixture di `tests/fixtures/<source>/`.
2. Buat `src/sources/<nama>/` dengan `mod.rs`, `client.rs`, `parse.rs`.
3. Implementasikan `AnimeSource`; isi `capabilities()` **jujur** (fitur yang tidak ada
   → `false` + `Unsupported`).
4. Tulis test parser dari fixture (minimal: search, detail, episodes, streams).
5. Daftarkan di `sources::registry::factories()` dan tambahkan blok `[sources.<nama>]`
   di `config.example.toml`.
6. Jalankan `anicli sourcecheck --source <nama> --query "<judul>"` untuk verifikasi live.
7. Update tabel recon §22 dan README.

---

## 24. Struktur direktori final

```
anicli-rs/
├── Cargo.toml
├── Cargo.lock
├── rust-toolchain.toml        # channel = "1.88" (atau stable)
├── README.md
├── LICENSE                    # MIT
├── DESIGN.md                  # dokumen ini
├── config.example.toml
├── .github/workflows/ci.yml   # fmt, clippy, test, matrix build linux+windows
├── assets/logo.txt
├── research/                  # recon mentah + fixture asli (dev only)
│   ├── samehadaku.md
│   ├── otakudesu.md
│   ├── kusonime.md
│   └── raw/<source>/*.html
├── src/
│   ├── lib.rs                 # permukaan library untuk binary & test
│   ├── main.rs                # binary tipis: CLI + bootstrap
│   ├── bin/sourcecheck.rs     # smoke test live per source (Tahap 5)
│   ├── config.rs
│   ├── paths.rs
│   ├── logging.rs
│   ├── model.rs
│   ├── error.rs
│   ├── http.rs
│   ├── embed.rs               # resolver embed/host: icefile, desustream, blogspot, pixeldrain
│   ├── resolve.rs             # pemilihan resolusi/stream
│   ├── fallback.rs            # orkestrasi fallback + fan-out search
│   ├── resolve.rs
│   ├── player.rs
│   ├── history.rs
│   ├── bookmarks.rs
│   ├── sources/
│   │   ├── mod.rs             # trait + Capabilities + registry + factories (3 source)
│   │   ├── util.rs            # helper parsing bersama (termasuk base64)
│   │   ├── mock.rs            # MockSource untuk tes & --mock
│   │   ├── samehadaku/{mod,client,parse}.rs   # REST search + player_ajax + icefile/blogspot
│   │   ├── otakudesu/{mod,client,parse}.rs    # nonce AJAX + desustream + soft-404-safe
│   │   ├── kusonime/{mod,client,parse}.rs     # REST metadata + katalog batch (no stream)
│   │   └── ... (source lain menyusul)
│   └── ui/                    # Tahap 6: 4 layar inti + Help
│       ├── mod.rs
│       ├── app.rs             # state machine, job async, keymap, fallback status
│       ├── render.rs          # render murni per layar (dites TestBackend)
│       └── theme.rs
│   # layar Browse/Settings/History/Bookmarks + widget picker menyusul Tahap 9
└── tests/
    ├── cli_smoke.rs           # 11 test end-to-end binary (config/log/exit code)
    ├── fixtures/<source>/*.html
    ├── parser_samehadaku.rs
    ├── parser_otakudesu.rs
    ├── parser_kusonime.rs
    ├── fallback.rs
    └── resolve.rs
```

---

## 25. Lampiran: dependensi terkunci (target Tahap 2)

| Crate | Versi | Fitur penting | Alasan |
|-------|-------|----------------|--------|
| `ratatui` | 0.30 | default (`crossterm`, `macros`, `layout-cache`) | TUI widget + re-export crossterm (MSRV 1.88) |
| `crossterm` | 0.29 | hanya `event-stream` | ⚠️ dependency tambahan hanya untuk `EventStream`; tipe/backend tetap diimpor dari `ratatui::crossterm` (D16) |
| `tokio` | 1 | `rt-multi-thread`, `macros`, `sync`, `time`, `process` | runtime async + spawn player |
| `reqwest` | 0.13 | `rustls`, `webpki-roots`, `json`, `gzip`, `brotli`, `charset`, `cookies` | HTTP tanpa OpenSSL |
| `scraper` | 0.29 | default | parse HTML CSS selector |
| `serde` / `serde_json` | 1 | `derive` | model + persistence |
| `toml` | 1 | default | config |
| `directories` | 6 | default | known folders |
| `tracing` | 0.1 | default | instrumentasi |
| `tracing-subscriber` | 0.3 | `env-filter`, `fmt` | output log |
| `tracing-appender` | 0.2 | default | rolling file |
| `clap` | 4 | `derive` | CLI |
| `async-trait` | 0.1 | default | dyn-compatible async trait |
| `anyhow` / `thiserror` | 1 / 2 | default | error aplikasi / error domain |
| `url`, `html-escape`, `futures`, `unicode-width`, `chrono` | sesuai | — | util (chrono: tanggal episode & timestamp riwayat) |
| `ratatui-image` (opsional) | 11 | `crossterm` | fitur `image-preview`; kompatibel dengan ratatui 0.30 |
| `image` (opsional) | 0.25 | `png`, `jpeg`, `webp` | decoder poster — **bukan** `image-defaults` (menarik avif/ravif/tiff, build jauh lebih berat) |
| `tempfile` (dev) | 3 | — | test config/persistence di direktori sementara |

Catatan: `chrono` dipakai untuk `NaiveDate`/timestamp; kalau mau lebih ringan bisa
`time`, tapi `chrono` lebih familiar untuk parsing tanggal Indonesia.

---

*Dokumen ini akan diperbarui setiap tahap; perubahan desain harus lewat sini dulu.*
