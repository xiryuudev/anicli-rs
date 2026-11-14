# anicli-rs

> TUI streaming anime subtitle Indonesia — cari, pilih episode & resolusi, lalu putar
> ke **mpv** (fallback **vlc**). Modular: tiap situs penyedia adalah adapter terpisah.

Proyek ini adalah penulisan ulang [`anicli-indo`](https://github.com/xiryuudev/) (Python,
deprecated) dengan Rust + ratatui. Dirancang untuk **penggunaan pribadi/edukasi**.

---

## Fitur

| Fitur | Status |
|-------|--------|
| Pencarian paralel lintas source (dedupe judul) | ✅ |
| Browse ongoing / completed (per source, `l`) | ✅ (Samehadaku & Otakudesu) |
| Detail anime (sinopsis, genre, status, tahun, skor, episode) | ✅ |
| Daftar episode + navigasi | ✅ |
| Pemilihan resolusi (240p–1080p sesuai source) | ✅ `r` untuk ganti |
| Preferensi resolusi di config (`nearest_higher`/`nearest_lower`/`exact_only`) | ✅ |
| Ganti source (`s`) & fallback otomatis antar source | ✅ |
| Putar ke mpv/vlc dengan `Referer` + `User-Agent` | ✅ |
| Embed (iframe) via mpv `--ytdl` / browser | ✅ (`embed_strategy`) |
| Config TOML persisten + auto-create + layar Setting (`g`, baca-saja) | ✅ |
| Logging file harian + layar Logs (`L`) | ✅ |
| Source terpasang | ✅ Samehadaku, Otakudesu, **Oploverz**, **Animeindo**, **Anoboy**, Kusonime |
| Source tambahan dari roadmap | ⏳ lihat "Status source" di bawah |
| Riwayat tonton (`H`) & bookmark (`B`, toggle `b` di detail) | ✅ |
| Source tambahan (Anoboy, Oploverz, Neonime, dll.) | ⏳ (Tahap 11) |
| Download offline | ⏳ (hook disiapkan; v1 tidak) |
| Image preview poster | ⏳ (feature flag `image-preview`) |

---

## Status source

| Source | Status | Catatan |
|--------|--------|---------|
| Samehadaku | ✅ playable | IceFile HLS/MP4 (1080/720/360) + Blogspot |
| Otakudesu | ✅ playable | desustream → MP4 `cdn.odcloud.net` |
| Oploverz | ✅ playable | API JSON (search/detail/episodes) + stream upbolt/filedon(presigned MP4)/blogger; live: 10 hasil, strea 1 direct |
| Animeindo | ✅ playable | server `xtwap` → googlevideo MP4; search `/search.php?q=` |
| Anoboy | ✅ playable (sebagian) | iframe Blogger → MP4; host gdplayer/turbovid butuh browser/mati |
| Kusonime | ✅ metadata | download-only (batch), tanpa streaming per episode |
| KotakAnime | 🔜 recon selesai (playable) | HLS via AJAX player + decode embed (XOR); belum diimplementasikan |
| AWSubs | 🔜 recon selesai | batch/download-only (Blogger + puristream opsional) |
| Doronime | 🔜 recon selesai | download-only (safelink/CSRF; robots melarang jalur media) |
| Neonime | ⚠️ katalog sama | `/neonime/` = alias katalog `animase.xyz` (mirip Anoboy/Animeindo) |
| Hunter No Sekai | ❌ tidak aktif | domain mati/landing; tidak diimplementasikan |

Detail recon per source (selector, endpoint, fixture) ada di folder `research/`.

## Prasyarat

- **Rust 1.88+** (edition 2021). Cek: `rustc --version`.
- **mpv** (disarankan) dan/atau **vlc** untuk memutar.
- **yt-dlp** (opsional) untuk link embed yang butuh ekstraktor.
- **cmake** + build tools C: dibutuhkan saat build karena `rustls` memakai `aws-lc-rs`
  (kode C). Di Windows: MSVC Build Tools + CMake (+ NASM bila diminta).
- Linux: tidak butuh OpenSSL dev (TLS pakai rustls).

Install player:

```bash
# Arch / Manjaro / CachyOS
sudo pacman -S mpv
# Debian / Ubuntu
sudo apt install mpv
# Windows
winget install mpv
# yt-dlp (opsional)
sudo pacman -S yt-dlp        # atau: pipx install yt-dlp
```

