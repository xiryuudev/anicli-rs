# Neonime — Technical Reconnaissance Report

**Target:** Neonime (anime sub Indo)
**Purpose:** offline-testable DOM/endpoint contract for a Rust `scraper` parser
**Recon date:** 2026-11-15 (Asia/Jakarta). Every claim below comes from a 200 response saved in `research/raw/neonime/`.
**Verdict in one line:** Neonime is **not a site** — it is a WordPress *page* (`/neonime/`, post ID 467980) on a shared Muvipro install whose real domain is **`https://animase.xyz`**. The catalog, search, detail, episode and player endpoints all live at that host's **root**, not under `/neonime/`. Playback is **iframe-only** (third-party file hosts); no direct `mp4`/`m3u8` is reachable without executing an AES+WASM JavaScript player.

---

## 1. Domain & discovery

| Candidate | Result (live) |
|---|---|
| `https://animase.xyz/neonime/` | **200**, canonical host |
| `http://154.203.167.107/neonime/` | **200**, nginx, site name "ANOBOY" |
| `http://154.203.167.17/neonime/` | 301 → `https://154.203.167.17/neonime/` → 200, site name "ANIMEINDO" |
| neonime.me | DNS 162.4.84.255, **no HTTP (000)** |
| neonime.id / .tv / .live / .net / .org / .bid / .one | **no DNS (000)** |
| neonime.xyz | 13.248.169.48, no HTTP |
| neonime.site | 403 (Cloudflare parking) |
| neon-ime.vercel.app | unrelated Jikan-based hobby project (github `negirox07/NeonIME`) — **not** the real site |

**How the real domain was found (reproducible, not guessed):** the open WP REST API emits canonical links, not the IP you called it on —

```
curl -s 'https://animase.xyz/wp-json/wp/v2/search?search=tantei&per_page=1'
# -> "url":"https://animase.xyz/eps/tantei-wa-mou-shindeiru-season-2-6/"
```

The three entry points are the **same WordPress install**: identical page ID 467980, identical catalog (`X-WP-Total: 5205` tv shows). Only the `<title>`/site-name string differs (ANOBOY / ANIMEINDO / ANIMASE). Treat `/neonime/` as a **listing alias**, not a separate source.

---

## 2. Platform / edge

- **WordPress** + theme **Muvipro 2.1.9** (Gian MR), Yoast SEO 28.8, WP 7.0.8 core assets.
- `animase.xyz` is behind **BunnyCDN** (`server: BunnyCDN-SG1-1173`), `vary: Accept-Encoding`. **No Cloudflare** on the canonical host.
- IP entry points: plain **nginx**, no CDN, no `cf-*` headers. `154.203.167.107` is **HTTP-only**; `.17` force-redirects to HTTPS.
- Static assets and thumbnails are served from a third host: `https://animehade.playsobat.xyz/wp-content/...`.
- Site is **indexable** (`robots: index, follow`); no login, no nonce gate on the endpoints used below.

---

## 3. Capability matrix

| Capability | Works? | Canonical endpoint | Notes |
|---|---|---|---|
| Search | **Yes** | `/?s=<q>` (root only) | 32 hits/page; returns **episodes** (subtype `episode`) and movies, not series |
| Search (JSON) | **Yes** | `/wp-json/wp/v2/search?search=<q>` | best: `id`, `url`, `subtype`; also `/wp/v2/tv?search=` (series-only) and `/wp/v2/episode?search=` |
| Ongoing list | **Yes** | `/tag/ongoing/` | **tag**, not category: count **465**, 15 pages |
| Completed list | **Yes** | `/tag/completed/` | tag, count **4436** |
| Full catalog | **Yes** | `/neonime/` + `/neonime/page/N/` | 188 pages ≈ 6 000 entries, mixed `/tv/` + root-level movies |
| `/category/on-going/` | **NO** | — | returns 200 but `<section class="no-results">` ("Nothing Found") |
| Detail (series) | **Yes** | `/tv/<slug>/` | full metadata + episode list in HTML |
| Detail (movie) | **Yes** | `/<slug>-<year>/` | root-level, WP post type `post` |
| Episode list | **Yes** | in detail DOM (`.gmr-listseries`) | no separate AJAX needed |
| Episode page | **Yes** | `/eps/<slug>` (Eps 1) / `/eps/<slug>-N/` | `data-id` = WP post ID |
| Streams | **Yes, iframe-only** | `POST /wp-admin/admin-ajax.php` | returns one `<iframe>` on `playsobat.xyz`; no `mp4`/`m3u8` |
| Direct file | **No** | — | download providers 302 to file-host pages, not media URLs |
| RSS | Yes | `/feed/` | not useful (posts only) |

---

## 4. URL patterns (verified by fetch)

