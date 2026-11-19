# anicli-rs

[![CI](https://github.com/xiryuudev/anicli-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/xiryuudev/anicli-rs/actions/workflows/ci.yml)
[![Latest release](https://img.shields.io/github/v/release/xiryuudev/anicli-rs)](https://github.com/xiryuudev/anicli-rs/releases)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

A terminal UI (TUI) for watching Indonesian-subtitled anime with **mpv** (and **vlc**
as a fallback). Search across several sources, pick an episode and resolution, and
play — with automatic fallback between sources when one fails.

> Personal/educational project. It does not host or redistribute video; it only
> resolves links from public sites and hands them to your local player.

## Features

- **TUI** built with `ratatui`/`crossterm`: Search, Browse, Detail, Episodes, Streams,
  History, Bookmarks, Logs, Settings, and Help screens.
- **9 sources** with a shared trait (`AnimeSource`), so a new site is a small module.
- **Language metadata + filter**: every source exposes a `Language` (`[ID]`, `[EN]`,
  extensible). Press `Y` for the **Bahasa** popup (rolling selection) to filter both
  the source list and search; the choice persists in
  `[general] language_filter = "all" | "id" | "en" | "jp"`. Adding a source with a new
  language needs no UI changes.
- **Cross-source fallback** based on *identity* (title + episode number), not URLs:
  if a source fails, the app tries the next one from your fallback order.
- **Streaming search**: results appear per source as soon as that source responds
  (no waiting for the slowest), merged in priority order with title de-duplication,
  a `[source]` badge on every result, per-source error summary, and an `S` toggle to
  search **only the active source** (`general.search_scope = "all" | "selected"`).
  `Esc` while searching cancels all in-flight source tasks.
- **Source selector**: `s` opens a filterable popup listing every configured source
  (active `●`, enabled `○`, disabled/unavailable), keyboard driven.
- Results keep a deterministic order: the active source is always first.
- **Resolution selection**: exact or nearest resolution, HLS/MP4 direct links, and
  `Embed` links played through `yt-dlp` or opened in the browser.
- **Player integration**: `mpv` or `vlc`, headers (User-Agent/Referer), `--start`,
  embed strategy (`yt-dlp` / browser), foreground or detached.
- **Downloads**: selecting a `download` row downloads the file inside the app with a
  live progress bar (speed-agnostic byte counter, saved to
  `<data>/downloads/<source>/`). Hosts that only expose an intermediate page
  (GoFile, Mega, MediaFire, AceFile…) open in your browser instead; Pixeldrain pages
  are normalised to direct file URLs.
- **Persistence**: TOML config (commented template, validation, migrations, atomic
  writes), watch history, bookmarks, rotating log files.
- **Self-update**: `anicli update` (or `-U`) checks GitHub Releases, verifies the
  platform asset, downloads it and replaces the running binary — on Linux, Windows
  and Termux. The TUI shows a non-intrusive one-line toast in the footer when a newer
  version exists (cached, checked at most every `[update] check_interval_hours`).
- **Diagnostics**: `sourcecheck` binary for live source verification, real HTML
  fixtures for offline tests, `--mock` mode for a network-free demo.
- **CI** on Ubuntu and Windows (native MSVC) plus an MSRV (1.88) job.

## Supported sources

| Source | Kind | Notes |
|--------|------|-------|
| Samehadaku | Streaming | IceFile HLS/MP4 (1080p/720p/360p) and Blogspot fallback |
| Otakudesu | Streaming | AJAX player → desustream → direct MP4 |
| Oploverz | Streaming | JSON API (`backapi.oploverz.ac`); presigned MP4 / embeds |
| Animeindo | Streaming | `a.server[data-video]` → xtwap → `googlevideo` MP4 |
| Anoboy | Streaming | Blogger iframe → MP4; JS-only hosts are exposed as `Embed` |
| KotakAnime | Streaming | HLS (480p/720p/1080p) via AJAX player + decoded embed page |
| Anichin [ID] | Streaming | Donghua sub Indo; search, detail, episode list, player embed (`rmb.anichin.bio`) |
| Ani.pm [EN] | Catalog | English; JSON API search + LD+JSON detail + 1…N episode URLs; SPA player not implemented |
| Kusonime | Download | Batch page per series: 25–30 host links (360p–1080p, MKV/MP4); no streaming |

Sources that are download-only, duplicate catalogs, or defunct (AWSubs, Doronime,
Neonime, Hunter No Sekai) are intentionally **not** included.

## Requirements

- Rust **1.88+** (edition 2021)
- `mpv` (recommended) or `vlc`
- `yt-dlp` for `Embed` links (optional but recommended)
- Linux or Windows (x86_64), or Android via Termux (aarch64)

## Build

```bash
git clone https://github.com/xiryuudev/anicli-rs
cd anicli-rs
cargo build --release
./target/release/anicli
```

Windows (MSVC) notes:

- `reqwest`'s `rustls` backend builds `aws-lc-rs`, which needs **CMake** and **NASM**
  on `PATH` (`choco install cmake nasm -y` works on CI runners).
- Release artifacts are built by CI for `x86_64-pc-windows-msvc`.

## Install a prebuilt binary (built by CI)

Every release is built and packaged by GitHub Actions on **Ubuntu** and **Windows
(MSVC)** — no locally-built binaries are published.

```bash
# Linux x86_64 — single binary (also available as .tar.gz in the release)
curl -L -o anicli \
  https://github.com/xiryuudev/anicli-rs/releases/latest/download/anicli-x86_64-linux
chmod +x anicli && ./anicli

# Windows x86_64 — download from the release page:
# https://github.com/xiryuudev/anicli-rs/releases/latest
#   anicli-x86_64-windows.exe  (or the .zip with both binaries)
```

Each release ships: `anicli-rs-v<version>-x86_64-linux.tar.gz`,
`anicli-rs-v<version>-x86_64-windows.zip`, per-file binaries
(`anicli-x86_64-linux`, `anicli-x86_64-windows.exe`, `sourcecheck-*`) and
`SHA256SUMS-*.txt` — all built and attached by CI.

**Bleeding edge (not yet tagged):** open the latest green
[CI run](https://github.com/xiryuudev/anicli-rs/actions/workflows/ci.yml), scroll to
**Artifacts**, and download `anicli-rs-x86_64-linux` / `anicli-rs-x86_64-windows`.

## Usage

```bash
anicli                 # TUI
anicli --no-tui        # print a short config/registry summary and exit
anicli --source otakudesu --query "one piece"   # one-shot resolve (no TUI)
anicli config init     # create the config from the commented template
anicli config path     # print config/data/log locations
anicli update --check  # is a newer release available?
anicli update          # download & install the latest release (alias: anicli -U)
```

First run creates the config automatically:
`~/.config/anicli-rs/config.toml` (Linux) or `%APPDATA%\anicli-rs\` (Windows).
Data (history, bookmarks) and logs live next to it in the data directory
(`~/.local/share/anicli-rs/` on Linux).

## Keybindings

| Key | Action |
|-----|--------|
| `/` | Focus search |
| `Tab` | Move to results / next pane |
| `Enter` | Open selection / fetch streams |
| `j` / `k`, `↓` / `↑` | Move selection |
| `p` | Play highlighted stream (mpv/vlc) — or start/browse a download |
| `r` | Pick resolution (highlight follows) |
| `s` | Open source selector (type to filter, Enter to choose) |
| `S` | Toggle search scope: all sources / active source only |
| `Y` | Language filter popup (all / ID / EN / JP) — filters source list & search |
| `l` | Browse (ongoing/completed) |
| `H` | History |
| `B` | Bookmarks |
| `L` | Logs |
| `g` | Settings |
| `b` / `d` | Bookmark / download-only action |
| `Esc` | Back / clear |
| `q` | Quit (also `Ctrl+C`) |
| `?` | Help |

## Android / Termux

`anicli` runs inside **Termux** (F-Droid build) on aarch64 and hands video to
**mpv-android** or **VLC Android** — Termux's own `mpv` package has no GUI, so the app
launches the Android player through an intent.

```bash
# Option A (recommended): download the CI-built Android binary (no build needed)
pkg install -y curl
curl -L -o "$PREFIX/bin/anicli" \
  https://github.com/xiryuudev/anicli-rs/releases/latest/download/anicli-aarch64-android
curl -L -o "$PREFIX/bin/sourcecheck" \
  https://github.com/xiryuudev/anicli-rs/releases/latest/download/sourcecheck-aarch64-android
chmod +x "$PREFIX/bin/anicli" "$PREFIX/bin/sourcecheck"

# Option B: build on device (rust + clang + perl + make + git)
git clone https://github.com/xiryuudev/anicli-rs && cd anicli-rs
bash scripts/build-termux.sh
```

`anicli` → press `p` on a stream → opens mpv-android.

- No NDK, no `--target`, no cmake/NASM: Termux's `rust` already targets Bionic, and
  TLS uses the **ring** provider (not `aws-lc-rs`).
- Launch order: `am start` intent (+ `-p is.xyz.mpv` / `-p org.videolan.vlc` when
  preferred) → `termux-am` → `termux-open` → `termux-open-url`.
- **Headered links go through a local proxy.** Android intents cannot carry HTTP
  headers, which broke Blogger (`Referer: https://www.blogger.com/`) and desustream
  (`User-Agent`). When a link needs headers or is HLS, anicli starts a proxy on
  `127.0.0.1` that injects the headers, forwards `Range`/`Content-Length`, and
  rewrites HLS playlists — then hands mpv-android the proxy URL.
- Set the player with `[playback] android_player = "mpv" | "vlc" | "auto"`.
- Full walkthrough + troubleshooting, plus what (not) to change in `mpv.conf` /
  `input.conf`: **[docs/TERMUX.md](docs/TERMUX.md)**.

> On Termux the app does **not** spawn `mpv`; it sends an intent to mpv-android. Your
> Termux `~/.config/mpv/mpv.conf` and `input.conf` are therefore unused. If a CDN
> needs a User-Agent/Referer, set it in **mpv-android's own** `mpv.conf`
> (`Android/data/is.xyz.mpv/files/mpv.conf`) — intent cannot pass per-file mpv args.

## Configuration

```toml
config_version = 1

[general]
default_source = "samehadaku"
default_resolution = "720p"
fallback_order = [
  "samehadaku", "otakudesu", "oploverz", "animeindo", "anoboy", "kotakanime", "kusonime",
]

[playback]
player = "mpv"                 # mpv | vlc
embed_strategy = "yt-dlp"      # yt-dlp | browser
foreground = false

[ui]
language = "id"

[http]
user_agent = "Mozilla/5.0 ..."
rate_limit_per_host_ms = 500
retry_max = 3

[update]
check_on_start = true      # non-blocking check; shows a footer toast if newer
check_interval_hours = 12  # cache window for the check

[sources.kotakanime]
enabled = true
base_url = "https://s13.nontonanimeid.boats"   # host rotates; update if it moves
mirrors = []
```

See `config.example.toml` for every field, including logging, history limits, and
mirror lists.

## Updating

```bash
anicli update --check   # just report: current vs latest
anicli update           # download + install (same as anicli -U)
```

The updater reads `https://api.github.com/repos/xiryuudev/anicli-rs/releases/latest`,
picks the asset for your platform
(`anicli-x86_64-linux`, `anicli-x86_64-windows.exe`, `anicli-aarch64-android`),
downloads it to `<data>/update/`, and swaps the running binary.

| Platform | Behaviour |
|----------|-----------|
| Linux | staged copy + `rename` over the binary; needs a writable install dir (use `~/.local/bin`, not `/usr/bin`, or run with sudo) |
| Termux (Android) | same as Linux — installs into `$PREFIX/bin/anicli` |
| Windows | the running `.exe` is locked, so the new file is written next to it (`anicli.new.exe`) and the exact `move /Y` command is printed |

If the install directory is not writable the updater prints the manual command
instead of failing silently. All of this is also available as `--check` only, and can
be disabled with `[update] check_on_start = false`.

## How it works

1. **Search** fans out across enabled sources and de-duplicates by normalized title.
2. **Detail/episodes** are parsed with `scraper` selectors in pure functions that are
   unit-tested against saved HTML fixtures.
3. On play, the **episode identity** (title + number) is resolved on the chosen
   source; if it fails, the next source in `fallback_order` is tried.
4. Streams are ranked (`Direct` before `Embed`, preferred resolution, `prefer_mkv`)
   and handed to the player. `Embed` links use `yt-dlp` when available, otherwise the
   browser.
5. Player failures (non-zero exit) are surfaced with the stderr tail instead of a
   silent "finished".

## Adding a source

1. Create `src/sources/<name>/` with `mod.rs`, `client.rs`, `parse.rs`.
2. `parse.rs`: pure functions with tests that load fixtures from `tests/fixtures/`.
3. `client.rs`: base URL, `MirrorSet` marker, HTTP calls, stream resolution.
4. Implement `AnimeSource`, override `language()` if not Indonesian, and expose
   `NAME`, `build`, `factory`.
5. Register the factory in `src/sources/mod.rs`.
6. Add a `[sources.<name>]` block to `config.example.toml` and `Config::default()`.
7. Verify with `cargo run --bin sourcecheck -- --source <name> --query "<title>"`.

## Testing

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test                    # 200+ tests, offline fixtures, TUI TestBackend
cargo run --bin sourcecheck -- --source animeindo --query "black clover"
```

CI runs these on Ubuntu and Windows, plus a MSRV 1.88 job. Two extra CI jobs build the
release binaries (Linux `tar.gz`, Windows `zip`) and, for `v*` tags, attach them to the
GitHub release automatically.

## Project layout

```
src/
  sources/        # one module per site (parse.rs pure + client.rs + mod.rs trait impl)
  ui/             # app state, event loop, render, theme
  http.rs         # rate limit, retry, mirrors, soft-404, multipart
  fallback.rs     # identity-based cross-source resolution
  resolve.rs      # stream ranking
  player.rs       # mpv/vlc/yt-dlp spawning and diagnostics
  config.rs       # TOML load/save, validation, migrations, atomic writes
  history.rs      # watch history
  bookmarks.rs    # bookmarks
  logging.rs      # file logging + in-TUI ring buffer
tests/            # CLI/HTTP/registry/TUI integration tests + HTML fixtures
research/         # recon notes per source (selectors, endpoints, fixtures)
docs: DESIGN.md, CHANGELOG.md, config.example.toml
```

## Troubleshooting

- **Embed link fails to play**: hosts that need a browser session cannot be resolved
  by `yt-dlp`. Set `playback.embed_strategy = "browser"` or pick a `Direct` link.
- **Source stopped working / domain moved**: update `[sources.<name>].base_url` (or
  add a mirror) in the config; mirror probing is built in.
- **Windows: config not saving**: fixed in 0.1.5; make sure you run a current build.
- **Player exits immediately**: the error banner now shows the player's stderr tail.
- **Downloads**: `download` rows either download in-app (progress bar in the footer,
  file under `<data>/downloads/`) or open in the browser when the host needs a page
  and a captcha/JS flow.

## Legal

This project is for personal, educational use only. All content is owned by the
respective sites and rights holders. The project does not host, store, or distribute
video, and does not bypass DRM or paywalls. Respect the terms of service and the laws
in your country; you are responsible for how you use it.

## License

MIT — see [LICENSE](LICENSE).