---

## Build & install

### Linux

```bash
git clone https://github.com/xiryuudev/anicli-rs.git
cd anicli-rs
cargo build --release
install -Dm755 target/release/anicli ~/.local/bin/anicli
anicli --version
```

Butuh `cmake`/compiler untuk `aws-lc-sys`: `sudo pacman -S cmake base-devel` (Arch)
atau `sudo apt install build-essential cmake` (Debian/Ubuntu).

### Windows (x86_64-pc-windows-msvc)

1. Install [rustup](https://rustup.rs) → toolchain default `stable-x86_64-pc-windows-msvc`.
2. Install **Visual Studio Build Tools** (workload "Desktop development with C++") dan
   **CMake** (`winget install Kitware.CMake`). Jika build meminta, install NASM
   (`winget install NASM.NASM`).
3. Install mpv (`winget install mpv`) dan pastikan `mpv.exe` ada di `PATH`.
4. Build:

```powershell
git clone https://github.com/xiryuudev/anicli-rs.git
cd anicli-rs
cargo build --release
copy target\release\anicli.exe "$env:LOCALAPPDATA\Programs\anicli\anicli.exe"
anicli --version
```

Kalau `mpv.exe` tidak ada di `PATH`, set `playback.mpv_path` di config.

### Build tanpa jaringan tes / offline

Tidak perlu internet untuk `cargo build` setelah dependensi ter-cache. Untuk verifikasi
parser, semua fixture disimpan di `tests/fixtures/` (offline).

---

## Menjalankan

```bash
anicli                      # buka TUI
anicli --source otakudesu   # mulai dengan source lain (sesi ini saja)
anicli --resolution 1080p   # override resolusi sesi ini
anicli --player vlc         # override player
anicli --no-tui             # cetak ringkasan config/log (untuk CI/pipe/SSH)
anicli config init          # tulis config.toml dari template berkomentar
anicli config path          # cetak lokasi config/data/log
anicli config show          # cetak config efektif (TOML)
```

Di lingkungan tanpa TTY, `anicli` otomatis mencetak ringkasan dan tidak menyalakan TUI.

### Keymap

| Tombol | Aksi |
|--------|------|
| `/` atau `Tab` | fokus input pencarian |
| `Enter` | cari → buka detail → pilih episode → ambil stream; di layar Stream: **putar** |
| `j` `k` / `↑` `↓` | navigasi daftar |
| `p` | putar link terbaik (mpv/vlc/browser untuk embed) |
| `r` | ganti resolusi (cycle yang tersedia) |
| `s` | ganti source aktif |
| `l` | browse ongoing/completed (Tab: ganti tab) |
| `b` (di detail) | toggle bookmark |
| `H` | riwayat tonton (Enter: putar ulang, `d`: hapus) |
| `B` | bookmark (Enter: buka, `d`: hapus) |
| `L` | log aplikasi (ring buffer, `j`/`k` scroll) |
| `g` | pengaturan efektif (baca-saja) |
| `Esc` | kembali / lepas fokus |
| `q` | keluar |
| `?` | bantuan |

Alur: `Search` → `Enter` (detail) → `Enter` (episode) → `Enter` (stream) → `p` (putar).

---

## Konfigurasi

Lokasi default:

| OS | Config | Data (riwayat, log) |
|----|--------|---------------------|
| Linux | `~/.config/anicli-rs/config.toml` | `~/.local/share/anicli-rs/` |
| Windows | `%APPDATA%\anicli-rs\config.toml` | `%LOCALAPPDATA%\anicli-rs\` |
| macOS | `~/Library/Application Support/anicli-rs/config.toml` | sama |

File dibuat otomatis dari template saat pertama jalan (`anicli config init` juga bisa).
Contoh lengkap berkomentar ada di [`config.example.toml`](config.example.toml).

Presedensi: **flag CLI > environment (`ANICLI_*`) > file config > default**.

Setting penting:

```toml
[general]
default_source = "samehadaku"
fallback_order = ["samehadaku", "otakudesu", "kusonime"]

[playback]
player = "mpv"                  # mpv | vlc
default_resolution = "720p"
resolution_policy = "nearest_higher"
embed_strategy = "ytdl"         # ytdl | browser | skip
foreground = false              # true = mpv pakai terminal, TUI suspend

[http]
request_delay_ms = 500          # sopan santun ke situs
jitter_ms = 300
max_concurrent = 6
timeout_secs = 20
retries = 2
user_agent = "Mozilla/5.0 ... Chrome/126 ..."

[sources.samehadaku]
enabled = true
base_url = "https://v2.samehadaku.how"
mirrors = []
```

**Domain berpindah?** Edit `base_url`/`mirrors` untuk source tersebut — tidak perlu
rebuild. Aplikasi memvalidasi domain dengan marker konten (bukan hanya status 200),
lalu menyimpan cache 1 jam.

---

## Menambah source baru

1. Riset endpoint/selector; simpan fixture HTML di `tests/fixtures/<nama>/`.
2. Buat `src/sources/<nama>/` dengan `mod.rs`, `client.rs`, `parse.rs`
   (`parse.rs` = fungsi murni, tanpa I/O, tanpa `unwrap`).
3. Implementasikan `AnimeSource` (lihat `src/sources/mod.rs`); isi `capabilities()` jujur.
4. Tulis test parser dari fixture (minimal search, detail, episode, stream/unduhan).
5. Daftarkan factory di `sources::factories()` dan blok `[sources.<nama>]` di
   `config.example.toml`.
6. Verifikasi live: `cargo run --bin sourcecheck -- --source <nama> --query "judul"`.
7. Update `DESIGN.md` §22 (recon) dan README ini.

Detail arsitektur, alur fallback, dan kontrak parser ada di [`DESIGN.md`](DESIGN.md).

---

## Arsitektur singkat

```
UI (ratatui, async event loop)  ──channel──▶  PlaybackService (fallback)  ──▶  sources/*
        ▲                                              │
        └──────────── AppEvent (hasil job) ────────────┘
                                                       ├─ resolve.rs  (pilih resolusi)
                                                       ├─ player.rs   (mpv/vlc/browser)
                                                       └─ http.rs     (rate limit, retry, mirror)
```

- Parser source adalah **fungsi murni** `&str → model`, dites dengan fixture offline.
- Fallback lintas source memakai **identitas** (judul + nomor episode), bukan URL.
- Semua request lewat `HttpClient` dengan jeda per host, retry terbatas, dan deteksi
  anti-bot; satu TUI loop tidak pernah menunggu HTTP.

---

## Verifikasi & pengembangan

```bash
cargo test          # unit + CLI + HTTP (server lokal) + registry + TUI render
cargo clippy --all-targets -- -D warnings
cargo fmt --check
cargo run --bin sourcecheck -- --source samehadaku --query "naruto"
```

`sourcecheck` mencetak alur search → detail → episode → stream untuk satu source
(berguna saat situs berubah; error parse menyebut field yang gagal).

---

## Batasan & catatan hukum

- Proyek ini **untuk penggunaan pribadi/edukasi**. Semua konten milik masing-masing
  penyedia; proyek ini tidak meng-host, menyimpan, atau mendistribusikan video.
- Tidak ada bypass DRM/paywall/Cloudflare challenge. Kalau source memblokir, aplikasi
  melaporkan dan mencoba source lain (atau memakai `cookie_header` milik user sendiri).
- Scraping mengikuti `robots.txt` yang terverifikasi dan memakai jeda antar request.
  Untuk situs yang ToS-nya melarang scraping, gunakan atas risiko sendiri.
- **Samehadaku** memblokir User-Agent kosong/generik; jangan mengganti `http.user_agent`
  dengan nilai aneh.
- **Otakudesu**: mirror alternatif sebagian berisi embed JS; v1 memakai player default
  (`#pembed` → desustream → MP4).
- **Kusonime** adalah katalog **download-only** (tidak ada streaming per episode) —
  aplikasi menandainya jujur dan tidak menawarkan tombol putar.
- Daftar episode di halaman detail Samehadaku dibatasi 100 baris oleh situsnya.
- Link media beberapa host terikat IP & kedaluwarsa; aplikasi me-resolve tepat sebelum
  memutar, bukan menyimpan URL media.

---

## Lisensi

MIT — lihat [`LICENSE`](LICENSE).