```
LIST      https://animase.xyz/neonime/            and  /neonime/page/{2..188}/
ONGOING   https://animase.xyz/tag/ongoing/        (+ /page/N/)
COMPLETED https://animase.xyz/tag/completed/      (+ /page/N/)
SEARCH    https://animase.xyz/?s={q}              (+ &paged=N)
DETAIL    https://animase.xyz/tv/tantei-wa-mou-shindeiru-season-2/
MOVIE     https://animase.xyz/hakuouki-movie-1-kyoto-ranbu-2013/
EPISODE   https://animase.xyz/eps/tantei-wa-mou-shindeiru-season-2-6/
EMBED     <iframe src> e.g. https://playsobat.xyz/e/8aFduZ2KZQRB
DOWNLOAD  https://dlv2.playsobat.xyz/download.php?slug=<EMBED_ID>
```

Episode 1 has **no** `-1` suffix (`/eps/<slug>/`); `N>=2` is `/eps/<slug>-N/`. Do not derive episode numbers by string parse — take them from `.gmr-listseries` (label `EpsN` + `title="... Episode N"`).

---

## 5. DOM map (selectors verified against saved fixtures)

### 5.1 List / archive / search (`/neonime/`, `/tag/*`, `/?s=`)
```css
#primary                                  /* column */
main#main.site-main                       /* result container */
article.item-infinite                     /* one card; also has .item */
article h2.entry-title > a                /* href = detail URL, title = name */
article .content-thumbnail img            /* src = thumbnail (host animehade.playsobat.xyz) */
article .gmr-rating-item                  /* text "6.91" */
article .gmr-numbeps span                 /* episode count */
article .gmr-posttype-item                /* "TV Show" */
article .gmr-movie-on a                   /* genre links -> /category/<genre>/ */
#primary h1.page-title                    /* "Genre: On Going" / "Search Results For: x" */
.pagination a                             /* /neonime/page/N/ */
section.no-results                        /* empty tag confirms nothing found */
```
Home `/` and `/neonime/` share this markup. Cards are filtered by `article.item` only — the archive uses no `<article>` wrapper on empty pages.

### 5.2 Detail (`/tv/<slug>/`, `/<slug>-<year>/`)
```css
h1.entry-title[itemprop="name"]                 /* series title */
.entry-content-single[itemprop="description"] > p:first-of-type   /* synopsis */
.content-moviedata .gmr-moviedata               /* metadata rows, label in <strong>: */
  /*  By: | Posted on: | Views: | Genre: | Year: | Release: | Number Of Episode:  */
[itemprop="ratingValue"]                        /* e.g. 6.9 */
[itemprop="ratingCount"]                        /* e.g. 2372 */
.gmr-rating-bar span[style]                     /* width:69% */
.gmr-listseries a[href*="/eps/"]                /* ALL episodes: text EpsN, title attr has full name */
.gmr-listseries a.gmr-all-serie                 /* "View All Episodes" -> same /tv/ URL */
.gmr-grid article                               /* "Related Movies" (noise — must be excluded) */
```
Genre is a **category** list; Ongoing/Completed are **tags** on the same page (`/tag/ongoing/`). The detail page carries no `<tag>` block selector of its own — read the tag set from REST or from `/?tag=` links if needed.

### 5.3 Episode (`/eps/<slug>[-N]/`)
```css
h1.entry-title                                  /* "… Episode 6" */
#muvipro_player_content_id[data-id]             /* data-id = WP post_id (e.g. 483850)  <-- key */
ul.muvipro-player-tabs#gmr-tab > li > a        /* only one tab: "MULTI SERVER", href="#p1" */
#p1.tab-content-ajax                            /* EMPTY in initial HTML; filled by AJAX */
.gmr-embed-responsive iframe                    /* exists only AFTER the AJAX call */
#download .gmr-download-list a                  /* href dlv2.playsobat.xyz/download.php?slug=<EMBED_ID> */
.gmr-listseries a[href*="/eps/"]                /* sibling episode nav */
.post-views / .gmr-movie-view                   /* view counter (noise) */
```
The episode page **contains no iframe and no media URL** until the AJAX round-trip. A parser that only fetches HTML will find nothing.

---

## 6. AJAX (the only dynamic endpoint)

Source: `https://animehade.playsobat.xyz/wp-content/themes/muvipro/js/ajax-player.js` (200, 1 066 B).

