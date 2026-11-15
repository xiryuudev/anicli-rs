# Changelog

Semua perubahan penting pada proyek ini didokumentasikan di file ini.
Format mengikuti [Keep a Changelog](https://keepachangelog.com/id/1.1.0/) dan
[Semantic Versioning](https://semver.org/lang/id/).

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
