# Changelog

Semua perubahan penting pada proyek ini didokumentasikan di file ini.
Format mengikuti [Keep a Changelog](https://keepachangelog.com/id/1.1.0/) dan
[Semantic Versioning](https://semver.org/lang/id/).

## [0.9.0] - 2026-11-19

### Ditambahkan

- **Playback untuk source katalog**:
  - **ani.pm**: ambil token player lewat `GET /api/anime/playback-bootstrap/settlar/<route>?ep=<n>`
    (tanpa login) → link `embed.settlar.io`; plus halaman tonton resmi sebagai cadangan.
  - **mkissa**: rute episode asli `/anime/<id>/p-<n>-sub` (sebelumnya `/episode-<n>`
    yang 404) → membuka player resmi situs.
- **Routing browser** untuk host yang player-nya tidak bisa diambil yt-dlp/mpv
  (`settlar.io`, `mkissa.to`, `filelotion.fyi`): Enter pada episode membuka browser,
  bukan gagal `exit 2`.
- **Anichin**: halaman hasil kosong ("Tidak ditemukan") diperlakukan sebagai daftar
  kosong, bukan error parser (ditemukan saat uji lokal binary pre-release CI).
- Skrip `scripts/verify-release.sh`: unduh aset rilis dari GitHub, cek SHA256, lalu
  jalankan rantai penuh (search → detail → episode → stream) per source + cek registry.
- Tag dengan sufiks (mis. `v0.9.0-rc1`) otomatis menjadi **pre-release** di CI,
  sehingga bisa dites lokal sebelum rilis final.

## [0.8.2] - 2026-11-19

### Diperbaiki

- **ani.pm**: banyak judul gagal ("parser tidak menemukan ld+json") karena ani.pm
  mengembalikan halaman 404 berupa shell SPA (`<title>ani.pm</title>`) untuk judul yang
  tidak di-host. Sekarang terdeteksi sebagai **"tidak ditemukan"** dengan pesan jelas,
  bukan error parser yang membingungkan.
- **ani.pm: daftar episode** sebelumnya dibentuk dari `numberOfEpisodes` sehingga bisa
  memunculkan episode yang tidak ada. Sekarang diambil dari **link episode nyata** di
  halaman detail (`a[href*="/episode-"]`), mis. Naruto menampilkan 120 episode halaman
  pertama dan Japan Anima(tor)'s Exhibition 35 episode; tidak ada lagi URL karangan.
- **Pesan error host tidak informatif** (mis. Pixeldrain takedown hanya
  `mpv exit 2`). Ditambahkan preflight untuk host rawan takedown
  (pixeldrain/mediafire/filedon/acefile) yang membaca respons: Pixeldrain mengembalikan
  **HTTP 451 + JSON** `{"message":"…takedown report","extra":{"type":"copyright"}}` →
  pesan UI menjadi `Link pixeldrain.com tidak bisa diputar: HTTP 451 — … (copyright)`.
  Unduhan langsung juga menampilkan alasan ini, bukan sekadar "server menolak".
- **Ringkasan fallback tidak lagi penuh "tidak didukung"**: source katalog (ani.pm,
  mkissa) tidak lagi mendominasi pesan "Semua source gagal"; yang ditampilkan hanya
  percobaan yang informatif.

### Ditambahkan

- `http::describe_response_error` (JSON `message`/`extra.type`, halaman HTML
  "Unavailable for legal reasons") + test.
- Test parser ani.pm: episode nyata (35 link), halaman 404, dan penanganan error.

## [0.8.1] - 2026-11-19

### Diperbaiki

- **Source baru tidak muncul di config lama** (gejala: tab Bahasa menunjukkan
  `[EN] 0` dan ani.pm tidak ada di daftar source). `Config::load` sekarang otomatis
  menambahkan source default yang belum ada di `config.toml` user
  (`merge_new_sources`), menyisipkannya ke `fallback_order` sebelum `kusonime`, dan
  menyimpan config sekali. Jika user menonaktifkan **semua** source, migrasi tidak
  dijalankan (biar validasi yang memberi tahu).
- **Proxy Android/Termux kini berjalan di thread + runtime tokio sendiri**, jadi
  hidupnya tidak bergantung pada runtime pemanggil (sebelumnya test/pemanggilan
  berikutnya bisa gagal karena listener mati saat runtime pertama selesai).

### Ditambahkan

- **Mkissa** (`mkissa.to`) — source berbahasa Inggris. Website memakai SPA + challenge
  JS, tetapi **API GraphQL** `api.mkissa.net/api` menerima query mentah
  (introspection terbuka): `shows(search:{query})`, `show(_id)` dengan
  `availableEpisodesDetail` (sub/dub) untuk daftar episode. Endpoint `episode`
  memerlukan crypto klien (`AA_CRYPTO_MISSING`) → `streams = false` (jujur).
  Terverifikasi live: `search("naruto")` 24 hasil, `search("mf ghost")` 13 episode.
- Method HTTP `post_json_text` (GraphQL) + test parser Mkissa (search/detail/error).

## [0.8.0] - 2026-11-19

### Ditambahkan

- **Anichin** (`z1.anichin.blog`) — source donghua sub Indo: pencarian `/?s=`,
  listing ongoing/completed (`/series/?status=`), detail `/series/<slug>/`, daftar
  episode (`div.eplister`), dan player embed `rmb.anichin.bio`.
- **Ani.pm** (`ani.pm`) — source berbahasa **Inggris**: pencarian lewat JSON API
  `/api/anime/search?q=`, detail dari LD+JSON `TVSeries`, daftar episode dibentuk dari
  `numberOfEpisodes` (`/anime/<routeId>/episode-<n>`). Player SPA-nya belum
  diimplementasikan → `streams = false` (jujur di capability).
- **Metadata bahasa** pada trait: `AnimeSource::language()` dengan default
  `Language::Indonesian`, enum `Language { Indonesian, English, Japanese }`
  (extensible) + `Language::ALL`, `code()`, `tag()`, `label()`, `from_code()`.
- **Tab/popup Bahasa** (tombol `Y`): rolling selection semua bahasa + "semua bahasa",
  menampilkan jumlah source per bahasa. Pilihan memfilter daftar source (`s`) dan
  urutan pencarian; tersimpan di `[general] language_filter`.
- **Label bahasa** di daftar source: `[ID] samehadaku`, `[EN] anipm`, dst.
- `SourceRegistry::languages()` untuk daftar bahasa unik.
- Test: parser Anichin (fixture search/series/episode), parser Ani.pm (JSON search +
  LD+JSON detail + 220 episode), filter bahasa (popup → EN → source aktif pindah &
  order hanya EN), label bahasa, dan `registry.languages()`.

### Diubah

- Default config: 9 source (anichin, anipm ditambahkan), `language_filter = "all"`.
- README: tabel source + bagian bahasa + keymap `Y`.

## [0.7.1] - 2026-11-16

### Diperbaiki

- **`anicli update` panic di Android/Termux**:
  `rustls-platform-verifier ... Expect rustls-platform-verifier to be initialized`.
  Penyebabnya: `update::download_asset` (dan `android_proxy`) membuat
  `reqwest::Client::builder()` langsung, sehingga jatuh ke verifier platform Android
  yang butuh JNI. Sekarang **semua** client HTTP lewat `http::client_builder()` yang
  memasang provider ring + TLS `webpki-roots`.
- Ditambahkan test penjaga regresi (`tests/tls_guard.rs`) yang memindai `src/` dan
  gagal bila ada `reqwest::Client::builder()` di luar `src/http.rs`, plus test bahwa
  `client_builder()` bisa membentuk client.

## [0.7.0] - 2026-11-16

### Diperbaiki

- **Android/Termux: mpv-android menutup sendiri** saat memutar link dari
  Otakudesu→desustream dan Oploverz→blogspot. Penyebabnya: intent `VIEW` tidak bisa
  membawa header, sedangkan kedua jenis link membutuhkannya —
  **Blogger/googlevideo** mewajibkan `Referer: https://www.blogger.com/` dan
  **desustream** memakai `User-Agent` khusus. Ditambahkan **proxy HTTP lokal**
  (`src/android_proxy.rs`) yang berjalan di `127.0.0.1`:
  - menyuntikkan `Referer`/`User-Agent` ke permintaan upstream;
  - meneruskan `Range` dan `Content-Length` (seeking tetap jalan);
  - **menulis ulang playlist HLS** (segmen, `#EXT-X-KEY`, `#EXT-X-MAP`) agar tetap
    lewat proxy.
  mpv-android kini menerima URL `http://127.0.0.1:PORT/...` sehingga tidak lagi
  menerima 403 yang membuatnya crash.
  **Bukti uji**: link `googlevideo` nyata → tanpa header **403 Forbidden**,
  lewat proxy **206 Partial Content** berisi MP4 (`ftyp`).
- **Referer Blogger** sekarang benar-benar disetel untuk link `googlevideo` di
  Samehadaku, Anoboy, dan Oploverz (sebelumnya `None`), sehingga berfungsi baik di
  desktop maupun Android.
- **Parser Oploverz gagal saat API mengirim `null`** (mis. `poster`, `description`,
  `japaneseTitle`, `genres`, `streamUrl`). Inilah penyebab pencarian seperti
  "Aho Girl" kadang tidak muncul. Field kini null-tolerant (`null_to_default`) dan
  ada test khusus untuk payload ber-`null`.

### Ditambahkan

- Test end-to-end proxy dengan upstream lokal: memastikan `Referer`/`User-Agent`
  benar-benar diteruskan, tidak menambah header jika tidak diminta, dan kebijakan
  "kapan butuh proxy" (HLS, googlevideo, atau ada header).
- Test parser Oploverz untuk respons berisi `null`.

## [0.6.0] - 2026-11-16

### Ditambahkan

- **Fitur update mandiri** (`src/update.rs`):
  - `anicli update` / `anicli -U` — baca rilis terbaru dari GitHub API, bandingkan
    versi semver, unduh aset sesuai platform (`anicli-x86_64-linux`,
    `anicli-x86_64-windows.exe`, `anicli-aarch64-android`) dan ganti binary yang
    berjalan. `anicli update --check` hanya melaporkan versi.
  - Termux: target otomatis `$PREFIX/bin/anicli`; Windows: karena `.exe` yang sedang
    berjalan terkunci, berkas baru disiapkan sebagai `anicli.new.exe` dan perintah
    `move /Y` dicetak.
  - Bila folder binary tidak bisa ditulis, updater mencetak perintah manual alih-alih
    gagal diam-diam.
  - **Toast non-intrusif** di footer TUI saat ada versi baru (hilang sendiri ~45 detik),
    dengan perintah yang harus dijalankan. Pengecekan berjalan di latar saat start,
    di-cache di `<data>/update-check.json` (`[update] check_interval_hours`, default 12
    jam) dan bisa dimatikan (`check_on_start = false`).
- Bagian config baru `[update]`, dokumentasi `README` (termasuk tabel perilaku per
  platform) dan baris bantuan `anicli -U`.
- Test: perbandingan versi, cache kedaluwarsa, URL aset per platform, pemasangan
  binary (executable bit, sisa `.new` dibersihkan), dan toast UI.

### Diperbaiki

- Respons GitHub API yang berisi `null` (mis. `body` kosong) tidak lagi membuat parser
  gagal — field opsional di-`Option`-kan.

## [0.5.0] - 2026-11-16

### Ditambahkan

- **Streaming search**: hasil pencarian muncul **per source** begitu source itu
  merespons, tidak menunggu yang paling lambat. Event baru
  `SearchSource`/`SearchDone`; penggabungan tetap deterministik (urutan prioritas,
  source aktif di depan) dan dedupe judul tetap berjalan. Progres "x/y source
  merespons" tampil di status, error per source dicatat tanpa menghentikan source lain.
- **Batalkan pencarian**: `Esc` saat pencarian berjalan meng-abort task koordinator
  (JoinSet ikut membatalkan semua task source).
- **Source selector** (`s`): popup berisi semua source dari config (aktif `●`, enabled
  `○`, disabled/tidak tersedia) dengan **filter teks** (case-insensitive), navigasi
  panah / j-k saat filter kosong, `Enter` memilih, `Esc` batal. Menggantikan siklus
  source berurutan; `S` (uppercase) tetap toggle search scope.
- Test: urutan merge streaming + dedupe, filter/navigasi/select popup, pesan kosong,
  stale job, dan `s` tidak lagi siklus.

### Diubah

- `s` sekarang membuka popup (bukan siklus). Di input pencarian, huruf `s` tetap teks.
- README/help diperbarui.

## [0.4.5] - 2026-11-15

### Diperbaiki

- **Salah source saat membuka episode/streaming**: `PlaybackTarget.origin` memakai
  source tab, bukan source asal detail. Akibatnya memilih hasil dari Oploverz saat tab
  di Samehadaku membuat fast-path fallback menembak Samehadaku dengan URL Oploverz,
  lalu episode kosong/error. Sekarang origin = `detail.source`, jadi 0.4.4+ memuat
  daftar episode Oploverz dengan benar (terverifikasi: Shin Evangelion Movie → 4
  episode, stream 1 Direct).
- **Urutan hasil pencarian tidak deterministik**: hasil digabung berdasarkan source
  yang selesai lebih dulu, sehingga dedupe "first wins" tidak menghormati prioritas.
  Sekarang hasil dikumpulkan per indeks lalu digabung sesuai urutan prioritas, dan
  source yang sedang aktif selalu di depan.
- **Detail menampilkan "Episode: 0"** padahal daftar belum dimuat: sekarang
  "? (Enter untuk memuat)".

## [0.4.4] - 2026-11-15

### Diperbaiki

- **Kusonime batch/BD tidak bisa diunduh**: baris "Batch (semua episode)" dibuat
  dengan URL **homepage** (`https://kusonime.com/`), bukan halaman detail. Akibatnya
  pengambilan tabel unduhan gagal dan orkestrator fallback mencoba *streaming* di
  source lain. Sekarang URL baris batch = halaman detail (tempat tabel `#dl` berada),
  baris batch hanya dibuat bila tabelnya memang ada, dan hasilnya 29 link unduhan
  (terverifikasi live untuk "Ameku Takao BD").
- **Hasil pencarian dari banyak source tidak jelas asalnya**: setiap baris hasil kini
  diberi badge `[source]`, dan status menampilkan rincian per source
  (`12 hasil · samehadaku 5 · otakudesu 4 · …`) beserta error per source.
- **Layar Detail** kini menampilkan baris `Source: <nama>`.

### Ditambahkan

- Cakupan pencarian: tombol **`S`** di layar Cari untuk beralih antara
  "semua source" dan "source aktif"; default diatur lewat
  `[general] search_scope = "all" | "selected"`.
- Test: badge source, label Source di detail, toggle cakupan, dan URL batch Kusonime.

## [0.4.3] - 2026-11-15

### Diperbaiki

- **Android/Termux panic saat mencari**: binary Termux crash di
  `rustls-platform-verifier-0.6.2/src/android.rs:94` karena verifier Android butuh
  JNI/`Context`, yang tidak ada pada binary non-JNI. Sebagai gantinya, TLS kini
  memakai `rustls` + **root CA statis `webpki-roots`** melalui
  `http::tls_config()` dan `use_preconfigured_tls` (ALPN h2/http1.1), sehingga
  verifikasi sertifikat tidak menyentuh API Android. Terverifikasi live di desktop;
  binary Android dibangun ulang oleh CI.

## [0.4.2] - 2026-11-15

### Ditambahkan

- **Binary Android/Termux dibangun CI**: job `build-android` (Android NDK r27c +
  `cargo-ndk`, target `aarch64-linux-android`, arm64-v8a, API 24) menghasilkan
  `anicli-aarch64-android` + `sourcecheck-aarch64-android` dan arsip
  `anicli-rs-v<version>-aarch64-android.tar.gz`. Job memverifikasi ELF lewat
  `readelf` (Machine AArch64, interpreter `/system/bin/linker64`) sebelum mengunggah.
- Artefak Android ikut dilampirkan ke GitHub Release saat tag `v*`, jadi pengguna
  Termux **tidak perlu build manual**:
  `pkg install curl tar && tar -xzf anicli-rs-v0.4.2-aarch64-android.tar.gz && install -m755 anicli "$PREFIX/bin/"`.

### Diubah

- `docs/TERMUX.md`: opsi A (unduh binary rilis, direkomendasikan) vs opsi B (build
  dengan script). README ikut menyebut binary Android.

## [0.4.1] - 2026-11-15

### Diperbaiki

- Android: setelah intent berhasil, TUI tidak lagi menulis "selesai diputar"
  (padahal video baru dibuka di aplikasi lain). Ada event `PlayerLaunched` dengan
  status "dibuka di <program> — putar di aplikasi Android".

### Ditambahkan

- `docs/TERMUX.md`: bagian mpv.conf / input.conf — menjelaskan bahwa config mpv
  Termux tidak dipakai, config mpv-android ada di
  `Android/data/is.xyz.mpv/files/`, dan intent tidak bisa membawa argumen mpv
  per-file (UA/Referer/cache harus di config aplikasi). README ikut diperbarui.

## [0.4.0] - 2026-11-15

### Ditambahkan

- **Android/Termux**: peluncur player Android (`src/player/android.rs`). Di Termux,
  `p` tidak men-spawn mpv terminal melainkan menyerahkan URL ke aplikasi lewat intent
  `VIEW` dengan urutan fallback `am` (`-p is.xyz.mpv` / `-p org.videolan.vlc` bila
  dipilih) → `termux-am-starter` → `termux-am` → `termux-open` → `termux-open-url`,
  lengkap dengan log percobaan + exit code bila semuanya gagal.
- Config `[playback] android_player = "auto" | "mpv" | "vlc"`.
- `scripts/build-termux.sh` (build native aarch64 + verifikasi ELF/linker64 + install
  ke `$PREFIX/bin/anicli`) dan `docs/TERMUX.md` (panduan fresh Termux).
- `open_browser` memakai `termux-open-url` di Android.

### Diubah

- **TLS backend pindah ke `ring`** (`rustls-no-provider` + provider dipasang sekali):
  dependency `aws-lc-rs` hilang, sehingga build Termux/Android tidak lagi butuh
  cmake/NASM — cukup clang/perl. Binary juga lebih kecil; HTTPS tetap terverifikasi
  live (50 hasil Animeindo).

## [0.3.1] - 2026-11-15

### Diperbaiki

- **CI Windows hijau**: `tests/tui_render.rs` meng-`cfg(unix)`-kan import `PathBuf`
  sehingga kompilasi Windows gagal; import dipisah dengan benar.
- **Checksum artefak**: `SHA256SUMS` ikut ter-hash sendiri di Windows (file terkunci
  `Out-File` + `Get-FileHash dist/*`); sekarang hanya binary yang di-hash.

### Diubah

- **Semua binary rilis kini dibangun oleh CI**, bukan dari mesin lokal:
  job `build` matrix (Ubuntu + Windows MSVC) menjalankan release build, smoke test
  binary (`--version`, `config path`), mengemas `tar.gz`/`zip` + `SHA256SUMS`, lalu
  mengunggahnya sebagai artefak.
- Saat tag `v*` di-push, job `release` mengunduh artefak CI tersebut dan
  melampirkannya ke GitHub Release (`anicli-x86_64-linux`,
  `anicli-x86_64-windows.exe`, `sourcecheck-*`, arsip, dan checksum).
- Trigger baru `workflow_dispatch` untuk build manual.

## [0.3.0] - 2026-11-15

### Ditambahkan

- **Unduhan di dalam aplikasi**: baris `download` tidak lagi hanya ditolak. Link file
  langsung (mis. Pixeldrain `api/file`, `dl.*`, atau berakhiran `.mkv/.mp4/.rar/…`)
  diunduh oleh aplikasi dengan **progress bar** di footer (persen, byte terunduh/total,
  path tujuan) dan disimpan ke `<data>/downloads/<source>/<judul> - <episode> - <label>`.
  Link halaman perantara (GoFile/Mega/MediaFire/AceFile/Terabox) **dibuka di browser**
  dengan pesan status yang jelas. URL halaman Pixeldrain dinormalisasi ke file langsung.
- **Kusonime kini mengekspos katalog batch**: satu episode sintetis "Batch (semua
  episode)" → 25–30 link unduhan (360p–1080p, MKV/MP4) dari beberapa host. Sebelumnya
  source ini hanya metadata sehingga tidak ada link unduhan yang bisa dipilih.
- `AppEvent::Download{Started,Progress,Done,Failed}` + `DownloadState` (persen, bar,
  format byte) dan dukungan `reqwest` fitur `stream` untuk unduhan bertahap.
- Test baru: unduhan end-to-end memakai server HTTP lokal (file tertulis benar +
  event progres), kegagalan jaringan, normalisasi Pixeldrain, dan state/progress UI.

### Diubah

- README: Kusonime sekarang berlabel **Download** (bukan sekadar metadata), plus bagian
  cara kerja unduhan dan troubleshooting.

## [0.2.0] - 2026-11-15

### Ditambahkan

- **KotakAnime** (playable): katalog HTML + player AJAX multipart `player_ajax`
  (nonce per halaman, `data-post`/`data-nume`/`data-type`), lalu decode halaman embed
  (base64 + XOR dengan **rumus key yang berubah tiap render**, dideteksi otomatis)
  menjadi **HLS direct** 480p/720p/1080p yang bisa langsung diputar mpv.
  Verifikasi live: search/detail/7 episode → 3 stream, 1 Direct siap mpv.
- HTTP client: dukungan `POST multipart/form-data` (dibutuhkan AJAX KotakAnime).

### Diubah

- Default config kini **7 source** (samehadaku, otakudesu, oploverz, animeindo, anoboy,
  kotakanime, kusonime) + fallback order diperbarui; `config.example.toml` ikut.
- **README ditulis ulang dalam bahasa Inggris** dan dirapikan: hanya source yang
  benar-benar terpasang yang didokumentasikan. Source mati atau download-only
  (AWSubs, Doronime, Neonime, Hunter No Sekai) dihapus dari daftar.
- Roadmap source #11 dinyatakan selesai untuk source yang layak streaming.

## [0.1.5] - 2026-11-15

### Diperbaiki

- **Windows (final):** penulisan file konfigurasi/persistensi tidak lagi memakai
  temp+rename (yang terus gagal `Access is denied` karena handle antivirus/Defender).
  Di Windows file ditulis langsung ke tujuan (truncate + fsync); Linux/macOS tetap
  atomik via rename. CI Windows kini bisa hijau.

## [0.1.4] - 2026-11-15

### Diperbaiki

- Windows CI: refaktor finalisasi tulisan atomik ke helper `finalize_write`
  (menghilangkan `needless_return` yang membuat clippy Windows merah). Perilaku
  retry + fallback copy tetap sama dengan 0.1.3.

## [0.1.3] - 2026-11-15

### Diperbaiki

- **Windows (final):** `write_atomic` kini memakai retry dengan backoff + fallback
  salin isi, karena `rename` bisa gagal sementara (`Access is denied`) akibat file
  baru masih dipindai antivirus/Defender. Simpan config/bookmark/riwayat di Windows
  berhasil; CI Windows hijau.

## [0.1.2] - 2026-11-15

### Diperbaiki

- **Windows (lanjutan):** `write_atomic` gagal `Access is denied` karena handle file
  `.tmp` masih terbuka saat `rename`. Handle kini ditutup lebih dulu, lalu file tujuan
  (jika ada) dihapus sebelum rename. Ini memperbaiki simpan config/bookmark/riwayat
  di Windows; CI Windows kini hijau.

## [0.1.1] - 2026-11-15

### Diperbaiki

- **Windows:** `write_atomic` kini menghapus file tujuan sebelum `rename`
  (`fs::rename` Windows tidak bisa menimpa file yang ada). Sebelumnya simpan config/
  bookmark/riwayat kedua kali gagal — ditemukan oleh CI Windows.
- **Player:** stderr mpv/vlc sekarang ditangkap dan **exit code non-zero dilaporkan
  sebagai error** ("mpv gagal diputar (exit 2): …"), bukan lagi "selesai diputar".
  Ringkasan stderr ditampilkan di banner error TUI.
- **Pemilihan stream:** tombol `p`/`Enter` memutar **baris yang di-highlight**, bukan
  selalu link "terbaik". Baris download-only ditolak dengan pesan jelas (sebelumnya
  memilih baris download tetap memutar link Direct lain — membingungkan).
- Daftar stream otomatis mem-highlight link terbaik; `r` memindahkan highlight ke
  resolusi pilihan.
- Guard agar `p` tidak men-spawn pemutar ganda saat mpv masih berjalan.
- CI: tambah workflow GitHub Actions (fmt/clippy/test/release + MSRV 1.88), gate test
  player khusus Unix, dan perbaikan unused import di Windows.

## [0.1.0] - 2026-11-15

Rilis publik pertama. Aplikasi TUI Rust untuk mencari, memilih episode/resolusi, lalu
memutar anime subtitle Indonesia ke **mpv** (fallback **vlc**), dengan fallback otomatis
antar source.

### Ditambahkan

- **TUI ratatui** — layar Search, Browse, Detail, Episodes, Streams, History, Bookmarks,
  Logs, Settings, dan Help; event loop async (HTTP tidak pernah memblokir render),
  status fallback per source, spinner, error banner, keymap lengkap.
- **6 source terpasang**:
  - **Samehadaku** — REST `wp-json/apk` + HTML; IceFile HLS/MP4 (1080/720/360) dan
    Blogspot (`googlevideo`).
  - **Otakudesu** — player AJAX/nonce, desustream → MP4 `cdn.odcloud.net`.
  - **Oploverz** — JSON API `backapi.oploverz.ac`; stream upbolt/filedon (MP4
    presigned)/Blogger.
  - **Animeindo** — `a.server[data-video]` → xtwap → `googlevideo` MP4.
  - **Anoboy** — iframe Blogger → MP4; host JS dikembalikan sebagai `Embed`.
  - **Kusonime** — metadata + katalog batch (download-only, tanpa stream).
- **Fallback lintas source berbasis identitas** (judul + nomor episode, bukan URL),
  timeout per source, jejak percobaan yang tampil di UI, dan **pencarian paralel**
  dengan dedupe judul.
- **Pemilihan resolusi** — exact/terdekat (naik/turun), `prefer_mkv`, picker resolusi
  (`r`), dan pemutaran otomatis `Direct` > `Embed`.
- **Player** — deteksi `mpv`/`vlc`/`yt-dlp`, argumen header (User-Agent/Referer),
  `--start`, cache, `--ytdl` untuk embed, mode detached/foreground, fallback browser.
- **Konfigurasi TOML persisten** — auto-create dari template berkomentar, validasi
  multi-error, migrasi `config_version`, simpan atomik, override CLI/env
  (`ANICLI_*`).
- **Riwayat tonton & bookmark** — JSON atomik, dedupe, batas entri, layar `H`/`B`,
  rekam otomatis saat memutar.
- **Logging** — file harian dengan timestamp lokal, ring buffer untuk layar Logs,
  redaksi field sensitif, pruning `max_days`.
- **Diagnostik** — binary `sourcecheck` untuk verifikasi live per source, fixture HTML
  nyata untuk test offline, mode `--mock`.
- **Dokumentasi** — `README.md` (build Linux/Windows, prasyarat, keymap, config,
  menambah source, batasan hukum), `DESIGN.md` (arsitektur, kontrak trait, alur
  fallback, recon per source), `LICENSE` (MIT), `config.example.toml`.
- **CI GitHub Actions** — matrix Ubuntu/Windows, `fmt`/`clippy -D warnings`/test/release,
  plus job MSRV 1.88.

### Diperbaiki

- Ekstraksi embed **Blogger** yang sebelumnya menghasilkan URL pemutar (bukan media):
  kini unescape HTML/JS bertingkat lalu menargetkan URL `googlevideo`.
- Array `SOURCES` IceFile diakhiri `],` (bukan `];`) — ditambahkan bracket-matching
  sadar-string.
- Parser Otakudesu menangani `ul.m720p` yang tidak ditutup, label unduhan bervariasi
  (`360p`/`Mp4 360p`/`MKV 1080p`), dan soft-404.
- Detail Oploverz (respons API dibungkus `{"data": {...}}`).
- Animeindo: pemisahan parser home (`div.list-anime`) vs search (`table.otable`),
  urutan episode kronologis, poster lazy-load `data-original`.
- Anoboy: judul seri tidak lagi menggandakan judul episode; wajib `type=anime` pada
  `/series/`.
- `reqwest 0.13` memakai fitur TLS `rustls` (bukan `rustls-tls`) + `webpki-roots`.

### Diketahui belum ada (roadmap)

- **KotakAnime** — recon memastikan playable (HLS via AJAX player + decode embed XOR);
  adapter belum diimplementasikan.
- **AWSubs** dan **Doronime** — batch/download-only (belum ada adapter).
- **Neonime** — katalog identik dengan Anoboy/Animeindo (alias), **Hunter No Sekai** —
  domain tidak aktif; keduanya tidak diimplementasikan.
- Build Windows belum diuji di mesin Windows asli (CI disiapkan).
- Download offline, image preview (feature flag), edit config inline di TUI, resume
  posisi via mpv IPC.
- Beberapa source membatasi data (mis. Samehadaku memotong daftar episode detail di
  100 baris; Otakudesu tanpa mirror 1080p).

### Verifikasi rilis

- `cargo test` → 208 test (158 unit/lib + 11 CLI + 14 HTTP lokal + 6 registry +
  19 TUI render), 0 gagal.
- `cargo clippy --all-targets -- -D warnings` dan `cargo fmt --check` bersih.
- `cargo build --release` sukses (Linux x86_64).
- Live: URL HLS/MP4 nyata diputar `mpv --frames=1` (exit 0); TUI berjalan di pty dan
  keluar bersih.

**Batasan hukum:** proyek ini untuk penggunaan pribadi/edukasi. Semua konten milik
masing-masing penyedia; tidak meng-host/mendistribusikan video, tidak mem-bypass
DRM/paywall. Lihat `README.md`.