```http
POST /wp-admin/admin-ajax.php HTTP/1.1
Content-Type: application/x-www-form-urlencoded; charset=UTF-8
Referer: https://animase.xyz/eps/<slug>-N/
X-Requested-With: XMLHttpRequest

action=muvipro_player_content&tab=p1&post_id=<data-id>
```
Verified response (`ajax_p1.html`, 200, 188 B):
```html
<div class="gmr-embed-responsive clearfix"><iframe src='http://playsobat.xyz/e/8aFduZ2KZQRB' frameborder=0 ... allowfullscreen></iframe></div>
```
- `tab` **must be `p1`**. `tab=1` and `tab=tab-1` return **200 with 0 bytes** — silently empty, not an error. Guard against that.
- No nonce, no cookie required; `Referer` + `X-Requested-With` were sent on every successful call.
- Only one tab exists on the episodes sampled ("MULTI SERVER"), so the multi-server UI is rendered **inside** the embed page, not by the site.

---

## 7. Stream extraction

### 7.1 Chain
```
episode HTML  --data-id-->  admin-ajax  --iframe-->  playsobat.xyz/e/<ID>
                                                        |
                                        encrypted window.payload + player.js (AES + WASM)
                                                        |
                                        window.playerJson = { SERVER: url }  (deleted after use)
                                                        |
                                        5 third-party iframe hosts (no direct media)
```

### 7.2 What the embed page is
`playsobat.xyz/e/<ID>` (200, 22 138 B saved) contains:
- `window.payload = "{\"iv\":\"…\",\"data\":\"…\"}"` — AES-CBC material, base64.
- `https://playsobat.xyz/assets/player.js?v=…` — decrypts (crypto-js) into `window.playerJson`, then `delete window.playerJson`.
- `disable-devtool.min.js` (blocks copy/cut/paste/right-click, can blank the page).
- An inlined base64 **WebAssembly** stub (`AGFzbQEAAAAB…`) that reports bot checks to a remote endpoint, plus popunder ad scripts (`semuadisini.xyz`, `llvpn.com`, `driverhugoverblown.com`).

**Consequence:** `playerJson` is **not fetchable by HTTP**. It was recovered only by running the page in a real browser and reading the rendered `#servers li a` (the map is deleted before any later eval).

### 7.3 Observed servers — episode 6 (`post_id=483850`)

| Server | Resolved URL | State at recon |
|---|---|---|
| HYDRAX | `https://abyssplayer.com/izFP62wTK` | redirects to `https://abyss.to/` when opened without the embed's referer → **effectively dead outside context** |
| VIDHIDE | `https://dintezuvio.com/embed/1ya743i9682w` | curl **000** (unreachable) |
| STREAMWISH | `https://hglink.to/e/mia79f3e5u5w` | 200, 288 B JS shell |
| MIXDROP | `https://mdfx9dc8n.net/e/8lll4lv7tnzd89` | 302 → `https://mxdrop.click/e/…` (rotating) |
| DOODSTREAM | `https://dood.la/e/ipjzkvl0s75g` | 301 → `https://playmogo.com/e/…` (rotating) |

Episode 1 (`post_id=480206`) resolves to `https://playsobat.xyz/e/ioUuB4o01JzP`; its server list was **not** captured (Chrome crashed twice at that step).

### 7.4 Direct media: negative result
- No `mp4`, `m3u8`, or `<video>` element in any fixture: home, list, search, detail, episode, embed, download.
- The download page (`playsobat.xyz/download.php?slug=<ID>`, 200) lists providers as
  `download.php?slug=<ID>&provider=HYDRAX|MIXDROP|VIDHIDE|DOODSTREAM|STREAMWISH|VIDCUY`, each of which **302s to the file host's landing page**, not to a file:
  - `HYDRAX` → `https://download.openx.xyz/?v=izFP62wTK`
  - `DOODSTREAM` → `https://dood.la/d/ipjzkvl0s75g`
  - `MIXDROP` → `https://mdfx9dc8n.net/f/8lll4lv7tnzd89`
- A Telegram-join gate (`t.me/+l2FAbb1QFBE5ZTVl`) sits on the REPORT/report flow, not on DOWNLOAD.

**Header notes for the embed chain:** `Referer` from the episode page is required for `admin-ajax.php` (sent on all successes); the file hosts additionally rotate domains, so any hardcoded host list rots within weeks. `playsobat.xyz` is **plain HTTP** in the iframe (`http://playsobat.xyz/e/…`), though it also answers on HTTPS.

---

## 8. Anti-bot

| Layer | Present? | Detail |
|---|---|---|
| Cloudflare | **No** on `animase.xyz` (BunnyCDN) | IPs are bare nginx |
| UA gating | No | Chrome UA used throughout; nothing challenged |
| Nonce/CSRF on AJAX | **No** | `muvipro_player_content` is open |
| Kill-switch on player | **Yes** | AES-encrypted `window.payload` + `player.js` |
| Devtools / copy block | **Yes** | `disable-devtool.min.js`, `disableSelect`/`disableCopy` |
| WASM bot probe | **Yes** | obfuscated wasm in embed page, beacons to ad endpoints |
| Off-site redirect trap | **Yes** | `GET /neonime/?s=<q>` → **301 to `https://www.tubenart.com`** (reproduced 3×, any `s=` value). `/?s=` at root is clean; `/neonime/?x=1` is 200. **Never put a query string on `/neonime/`.** |
| View-count beacon | Yes | `wp-postviews` (`postviews-cache.js`), harmless |

