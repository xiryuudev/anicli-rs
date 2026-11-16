# anicli-rs di Termux (Android)

Panduan build native + install + pemutaran video lewat aplikasi Android.
Target: **Termux dari F-Droid/GitHub** (versi Play Store tidak dirawat), Android 10+,
arsitektur **aarch64/arm64**.

> Ringkas: build dilakukan **di dalam Termux** memakai paket `rust` Termux, yang
> host-nya sudah `aarch64-linux-android` (Bionic). **Tidak perlu Android NDK,
> tidak perlu `--target`, tidak perlu cmake/NASM** — project ini memakai provider
> TLS `ring`, bukan `aws-lc-rs`.

## 1. Siapkan Termux

```bash
pkg update && pkg upgrade -y
pkg install -y rust clang perl make git termux-tools
```

Cek toolchain:

```bash
rustc -vV | grep host      # harus mengandung "android"
clang --version | head -1
```

Pasang aplikasi Android (dari **F-Droid**):

- **mpv-android** — `is.xyz.mpv` (disarankan, mendukung HLS/m3u8)
- **VLC** — `org.videolan.vlc` (opsional)
- Browser default — fallback untuk link embed

> `pkg install vlc` memasang VLC **terminal**, bukan VLC Android. Jangan dipakai.

## 2. Pasang binary

### Opsi A — unduh binary CI (disarankan, tanpa build)

Binary `aarch64-android` dibangun otomatis oleh GitHub Actions (NDK, Bionic) dan
dilampirkan ke setiap Release:

```bash
pkg install -y curl
# aset tanpa versi: selalu menunjuk rilis terbaru
curl -L -o "$PREFIX/bin/anicli" \
  https://github.com/xiryuudev/anicli-rs/releases/latest/download/anicli-aarch64-android
curl -L -o "$PREFIX/bin/sourcecheck" \
  https://github.com/xiryuudev/anicli-rs/releases/latest/download/sourcecheck-aarch64-android
chmod +x "$PREFIX/bin/anicli" "$PREFIX/bin/sourcecheck"
anicli --version

# verifikasi (opsional): AArch64 + linker64
readelf -h "$PREFIX/bin/anicli" | grep Machine
readelf -l "$PREFIX/bin/anicli" | grep interpreter
```

### Opsi B — build di perangkat

```bash
pkg install -y rust clang perl make git termux-tools
cd ~
git clone https://github.com/xiryuudev/anicli-rs
cd anicli-rs
bash scripts/build-termux.sh          # build + install ke $PREFIX/bin/anicli
```

Atau manual:

```bash
cargo build --release --locked
install -Dm755 target/release/anicli "$PREFIX/bin/anicli"
install -Dm755 target/release/sourcecheck "$PREFIX/bin/sourcecheck"
```

Verifikasi binary (harus ELF aarch64, interpreter `/system/bin/linker64`):

```bash
file target/release/anicli
readelf -h target/release/anicli | grep Machine     # AArch64
readelf -l target/release/anicli | grep interpreter # /system/bin/linker64
anicli --version
```

Script juga membuat alias `anicli-rs` di `$PREFIX/bin/`, jadi kedua nama bisa dipakai.

## 3. Konfigurasi

`anicli` membuat config otomatis saat pertama jalan:

```bash
anicli config path     # ~/.config/anicli-rs/config.toml
anicli config init     # kalau ingin membuat ulang dari template
```

Bagian player Android:

```toml
[playback]
player = "mpv"            # dipakai di desktop; di Termux otomatis jalur Android
android_player = "mpv"    # "mpv" | "vlc" | "auto"
embed_strategy = "browser" # embed di Android selalu dibuka di browser
```

- `auto` → intent `VIEW` biasa; Android memilih aplikasi default.
- `mpv` → intent difilter ke `is.xyz.mpv` (fallback tetap ada bila tidak terpasang).
- `vlc` → intent difilter ke `org.videolan.vlc`.

## 4. Pakai

```bash
anicli          # TUI: cari -> pilih episode -> pilih stream -> p
```

Di Termux, menekan `p` **tidak** men-spawn mpv terminal; aplikasi menyerahkan URL
ke player Android lewat intent, dengan urutan fallback:

1. `am start -a android.intent.action.VIEW -d <url> -t <mime> [-p <paket>]`
2. `termux-am` / `termux-am-starter` (wrapper socket Termux)
3. `termux-open --content-type <mime> <url>`
4. `termux-open-url <url>`

Jika semuanya gagal, banner error menampilkan daftar percobaan + exit code.

Catatan: aplikasi player berjalan di luar Termux; TUI boleh ditutup setelah video
mulai diputar. HLS diputar oleh player Android (bukan relay di Termux).

## 5. mpv.conf / input.conf — perlu diubah?

**Singkat: tidak wajib, dan `~/.config/mpv/mpv.conf` di Termux tidak dipakai** oleh
anicli. Di Termux, anicli **tidak men-spawn `mpv`** (paket Termux itu tanpa GUI); URL
diserahkan ke aplikasi Android lewat intent. Jadi konfigurasi mpv Termux/`input.conf`
tidak berpengaruh ke playback anicli.