---

## 9. Risks for a Rust client

1. **Brand ≠ host.** `/neonime/` is a page on an install that calls itself ANOBOY/ANIMEINDO/ANIMASE depending on entry point. If the Rust adapter is "neonime", it will in practice be scraping one shared catalog — expect duplicate results with anoboy/animeindo adapters.
2. **Three permalink shapes** must all be handled: `/tv/<slug>/`, `/eps/<slug>[-N]/`, and root `/<slug>-<year>/` (movies).
3. **No scoped search.** There is no `/neonime/?s=`; search must target the root. Any UI that lets users search "within Neonime" is really site-wide.
4. **Episode count is not derivable from the slug.** Take `.gmr-listseries a` and the `Number Of Episode` meta row; `-N` suffixes are not reliable for parsing back.
5. **The stream URL is behind JS.** A pure `reqwest`+`scraper` client can at best return the `playsobat.xyz/e/<ID>` iframe — which then needs a headless browser to yield the 5 host URLs, and those hosts' domains rotate. Do not promise playable extraction.
6. **Images** point at `animehade.playsobat.xyz`, a separate host not covered by `animase.xyz` uptime.
7. **`/category/on-going/` is a trap** (200 + "Nothing Found"). The real list is `/tag/ongoing/`.
8. **The 188-page `/neonime/` listing** (`item-infinite` class but **no infinite-scroll AJAX** in `customscript.js`) is deterministic pagination — safe to crawl, but ~6 000 entries at 32/page.
9. **REST is open** (`/wp-json/wp/v2/{tv,episode,search,tags}`). This is the cheapest, most stable interface: `X-WP-Total: 5205` series, tags `ongoing=465`, `completed=4436`. It returns canonical `animase.xyz` links even when queried via IP.

---

## 10. Fixtures — `research/raw/neonime/`

| File | Bytes | What |
|---|---|---|
| `home_animase_neonime.html` | 99 789 | `/neonime/` p.1 (canonical) |
| `list_page2_animase.html` | 102 732 | `/neonime/page/2/` |
| `neonime_last.html` | 91 642 | `/neonime/page/188/` (mixed tv + root movies) |
| `home_animase.html` | 155 383 | site home `/` |
| `tag_ongoing.html` | 101 237 | `/tag/ongoing/` (32 series) |
| `ongoing_animase.html` | 42 855 | `/category/on-going/` → "Nothing Found" (negative fixture) |
| `completed_animase.html` | 42 869 | `/category/completed/` (negative fixture) |
| `search_animase.html` | 84 386 | `/?s=tantei` |
| `detail_animase.html` | 65 132 | `/tv/tantei-wa-mou-shindeiru-season-2/` |
| `episode_animase.html` | 59 139 | episode 6 (empty player) |
| `episode1.html` | 58 956 | episode 1 (no `-1` suffix) |
| `ajax_p1.html` | 212 | admin-ajax success → iframe |
| `ajax_ep1.html` | 213 | admin-ajax for ep.1 |
| `ajax_tab-1.html` | 0 | **negative**: wrong `tab` → 200 empty |
| `embed_playsobat.html` | 22 138 | encrypted embed page |
| `ajax-player.js` | 2 398 | theme player JS |
| `customscript.js` | 6 147 | theme main JS (no infinite-scroll AJAX) |
| `download_page.html` | 14 078 | provider list page |
| `rest_types.json` | 26 671 | post types (`tv`, `episode`, `blogs`) |
| `rest_tv.json` / `rest_ep.json` / `rest_search.json` | small | REST payloads |
| `home_107.html`, `detail_107.html`, `episode_107.html` | — | same pages via IP entry point (title = ANOBOY) |

---

## 11. Unknowns (do not assume)

- Do **all** episodes expose 5 servers? Only 2 episodes were opened; only ep.6's list was captured.
- The player's sort order lists `MAIN`, `DIRECT` before the file hosts. **Neither key ever appeared** in the one captured `playerJson` — it may exist only on older episodes, and it may be the only direct-media path. Untested.
- Whether `admin-ajax.php` works without `Referer` (never tested without it).
- Whether HYDRAX/`abyssplayer` can be coerced into an HLS manifest with the right `Referer`/`Origin` (the `download.openx.xyz` hop hints at an API behind it).
- Certificate validity on `https://154.203.167.17` (curl worked, cert not inspected).
- Whether `/neonime/?s=` redirect is a stray plugin rule or an intentional anti-scrape trap; it may change without notice.