Yang relevan adalah **config milik mpv-android**, di:
`/storage/emulated/0/Android/data/is.xyz.mpv/files/mpv.conf`
(bisa diedit dari menu aplikasi → *Advanced* → *Edit mpv.conf*; `input.conf` ada di
folder yang sama untuk gesture/tombol di dalam player).

Kenapa perlu diubah? Intent `VIEW` **tidak bisa membawa argumen mpv per-file**, jadi
opsi yang biasanya dikirim anicli di desktop (`--force-media-title`, `--user-agent`,
`--referrer`, `--cache`, `--ytdl`) **tidak ikut** ke Android. Kalau CDN menolak,
tambahkan di mpv-android:

```ini
# mpv-android: /storage/emulated/0/Android/data/is.xyz.mpv/files/mpv.conf
user-agent=Mozilla/5.0 (Linux; Android 13; Pixel 7) AppleWebKit/537.36 Chrome/126.0 Safari/537.36
cache=yes
cache-secs=30
demuxer-max-bytes=150MiB
```

Header Referer bersifat **global** — hanya tambahkan bila kamu tahu source yang
dipakai membutuhkannya, dan hapus lagi kalau source lain jadi rusak:

```ini
# contoh; ganti host sesuai source. Global untuk semua playback!
http-header-fields="Referer: https://samehadaku.example/"
```

`input.conf` **tidak perlu diubah** untuk anicli: itu hanya pemetaan tombol di dalam
mpv-android (gesture, tombol TV), bukan keybinding TUI anicli.

Di desktop sebaliknya: mpv membaca `mpv.conf`/`input.conf` sendiri dan anicli
menambahkan argumen CLI yang menimpa sebagian opsi
(`--force-media-title`, `--user-agent`, `--referrer`, `--cache`, `--ytdl`), jadi
tidak ada yang wajib diubah. Tambahan argumen bisa lewat `[playback] extra_args`.

Untuk VLC Android: tidak ada file config yang perlu diubah; atur di menu aplikasi.

## 5. Troubleshooting

| Gejala | Sebab / solusi |
|--------|----------------|
| `error: bukan Termux` saat build script | Jalankan di Termux F-Droid; cek `echo $PREFIX` harus `.../com.termux/files/usr`. |
| `linker error: cannot find -l...` | `pkg install clang` lalu ulangi; pastikan `rust` Termux (bukan rustup desktop). |
| Build gagal di `ring`/`cc` | Pastikan `clang` dan `perl` terpasang, lalu `cargo clean -p ring && cargo build --release`. |
| `am: not found` | `pkg install termux-tools`; sebagian ROM perlu `termux-am` dari F-Droid. |
| Intent tidak membuka apa pun | Pasang mpv-android (`is.xyz.mpv`) dari F-Droid; coba `android_player = "auto"`. |
| Video tidak jalan, hanya embed | `embed_strategy = "browser"`; pilih baris `direct` di layar Stream. |
| Panic `rustls-platform-verifier ... android.rs` | Sudah diperbaiki di v0.4.3 (TLS memakai `webpki-roots` statis, bukan verifier Android). Unduh ulang binary rilis terbaru. |
| TUI berantakan | `stty rows 40 cols 100` atau perkecil font terminal Termux. |
| Config tidak tersimpan | v0.1.5+ sudah memperbaiki penulisan file di Windows/Android; pastikan versi terbaru. |

## 6. Update tanpa buka browser

Binary Android dibangun CI dan dilampirkan tiap rilis, jadi update cukup satu perintah:

```bash
anicli update --check   # cek versi (tidak mengubah apa pun)
anicli update           # unduh + pasang otomatis ke $PREFIX/bin/anicli
```

Updater memakai aset `anicli-aarch64-android` dari rilis terbaru, menaruh unduhan di
`~/.local/share/anicli-rs/update/`, lalu mengganti `$PREFIX/bin/anicli`.
Di TUI, saat start ada pengecekan latar; bila versi baru tersedia muncul toast satu
baris di footer: `↑ versi 0.6.0 tersedia — jalankan \`anicli update\`` (hilang sendiri).
Matikan dengan `[update] check_on_start = false` di config.

Jika `$PREFIX/bin` tidak bisa ditulis (jarang terjadi di Termux), updater mencetak
perintah `curl` manual yang bisa kamu jalankan sendiri.

## 6. Alternatif: proot-distro

Hanya jika build native gagal:

```bash
pkg install proot-distro
proot-distro install debian
proot-distro login debian
# di dalam Debian: apt install build-essential pkg-config libssl-dev cmake \
#   && curl ... rustup ... && cargo build --release
```

Binary hasil proot-distro tetap harus dijalankan **di dalam proot**; jalur ini tidak
memberi binary Bionic untuk Termux dan tidak bisa `am start` dengan mudah.

## 7. Catatan rilis

- Tidak ada binary Android resmi di GitHub Release; build di perangkat adalah jalur
  resmi (CI hanya membangun Linux & Windows).
- `ring` dipilih supaya tidak perlu cmake/NASM; ukuran binary lebih kecil daripada
  build `aws-lc-rs`.
