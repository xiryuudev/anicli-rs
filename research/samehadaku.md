# Samehadaku — Technical Reconnaissance for Rust `scraper` Parsers

- **Target**: Samehadaku (Indonesian fansub / anime streaming + download index)
- **Recon date**: 2026-11-14 (all times Asia/Jakarta, UTC+7)
- **Method**: `curl` with desktop Chrome UA, `--compressed`, HTTP/2; `lxml` CSS validation of every selector published below
- **Fixtures**: `anicli-rs/research/raw/samehadaku/` (34 files, ~3.0 MB)
- **Scope note**: no Rust code was written. All claims below carry an observed status code, header, selector hit-count, or sample string.

---

## 1. Domain & discovery

### Working base URLs (verified 2026-11-14)

| URL | HTTP | Final URL | Notes |
|---|---|---|---|
| `https://v2.samehadaku.how/` | **200** | same | **The real site.** 97,727 B, `server: cloudflare`, `x-samehada-cache: HIT`, `x-cache-engine: Redis-FPC` |
| `https://samehadaku.care/` | **200** | same | Static **JS redirect portal** (9,121 B), not the site |
| `https://samehadaku.how/` | 301 | `https://v2.samehadaku.how/` | Anchor domain, redirects to v2 |
| `https://v1.samehadaku.how/` | 301 | `https://v2.samehadaku.how/` | — |
| `https://samehadaku.email/` | 200 | `https://expatriate.travel/country-guides/saint_lucia/overview` | **Dead / hijacked.** 445,982 B unrelated travel page. Do not use. |

The `.care` portal sends **no HTTP redirect** — it returns 200 and redirects client-side after a 10-second countdown:

```js
// samehadaku.care body, verbatim
function countdown(){ ... icountdown--; ... }
window.location.replace('https://v2.samehadaku.how/')   // also in redirect()
```

Consequence for a Rust client: a plain HTTP GET on `samehadaku.care` will **look like success** (200, HTML) but contain zero anime. Either follow `v2` directly, or (better) treat `.care` as a *discovery hint* only.

### How the site announces its active domain

Two in-band signals, both parseable:

1. **Announcement block on every page** (`div.announcement-box`), verbatim:
   > `!!! PENTING !!! Website asli Samehadaku hanya ini, selain website ini adalah website palsu, Bookmark juga Samehadaku.care sebagai direct ke website Samehadaku asli` … followed by `<a href="https://samehadaku.care/">SAMEHADAKU.CARE</a>` and `<a href="https://t.me/samehadaku_care">Telegram</a>`

2. **Sidebar/footer social + Telegram links**: `https://t.me/samehadaku_care`, `https://www.facebook.com/samehadaku.care`, `https://instagram.com/samehadaku.care`.

Also relevant for domain rotation: the WP `guid` field leaks the *origin* install — `"guid":{"rendered":"https:\/\/sokuja.net\/?post_type=anime&#038;p=54641"}`. The DB has been migrated across at least `sokuja.net` → `samehadaku.how` → `v2.samehadaku.how`. Hardcoded absolute URLs inside `post_content` still point at the **old** host (e.g. `<a href="https://samehadaku.how/anime/chitose-kun-wa-ramune-bin-no-naka/">`), so **any URL harvested from body text must be re-based onto the current origin**.

**Discovery recommendation**: hardcode `v2.samehadaku.how` as default, keep `samehadaku.care` as a "check me" probe, and treat any host whose HTML lacks `var SamehadaData` as invalid (see §3 sanity check).

### Redirect behavior observed

- Trailing-slash canonicalisation is aggressive: `/genre/action` → **301** → `/genre/action/`; `?genre=action` → **301**; `/sitemap.xml` → **301**.
- Filter pagination 301s on the `+`-encoded form: `/daftar-anime-2/page/2/?status=Finished+Airing` → 301 → `?status=Finished%20Airing`. **Follow redirects** (`reqwest` default follows; keep it).
- 404 returns a real 404 (**59,410 B** themed page) — usable as a negative signal.

---

## 2. Platform

| Property | Value | Evidence |
|---|---|---|
| CMS | **WordPress 7.1.3** | `<meta name="generator" content="WordPress 7.1.3" />` |
| Theme | **`samehada`** (custom, "EastPlay"-derived) | `wp-content/themes/samehada/…` ×14; `<body class="… wp-theme-samehada …">`; HTML comment `<!-- Header matching Image #10 & EastPlay v1.5.9 -->` |
| CSS framework | **Tailwind** utility classes throughout (`flex`, `dark:bg-[#242526]`, `aspect-video`) | e.g. `<main id="content" class="flex-grow max-w-[1170px] mx-auto w-full px-4 sm:px-6 py-6">` |
| Front-end JS | Alpine.js (`@click="showComments = !showComments"`, `x-data`) + one bundled theme script | `wp-content/themes/samehada/dist/js/app.js?ver=1792055930` (84,653 B) |
| Asset versioning | Unix-epoch query strings — **changes every deploy** | `?ver=1792055930`, theme CSS/JS |
| CDN / WAF | **Cloudflare** (proxied, `server: cloudflare`, `cf-ray: …-SIN`) with a **custom page cache** in front (`x-samehada-cache`, `x-cache-engine: Redis-FPC`) | response headers |
| Bot protection | **Cloudflare "Just a moment…" interstitial — UA-gated, not JS-gated** (see §8) | 403 body `<title>Just a moment...</title>` |
| Security headers | `strict-transport-security: max-age=31536000` | — |
| HTTP/2 | Yes (`HTTP/2 200`) | — |
| Compression | **Brotli** (`content-encoding: br`) even when `Accept-Encoding: gzip, deflate, br` is offered | — |
| Analytics / ads | Heavy 3rd-party ad blocks (`div.ads-area-2col`, `blogger.googleusercontent.com` GIFs) + one player-overlay ad | — |

**Sanity check for a valid page**: presence of `var SamehadaData = {` and `<body class="… wp-theme-samehada …">`. The `.care` portal has neither.

---

## 3. Capability matrix

| Capability | Available | Where / how |
|---|---|---|
| **Search (HTML)** | **Y** — full paginated search | `GET /?s={q}`, paginates `GET /page/{n}/?s={q}`. 20 results/page. `?s=naruto` → 712 results, "Page 1 of 36" |
| **Search (JSON, live)** | **Y** — the good one | `GET /wp-admin/admin-ajax.php?action=samehada_live_search&s={q}` → JSON array (see §6). **Capped at 6 items, no pagination**, min 2 chars |
| **Ongoing / latest** | **Y** | `GET /anime-terbaru/` — 14 items/page, "Page 1 of 913" |
| **Completed browse** | **Y** (indirect only) | No dedicated page. Use `GET /daftar-anime-2/?status=Finished+Airing&order=latest` (30/page, "Page 1 of 25"). `/anime-movie/` is a **stub** (see §9) |
| **Popular / top** | **Y** | `GET /top10/` (10 items, `a.series` cards). Also `?order=popular` on `/daftar-anime-2/` |
| **Genre browse** | **Y (filter form only)** | `⛔ /genre/{slug}/` renders **no card list at all** (0 `article.animepost`, 0 `li[itemscope]` — only widget sidebars). Working alternative: `GET /daftar-anime-2/?genre[]={slug}&order=latest` → 30 cards |
| **Anime detail** — synopsis | Y | `div.entry-content-single p` |
| **Anime detail** — genres | Y | `a[href*="/genre/"]` (48 on page, incl. sidebar dupes — scope it) |
| **Anime detail** — status / type / episodes / score | Y | label/value spans, see §5.4 |
| **Episode list** | Y, **capped at 100** | `div.custom-scrollbar.space-y-2 > div` rows; One Piece yields exactly 100 rows (1180→1081) |
| **Episode page** | Y | `/…-episode-{n}/` |
| **Stream servers** | Y | `div.player-server-btn` with `data-nume` / `data-embed` (see §7) |
| **Download mirrors** | Y | `div.download-eps` blocks: 3 formats × 3–6 resolutions × 4–6 hosts |
| **Batch index** | Y | `GET /daftar-batch/` (30/page, "Page 1 of 21") |
| **Batch detail** | Y | `/batch/{slug}/` |
| **Schedule** | Y | `GET /jadwal-rilis/` + `GET /wp-json/apk/schedule` |
| **RSS** | Y | `GET /feed/` (10,956 B), `/comments/feed/` |
| **Sitemap** | Y | `GET /wp-sitemap.xml` → `wp-sitemap-posts-post-{1..7}.xml` |

---

## 4. URL patterns

All examples are **real, fetched 2026-11-14**.

| Page type | Pattern | Concrete example |
|---|---|---|
| Home | `/` | `https://v2.samehadaku.how/` |
| Ongoing / latest | `/anime-terbaru/` and `/anime-terbaru/page/{n}/` | `https://v2.samehadaku.how/anime-terbaru/` (200, 66,760 B, parent 913) |
| Completed (browse) | `/daftar-anime-2/?status=Finished+Airing&order=latest` | `…/daftar-anime-2/?status=Finished+Airing&order=latest` (200, 30 cards, "Page 1 of 25") |
| Popular / top 10 | `/top10/` | `https://v2.samehadaku.how/top10/` (200, 98,690 B) |
| Anime index + filters | `/daftar-anime-2/` (+ `/page/{n}/`) | `…/daftar-anime-2/?genre%5B%5D=mecha&order=title` (200, 12 cards) |
| Search | `/?s={q}` (+ `/page/{n}/?s={q}`) | `https://v2.samehadaku.how/?s=naruto` (200, 20 cards) |
| Genre (**broken**) | `/genre/{slug}/` | `https://v2.samehadaku.how/genre/action/` (200 but **0 cards**) |
| Anime detail | `/anime/{slug}/` | `https://v2.samehadaku.how/anime/chitose-kun-wa-ramune-bin-no-naka-cour-2/` (200, 109,349 B) |
| Episode | `/{series-slug}-episode-{n}/` (root-level, **no** `/episode/` segment) | `https://v2.samehadaku.how/chitose-kun-wa-ramune-bin-no-naka-cour-2-episode-5/` (200, 127,636 B) |
| Batch index | `/daftar-batch/` (+ `/page/{n}/`) | `https://v2.samehadaku.how/daftar-batch/` (200, 30 cards, "Page 1 of 21") |
| Batch detail | `/batch/{slug}/` | `https://v2.samehadaku.how/batch/bleach-sennen-kessen-hen-kashin-tan-cour-4-episode-1-10-batch/` (200, 129,996 B) |
| Schedule | `/jadwal-rilis/` | `https://v2.samehadaku.how/jadwal-rilis/` (200, 168,048 B) |
| Pagination (generic) | sortable archives: `/…/page/{n}/`; searches: `/page/{n}/?s={q}` | — |

**Gotcha**: pagination position differs per archive. Without a filter the page segment precedes the query (`/daftar-anime-2/page/2/?status=…`); for search it precedes too (`/page/2/?s=naruto`). Building `?s=x&page=2` returns page 1.

---

## 5. DOM map

> Selector counts below are from `lxml.cssselect` against the saved fixtures, so they are reproducible.

### 5.0 Two different card components — the single most important structural fact

The site uses **two unrelated card markups**, and which one appears is per-page-type, not per-item:

**A. `li[itemscope]` list** — *widget* lists (home, `/genre/*`, `/top10`, sidebar "Latest Episode", and `/anime-terbaru/`):
```
<div class="post-show"> <ul> <li itemscope itemtype="http://schema.org/CreativeWork"> … </li> </ul> </div>
```

**B. `article.animepost` grid** — *archive/query* lists (search, `/daftar-anime-2/`, `/daftar-batch/`):
```
<div class="anime-grid"> <article class="animepost post-… anime type-anime"> … </article> </div>
```

A parser with a single card selector will silently return 0 items on half the page types. **Implement both.**

---

### 5.1 Ongoing / latest — `GET /anime-terbaru/`

Fixture: `ongoing.html` (66,760 B, 14 items, "Page 1 of 913").

| Field | Selector | Verified count | Sample values |
|---|---|---|---|
| List container | `.post-show > ul` | 1 | — |
| Item | `.post-show > ul > li` | **14** | — |
| **Episode link** | `.post-show li .thumb a` → `@href` | 14 | `https://v2.samehadaku.how/shuiro-no-kamen-episode-6/` |
| **Anime link** | `.post-show li .entry-title a` → `@href` | 14 | `https://v2.samehadaku.how/anime/shuiro-no-kamen/` |
| Title | `.post-show li .entry-title a` (text; also `@title`) | 14 | `Shuiro no Kamen`, `Kusuriya no Hitorigoto Season 3` |
| Thumbnail | `.post-show li .thumb img` → `@src` | 14 | `…/uploads/2026/11/image-10-2-300x169.jpg`, `…/uploads/2026/11/web-1-12-300x170.jpg` |
| Episode number | `.post-show li .dtla span` (0) → `<b>Episode</b> 6` | 42 spans total | `Episode 6`, `Episode 7` |
| Author | `.post-show li .dtla span[itemprop="author"] span[itemprop="name"]` | 14 | `Azuki`, `Dinda` |
| Date | `.post-show li .dtla span` (2) → after `<b>Released on:</b>` | 14 | `1 Jam yang lalu`, `11 Jam yang lalu` |
| Pagination | `.pagination` (text) / `.pagination a.arrow_pag` | 1 / 1 | `Page 1 of 913` |

Sample span triple, verbatim: `['Episode 6', 'Posted by: Azuki', 'Released on: 1 Jam yang lalu']`.

**Fragile**: the three `.dtla > span` children are **positional**; there is no class distinguishing them. Prefer matching the `<b>` label text (`<b>Episode</b>`, `<b>Posted by:</b>`, `<b>Released on:</b>`) over `span[0..2]`.

**Note**: on this page the **thumb link is the episode**, the **title link is the series**. Do not merge them.

### 5.2 Grid card — search / `/daftar-anime-2/` / `/daftar-batch/`

Fixture: `search.html` (108,618 B, 20 cards). Identical card markup on all three, with a different `.type` payload per source.

| Field | Selector | Verified | Sample values |
|---|---|---|---|
| List container | `.anime-grid` | 1 | — |
| Item | `article.animepost` | 20 (search) / 30 (index, batch) | — |
| Link | `.animepost .animposx > a` → `@href` | 20 | `…/naruto-kecil-episode-220/` |
| Post id | `.animepost @id` → `post-37909` | 20 | `37909` |
| Title | `.animepost .data .title h2` (text or `@title`) | 20 | `Naruto Kecil Episode 220`, `Bleach: Sennen Kessen-hen – Kashin-tan (Cour 4) Episode 1-10 [BATCH]` |
| Thumbnail | `.animepost img.anmsa` → `@src` | 20 | `…/uploads/2024/08/naruto-instagramatnaruto-300x168.jpg` |
| **Badge (top-left)** | `.animepost .content-thumb .type` | 20 | `Eps 220` (search) / `TV` (batch) / `Eps 1180` |
| Score | `.animepost .content-thumb .score` | 20 | `★ 8.84`, `★ 8.86` |
| **Sub-line (bottom)** | `.animepost .data .type` | 20 | `3 Tahun yang lalu` (search) — **relative time, not status** |
| Tooltip synopsis | `.animepost .stooltip .ttls` | 30 | `Beberapa saat sebelum Naruto Uzumaki lahir, seekor iblis besar…` |
| Tooltip genres | `.animepost .stooltip .genres a` | 92 (30 cards) | `Action`, `Adventure`, `Shounen`, `Supernatural` |
| Tooltip type badge | `.animepost .stooltip .type-badge` | 30 | `TV`, `Movie` |
| Tooltip views | `.animepost .stooltip .views` | 30 | `8,394 Views`, `279,650 Views` |
| Tooltip score | `.animepost .stooltip .skor` | 30 | `★ 8.86` |
| Pagination total | `.pagination span` (first) | 1 | `Page 1 of 21` |
| Next page | `.pagination a.arrow_pag` → `@href` | 1 | `…/daftar-batch/page/2/` |

**⚠ Critical inconsistency**: `.content-thumb .type` and `.data .type` are **both named `type` but carry different semantics**. On search they are episode-count and relative age; on the batch index they are `TV` and `Completed`. And `.type-badge` in the tooltip is the *reliable* `TV/Movie/OVA` value everywhere. **Parse media type from `.stooltip .type-badge`, not from `.type`.**

The `.animepost` card also embeds a `<div class="ply"><i class="fa fa-play"></i></div>` overlay — a *decoration*, not a stream link. Do not mistake it for a player.

### 5.3 Batch index & batch detail

`/daftar-batch/` uses card B (§5.2) unchanged; `.type` = `TV`, `.data .type` = `Completed`, `.content-thumb .type` = `TV`.

`/batch/{slug}/` (`batch_detail.html`, 129,996 B) shares the **episode page** download markup (§7.2) but adds 4K tiers and different hosts:

```
formats: MKV, MP4, x265
MKV  → 360p, 480p, 720p, 1080p, 4K [Ep1-5], 4K [Ep6-10]
MP4  → 360p, 480p, MP4HD, FULLHD, 4K [Ep1-5], 4K [Ep6-10]
x265 → (same ladder)
hosts: Gofile, Mirrored (mir.cr), File io (limewire), Pixeldrain, Mediafire (RAR)
```

Batch-only host labels not seen on episode pages: **`Mirrored`** (`https://mir.cr/…`) and **`File io`** (`https://limewire.com/d/…`).

### 5.4 Anime detail — `GET /anime/{slug}/`

Fixture: `detail.html` (109,349 B). Selecting `h1.entry-title` here returns **0** — the detail page's `<h1>` has no class.

| Field | Selector | Verified | Sample value |
|---|---|---|---|
| Title | `h1` (first) | 1 | `Chitose-kun wa Ramune Bin no Naka Cour 2 Sub Indo` |
| Poster | `img[alt="…"]` inside the `w-48` column, or `div.w-48 img` | 1 | `…/uploads/2026/10/bx198727-BL0JTpg0G8tq.jpg` |
| **Score** | `span.font-extrabold` | 1 | `7.62` |
| **Vote count** | `span.font-extrabold + span` → text after `/` | 1 | `/ 1,438` |
| Stars | `div.text-yellow-400` | 1 | `★★★★★` |
| Synopsis | `div.entry-content-single p` | 1+ | `Lanjutan dari Chitose-kun wa Ramune Bin no Naka` (body-text link points at **old host** `samehadaku.how`) |
| Genres | `.entry-content-single ~ div a[href*="/genre/"]` — better: `div.pt-3.border-t a[href*="/genre/"]` | 4 (48 site-wide) | `Comedy`, `Harem`, `Romance`, `School` |
| Bookmark/anime id | `div.samehada-bookmark-wrapper @data-anime-id` | 1 | `54641` |

**Metadata rows** — label/value pairs, both plain `<span>`s, no semantic hook:

```css
span.text-gray-500.dark\:text-gray-400.w-28.shrink-0.font-medium     /* label */
  + span.font-semibold                                                /* value */
```

Verified extraction (12 rows, all present):

| Label | Value |
|---|---|
| Japanese | `千歳くんはラムネ瓶のなか 第2クール` |
| Synonyms | `Chiramune, Chitose-kun is Inside a Ramune Bottle, …` |
| English | `Chitose Is in the Ramune Bottle Part 2` |
| **Status** | `Ongoing` |
| **Type** | `TV` |
| Source | `Light novel` |
| Duration | `23 min.` |
| **Total Episode** | `11` |
| Season | `Fall 2026` |
| Studio | `feel.` |
| Producers | `Kadokawa, Studio Mausu` |
| Released | `Oct 13, 2026 to ?` |

Rows are **optional and sparse** — a finished anime (`detail_finished.html`) lacks `Synonyms` sometimes, and Duration can be `Unknown`; One Piece reports `Total Episode: Unknown`. Never assume 12 rows.

**Episode list** (on detail page):

| Field | Selector | Verified |
|---|---|---|
| Container | `div.custom-scrollbar.space-y-2` (equivalently `div.max-h-\[520px\]` or `div.space-y-2.pr-1`) | 1 |
| Item | container `> div` | **5** (Chiramune) / **12** (finished) / **100** (One Piece — **hard cap**) |
| Episode number | item `.shrink-0.w-8 a` (text) or `div[class*="w-8"] a` | `5`,`4`,`3`,`2`,`1` — **descending** |
| Episode title | item `h4 a` | `Chitose-kun wa Ramune Bin no Naka Cour 2 Episode 5` |
| Episode URL | item `h4 a @href` | `…/chitose-kun-wa-ramune-bin-no-naka-cour-2-episode-5/` |
| Air date | item `span.text-\[11px\]` | `11 November 2026`, `4 November 2026` |

Two traps: (1) rows are **newest-first**, reverse for chronological ordering; (2) at exactly **100 rows the list is truncated** (One Piece: 1180→1081 only) — there is **no load-more button and no `data-total`** in the HTML, so long-runners require crawling `/page/N/` of the *episode* post type or the sitemap.

⚠ **Selector trap, verified**: `div.custom-scrollbar` matches **two** elements on every detail page — index 0 is the sidebar (`flex-grow overflow-y-auto py-2 custom-scrollbar`, **0** child `div`s) and index 1 is the episode list (`max-h-[520px] … space-y-2 pr-1 custom-scrollbar`, 5/12/100 children). Naive `cssselect("div.custom-scrollbar")[0]` yields **zero episodes on every page** without erroring. Always disambiguate with `.space-y-2` or `.max-h-*`, or select the container that actually has `> div` children.

**Batch CTA** — only on fully-aired series:

```html
<a href="https://v2.samehadaku.how/batch/…-episode-1-12-batch/"
   class="w-full py-3 px-4 rounded bg-[#e65100] …"> … </a>
```
Selector: `a[href*="/batch/"]` (0 on ongoing shows, 1 on completed ones).

### 5.5 Episode page — `GET /{slug}-episode-{n}/`

Fixture: `episode.html` (127,636 B).

| Field | Selector | Verified | Sample |
|---|---|---|---|
| Title | `h1.entry-title` | 1 | `Chitose-kun wa Ramune Bin no Naka Cour 2 Episode 5` |
| **Episode number** | `[itemprop="episodeNumber"]` | 1 | `5` |
| Description | `div.entry-content-single[itemprop="description"]` | 1 | `Nonton Streaming anime … HD Subtitle Indonesia terbaru di Samehadaku!` |
| Sub marker | `span.lg` | 1 | `SUB` |
| Relative date | `span.time-post` | 1 | `4 Hari yang lalu` |
| Post id | `article[itemprop] @id` / `@data-post` on buttons | — | `55251` |
| Server buttons | `.player-server-btn` / `div[id^="player-option-"]` | **5** (varies 3–11) | `Blogspot`, `VIP STREAMING`, `Mega 1080p` |
| Default embed | `#player-frame-wrapper iframe @src` | 1 | `https://www.blogger.com/video.g?token=…&origin=sameguru2.blogspot.com` |
| Prev episode | `.naveps .nvs a` (0) | 3 (some are `#`/`.nonex`) | `…-episode-4/` |
| All episodes | `.naveps .nvs.nvsc a` | 1 | `…/anime/chitose-kun-wa-ramune-bin-no-naka-cour-2/` |
| Next episode | `.naveps .nvs.rght a` | 1 | `#` with class `nonex` when none |
| Download blocks | `div.download-eps` | **3** | — |

**Fragile on this page**: prev/next anchors sometimes carry `href="#" class="nonex"` and `aria-disabled="true"` instead of a real URL — always check `class` before following.

---

## 6. AJAX / JSON endpoints

All discovered by reading `wp-admin/admin-ajax.php` calls out of `themes/samehada/dist/js/app.js` and the inline `SamehadaData` object.

### 6.0 The bootstrap object (present on **every** HTML page)

```html
<script>var SamehadaData = {"ajaxUrl":"https://v2.samehadaku.how/wp-admin/admin-ajax.php",
"restUrl":"https://v2.samehadaku.how/wp-json/apk","nonce":"60bb8ee8f8","turnstileSiteKey":""};</script>
```
- `nonce` is **site-wide, identical across pages** (verified: same `60bb8ee8f8` on home, detail, episode, search). It is a WP `wp_create_nonce` value and survives at least the observation window; treat it as cacheable but refresh on 403.
- `turnstileSiteKey` is **empty** → Cloudflare Turnstile is installed but **not enforced** on these actions (no CAPTCHA observed on any endpoint below).

### 6.1 ⭐ Live search — the one structured endpoint worth preferring

```
GET https://v2.samehadaku.how/wp-admin/admin-ajax.php?action=samehada_live_search&s={query}
```
- **Method**: GET. **Auth**: none — no nonce, and `X-Requested-With` is *not* required (verified with and without).
- **Min length**: 1 char → `{"success":true,"data":[]}`; ≥2 chars returns hits (JS enforces `p.length<2` client-side).
- **Hard cap: 6 results, no pagination, no total count.** Measured: `s=na` → 6, `s=naruto` → 6, `s=one piece` → 6.
- **Latency**: 0.32 s. `content-type: application/json; charset=UTF-8`.

Response shape (verbatim, keys confirmed on 3 queries):

```json
{"success":true,"data":[
  {"id":37909,
   "title":"Naruto Kecil Episode 220",
   "url":"https:\/\/v2.samehadaku.how\/naruto-kecil-episode-220\/",
   "thumb":"https:\/\/v2.samehadaku.how\/wp-content\/uploads\/2024\/08\/naruto-instagramatnaruto-300x168.jpg",
   "score":"8.84",
   "type":"TV",
   "status":"Completed"}]}
```
Keys: `id` (int), `title`, `url`, `thumb`, `score` (**string**, may be empty), `type` (`TV`/`Movie`/…), `status` (`Completed`/`Ongoing`).

**⚠ Behavioural trap**: results are **episode posts, not series**, and ranking is recency-biased, not relevance-biased. `s=naruto` returns `Naruto Kecil Episode 220, 219, 218, 216, 217, 214` — i.e. one series' last six episodes, *not* a list of Naruto series. For "find the series", this endpoint is near-useless; use HTML search (§5.2) or `?title=` on the filter form. For "autocomplete an episode", it is excellent.

### 6.2 ⭐ Player embed resolver — required for servers whose `data-embed` is a shortcode

```
POST https://v2.samehadaku.how/wp-admin/admin-ajax.php
Content-Type: application/x-www-form-urlencoded

action=player_ajax&nonce={SamehadaData.nonce}&post={id}&nume={n}&type=schtml
```

- Returns **raw HTML fragment** (`<iframe …>`), `content-type: text/html`, 200. **Not JSON.**
- **nonce is mandatory**: without it or with a wrong one → body `-1`, **HTTP 403**.
- `post` = the episode's post id (`article @data-post`, e.g. `55251`); `nume` = the `data-nume` of the server button.
- `type` is `schtml` for every button observed; the JS default when absent is `urliframe`.

Verified responses:

| `nume` | Response body |
|---|---|
| 2 (filedon) | `<iframe src="https://filedon.co/embed/hJMhR8vo1m" class="…" …></iframe>` (262 B, 200) |
| 1 (blogspot) | `<iframe src="https://www.blogger.com/video.g?token=…&#038;origin=sameguru2.blogspot.com" …>` (440 B, 200) |
| 6 on `episode.html` (nonexistent) | **0 B, 200** — empty body means "no such server", not an error |

**Why this matters**: on older episodes the site stores an unresolved **shortcode**, not a URL:

```html
data-embed="[vidlion id=ayxpw0mqqi6b]"   <!-- server label: "Vidhide 360p" -->
```
and `player_ajax` returns that shortcode **verbatim** (`[vidlion id=ayxpw0mqqi6b]`, 25 B, 200) — the shortcode is resolved **client-side by a `vidlion` handler that is not in `app.js`** (grep: 0 hits). In practice these 3 of 11 servers on old episodes are **not resolvable without executing page JS**. Treat any `data-embed` that does not contain `<iframe` as unusable and fall through to the next server.

### 6.3 Other admin-ajax actions (enumerated, not needed for parsing)

| `action` | Method | Needed for |
|---|---|---|
| `samehada_live_search` | GET | search — **useful** |
| `player_ajax` | POST + nonce | embed resolution — **useful** |
| `samehada_bookmark_status` | GET??nonce&anime_id | bookmarks (auth) |
| `samehada_toggle_bookmark` | POST + nonce + `anime_id` | bookmarks (auth) |
| `samehada_get_watchlist_cards` | POST + nonce + `page`/`per_page`/`anime_ids[]` → `data.cards_html` | watchlist |
| `samehada_login` / `_logout` / `_register` / `_lostpassword` | POST + nonce | auth |
| `samehada_reaction` | POST + nonce + `post_id`,`reaction_type` | reactions |
| `samehada_vote_comment` | POST + nonce + `comment_id`,`direction` | comments |

Ignore all but the first two.

### 6.4 Custom REST namespace `apk` — **best for structured "latest" + schedule**

```
GET https://v2.samehadaku.how/wp-json/apk/latest      → 200, 7,398 B, JSON array
GET https://v2.samehadaku.how/wp-json/apk/schedule    → 200, 16,761 B, JSON array
```

`/apk/latest` element keys and sample values:

```json
{"id":55306,"title":"Shuiro no Kamen Episode 6","series_id":54546,
 "series_title":"Shuiro no Kamen","episode":"6","type":"SUB",
 "url":"https://v2.samehadaku.how/shuiro-no-kamen-episode-6/",
 "thumb":"…/uploads/2026/11/image-10-2-300x169.jpg","time":"1 Jam yang lalu"}
```

`/apk/schedule` element keys:

```json
{"id":12726,"title":"Vanitas no Carte","url":"…/anime/vanitas-no-carte/",
 "img":"…/uploads/2022/01/120021.jpg","type":"TV","score":"8.17","time":"03:00",
 "genre":["Fantasy","Historical","Shounen","Supernatural","Vampire"],
 "status":"Finished Airing"}
```

Note `status` here uses the **AniList vocabulary** (`Finished Airing`), *not* the HTML page's `Completed`. Two vocabularies exist; normalise both. Also `episode` and `score` are **strings**, not numbers.

### 6.5 Core WP REST (available, but thin)

| Endpoint | Status | Notes |
|---|---|---|
| `/wp-json/wp/v2/anime` | 200 | CPT list; fields `id, slug, link, title.rendered, content.rendered, genre[], season[], studio[], producers[], class_list[]` |
| `/wp-json/wp/v2/anime/{id}` | 200 | **No `meta`** (`meta: null`) — status/score/episode-count are **not** exposed. Not a substitute for HTML detail parsing. |
| `/wp-json/wp/v2/posts?slug={episode-slug}` | 200 | Returns the episode post; `content.rendered` is the short description only. **No stream/download data.** |
| `/wp-json/wp/v2/search?search={q}` | 200 | Generic WP search |
| `/wp-json/wp/v2/types` | 200 | CPTs: `anime`, `batch`, `blog` (+ core) |

**Bottom line**: REST gives you *discovery and metadata*, never *streams or mirrors*. Episode-page HTML parsing is unavoidable.

---

## 7. Stream / mirror extraction

There is **no direct `.mp4` or `.m3u8` anywhere in the episode HTML** — verified: `grep -c '\.mp4|\.m3u8' episode.html` → **0**. Everything is an `<iframe>` embed, and media URLs live on third-party hosts.

### 7.1 Player servers — the structure

```html
<div class="server_option"><div id="server"><ul>
  <li>
    <div id="player-option-1"
         class="east_player_option player-server-btn on"
         data-post="55251"
         data-nume="1"
         data-type="schtml"
         data-url=""
         data-embed="&lt;iframe src=&quot;https://www.blogger.com/video.g?token=…&quot; …&gt;&lt;/iframe&gt;">
      <span>Blogspot</span>
    </div>
  </li>
  …
</ul></div></div>
```

`data-embed` is **HTML-escaped** (`&lt;iframe src=&quot;…&quot;&gt;`). Un-escape once, then read `src`.

**Selectors**: `.player-server-btn` (or `div[id^="player-option-"]`); label = `span` (direct child); embed = `@data-embed`; id for `player_ajax` = `@data-post` + `@data-nume`.

### 7.2 Resolution encoding — **inconsistent, must be parsed from the label**

Resolutions are **not** a separate field. They are either embedded in the server label or implied by the host:

| Observed label | Resolution |
|---|---|
| `Blogspot` | unlabelled (single stream, usually 360–480p) |
| `Premium HD`, `VIP HD`, `VIP STREAMING` | unlabelled (usually 720p) |
| `VIP MKV`, `VIP MP4`, `VIP x265` | unlabelled **by format** |
| `Mega 480p`, `Mega 720p`, `Mega 1080p` | explicit |
| `Vidhide 360p / 480p / 720p` | explicit |
| `Kraken 360p / 480p / 720p` | explicit |
| `Pixel 360p / 480p / 720p` | explicit |
| `Server 1 480p`, `Server 2 720p`, `Server 3 1080p` | explicit, **generic labels** |

**Robust rule**: regex `(\d{3,4})p` on the label → `Some(height)`; otherwise `None`. Also detect the format token `(MKV|MP4|x265)` when present. Do **not** infer resolution from `data-nume` — the numbering is per-episode and reorders (server 2 was `VIP STREAMING` on one episode, `VIP MP4` on another).

Server count varies by episode: **3** (`episode.html`), **5**, **6**, **8**, **11** (`episode_old.html`). Never require a minimum.

### 7.3 Hosts observed (concrete examples, tokens redacted where opaque)

| Host | URL form | Reaches playable media? |
|---|---|---|
| **Blogspot / Blogger** | `https://www.blogger.com/video.g?token=AD6v5…&origin=sameguru2.blogspot.com` | **Yes.** Player page embeds `https://rr3---sn-….googlevideo.com/videoplayback?expire=…&id=…&itag=18&source=blogger…` (Google Video CDN). `itag=18` = 360p MP4, progressive → **directly mpv-able**. Embed page 24,996 B. |
| **Mega** | `https://mega.nz/embed/{id}#{key}` — e.g. `https://mega.nz/embed/TxgzDJCC#bGdIWquBkCWz9VYdcD_mAyC_0TXt4NRJKDSQSDhyMPc` | **No** (needs Mega SDK / decryption). Effectively unusable for a simple parser. |
| **Filedon** | `https://filedon.co/embed/{slug}` — e.g. `https://filedon.co/embed/hJMhR8vo1m` | **Yes, via JS inspection only.** Embed page (20,012 B) contains **no literal m3u8**; only an API route table (`public.media.hls` → `media/{slug}/hls.m3u8`). Requires the page's `embed-*.js` to construct the URL. |
| **IceFile** ("VIP MKV/MP4/x265", "VIP HD") | `https://beta.icefile.my.id/embed/{base64ish}` — e.g. `…/embed/MTlJZ2FyUFh5R0RFRERIeVNhaFJNM283NjRSUmFKdUEt` | ⭐ **Best case.** Embed HTML (55,502 B) contains a JSON `sources` array with **3 HLS URLs** (1080p/720p/360p) plus `res`/`resNum`. But the URLs **302 to a re-signed location**, so they expire. |
| **Vidhide / Vidlion** | `data-embed="[vidlion id=ayxpw0mqqi6b]"` — **unresolved shortcode** | **No.** Nothing in theme JS resolves it. |
| **Krakenfiles** | `https://krakenfiles.com/embed-video/{id}` | Unverified (did not fetch) |
| **Wibufile** ("Premium HD") | `https://api.wibufile.com/embed/naruto-220-720p-samehadaku-care-mp4` | Unverified |
| **Pixeldrain** ("Pixel 360p" as *player*) | `https://pixeldrain.com/u/{id}` | Yes — `https://pixeldrain.com/api/file/{id}/info` → **200**; `/api/file/{id}?download` streams |
| **Gofile** (download only) | `https://gofile.io/d/{code}` | ⚠ `GET https://api.gofile.io/contents/{code}` → **401 `{"status":"error-token"}`** — needs a session token; not directly fetchable |
| **Acefile**, **Mediafire (RAR)**, **Mirrored (mir.cr)**, **File io (limewire)** | download only | Password-protected RAR for Mediafire |

### 7.4 Playability of the two "good" paths

**A. IceFile HLS** — full concrete example:

```
https://beta.icefile.my.id/api/v1/media/cHVibGlj/{SERIES-TOKEN}/encoded/1/1280x720/{SIGNATURE}/file.m3u8
```
```
$ curl -sL "$URL" -o x.m3u8
200  7417 bytes  content-type: application/octet-stream
#EXTM3U
#EXT-X-VERSION:3
#EXT-X-PLAYLIST-TYPE:VOD
#EXT-X-TARGETDURATION:6
#EXTINF:5.005000,
1.png
#EXTINF:5.005000,
2.png
```
- **No `Referer` and no `User-Agent` required** — verified identical results with `-A ""` and with a Chrome UA, with and without `Referer: https://v2.samehadaku.how/`.
- **Must follow redirects**: `{SIGNATURE}` rotates (`1794660916904` → `1794662794395`), served as `HTTP/2 302` with `location:`. `reqwest`/`mpv` following redirects is sufficient.
- ⚠ Segment names are **`1.png`, `2.png`, …** — a JPEG/PNG-disguised HLS payload. Any client that filters on extension will break. Also `content-type` is `application/octet-stream`, not `application/vnd.apple.mpegurl`.
- `mpv` invocation that works: plain `mpv --http-header-fields="..."` is **not** needed; `mpv "$m3u8_url"` succeeds.

**B. Blogspot** — the `googlevideo` URL is CDN-signed:
```
https://rr3---sn-npoeenl7.googlevideo.com/videoplayback?expire=1794692894&ei=…&ip=157.85.206.96&id=135b03ac31dee1ff&itag=18&source=blogger…
```
- `expire`, `ei`, and **`ip=`** are baked in — the URL is bound to the requesting IP and expires. Extract-and-play immediately; do not cache.
- `itag=18` → 360p MP4 (progressive), safe for mpv.
- Requires first fetching the `blogger.com/video.g?token=…` page and scraping the `videoplayback` URL out of its escaped JSON (`\u003d`-style escapes; un-escape before use).

### 7.5 Referer / User-Agent summary

| Target | UA | Referer | Notes |
|---|---|---|---|
| `v2.samehadaku.how` HTML | **browser UA required** | optional | plain curl UA → 403 Cloudflare (§8) |
| `admin-ajax.php` (`live_search`, `player_ajax`) | browser UA | not required | nonce required on `player_ajax` only |
| `beta.icefile.my.id` (embed + HLS) | **not required** | **not required** | verified both ways |
| `blogger.com/video.g` | browser UA | sent `v2.samehadaku.how` during test | returned 200 |
| `googlevideo.com/videoplayback` | — | — | IP-bound + expiring; expect to re-derive |
| `pixeldrain.com/api` | any | none | 200 on `/info` |

---

## 8. Anti-bot & politeness

### Cloudflare: present, but **UA-gated, not JS-challenged**

| Request | Result |
|---|---|
| `curl` with **no** `User-Agent` | **403**, 5,354 B, `<title>Just a moment...</title>` |
| `curl -A "curl/8.5.0"` | **403** |
| `curl -A "Mozilla/5.0"` (truncated) | **403** |
| `curl -A "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36"` | **200**, full 97,724 B |

This is the **entire** anti-bot story in practice: a plausible desktop Chrome UA string is sufficient, and **no challenge cookie, no JS execution, no `cf_clearance`** is needed. There is no Turnstile on any content endpoint (`turnstileSiteKey` is `""`).

⚠ Worth noting the asymmetry: the *home page* 403s without a UA, but an *episode page* returned **200** even with no UA (`episode: 200 127636`). So the block is **route-dependent** — reproducing on `/` is not evidence that `/…-episode-N/` is blocked, and vice versa.

**Recommendation**: always send a full Chrome UA. Expect a spurious 403 if the UA is dropped by a proxy/redirect hop; retry once with the UA before declaring failure.

### robots.txt (2,794 B, 200)

- `User-agent: *` → `Content-Signal: search=yes,ai-train=no,use=reference`, **`Allow: /`** — general crawling is permitted.
- `Disallow: /wp-admin/` **but** `Allow: /wp-admin/admin-ajax.php` — the AJAX endpoints are *explicitly allowed*. Good news for §6.
- A long Cloudflare-managed block bans ~35 training crawlers (`GPTBot`, `ClaudeBot`, `CCBot`, `Google-Extended`, `Bytespider`, `Amazonbot`, …). These are *AI-training* agents; a personal-use TUI client is not one.
- `Sitemap: https://v2.samehadaku.how/wp-sitemap.xml`

### Performance

| Metric | Observed |
|---|---|
| Home TTFB total | 0.27 s / 0.51 s / 0.56 s / 0.69 s / 0.73 s (5 samples) |
| Episode page | 0.47 s (127 KB) |
| Detail page | 0.47 s (109 KB) |
| `live_search` AJAX | 0.32 s |
| Search page | 0.37 s |
| Batch index | 0.47 s |
| HTTP version | HTTP/2 |
| Encoding | **Brotli** (`content-encoding: br`); sizes above are *decompressed* |
| Cache | `x-samehada-cache: HIT` + `cf-cache-status: DYNAMIC` — a theme-level Redis FPC sits in front of WP |

**Rate limits**: **none observed.** 12 sequential requests (`/?p=1..12`) all completed with no 429, no challenge, no `Retry-After`; latencies 0.30–0.70 s. There is no evidence of throttling at ~1 req/s. Still, be a good citizen: 1 concurrent connection, ~300–500 ms spacing, and a declared UA is polite and adequate.

---

## 9. Parsing risks

Ordered by likelihood × blast radius.

1. **Two incompatible card components** (§5.0). Highest risk. A parser keyed on `article.animepost` returns **0** for `/anime-terbaru/`, `/top10/`, `/genre/*`; one keyed on `li[itemscope]` returns **0** for search, `/daftar-anime-2/`, `/daftar-batch/`. Implement both and assert `count > 0` before treating a page as empty.

2. **Tailwind utility classes are used as if they were semantic classes.** Selectors like `.w-28`, `.space-y-2`, `.max-h-\[520px\]`, `.text-\[11px\]`, `.pt-3`, `.border-t` are *the only* hooks on metadata rows, episode rows, and dates. These break the instant the theme is restyled. Prefer:
   - label-text matching (`<b>Episode</b>`, `<b>Released on:</b>`, `Status`, `Total Episode`) over positional index;
   - `@itemprop` / `@data-*` / `@id="post-…"` attributes where they exist — these are the *stable* layer (`itemprop="headline"`, `itemprop="episodeNumber"`, `itemprop="image"`, `data-post`, `data-nume`, `data-anime-id`);
   - `a[href*="/genre/"]`, `a[href*="/batch/"]`, `a[href*="-episode-"]` URL-shape selectors.

3. **`.type` is overloaded three ways** (§5.2): `.content-thumb .type` = episode count *or* media type; `.data .type` = relative age *or* status; `.stooltip .type-badge` = media type (reliable). Never bind a field called "type" to the first `.type` you find.

4. **`/genre/{slug}/` is a dead end** (§3). Returns 200 with a full page chrome, announcement, Top-10 widget, Latest-Episode widget, sidebar — and **zero** cards, on every slug tested (`action`, `isekai`, `mecha`) and on `/page/2/`. A naive parser will report "genre has 0 anime" rather than "this page type is broken". Use `?genre[]=` on `/daftar-anime-2/` instead.

5. **`/anime-movie/` is a stub.** 200, 98,248 B, `<title>Anime Movie Sub Indo – Samehadaku</title>`, **0** `article.animepost`, **0** `.animposx`, no `<h1>`. It renders only widget sidebars. Do not use it as "completed".

6. **Episode list truncates at 100 rows** with no marker (§5.4). One Piece returns exactly 100 of 1180 episodes. There is no load-more control, no `data-total`, no pagination within the detail page. Detect `rows == 100` and treat as truncated.

7. **Resolution/format live only inside free-text labels** (`Mega 1080p`, `VIP x265`, `Server 2 720p`, `Blogspot`). Labels are Indonesian/English mixed and reorder per episode. Regex `(\d{3,4})p` + format token; tolerate `None`.

8. **Unresolved shortcodes in `data-embed`** (`[vidlion id=…]`). `player_ajax` echoes them unchanged. Detect `data-embed` lacking `<iframe`/`http` → skip that server. Affected 3/11 servers on `naruto-kecil-episode-220`.

9. **Relative Indonesian timestamps** (`1 Jam yang lalu`, `4 Hari yang lalu`, `3 Tahun yang lalu`) on listing and episode pages — not ISO. The `apk` JSON has the same problem (`"time":"1 Jam yang lalu"`). Only the *detail page's* episode list has absolute dates (`11 November 2026`), and sitemaps/`wp-json/wp/v2/posts` give `date`/`date_gmt` ISO — use those if a sortable timestamp is required.

10. **Absolute URLs in body text point at retired hosts.** `post_content` links to `https://samehadaku.how/…`, while the live origin is `v2.samehadaku.how`. Re-base every extracted href against the response's own origin (`<link rel="canonical">` or the request URL), never against a hardcoded constant.

11. **Brotli-only compression.** If the HTTP client doesn't advertise `br` or can't decode it, bodies may arrive as gibberish or force a fallback. `reqwest` with `brotli` feature (or `gzip` fallback) — confirm the decoded bytes start with `<!doctype html>`.

12. **Ad blocks masquerade as content.** `div.ads-area-2col` / `div.ads-grid-wrapper` (each holding 6–13 casino ads on `blogger.googleusercontent.com` GIFs), plus a `#player-ad-overlay` "Tutup Iklan" modal inside the player. Measured inflation of `a[href^="http"]`: **detail 26, episode 26, batch-detail 26, player-overlay +2** — but **0 on home, 0 on search, 0 on the batch index**. So the hazard is concentrated exactly on the pages you parse for episodes and downloads, and absent on listing pages. 21 of detail's 117 absolute anchors and 67 of episode's 160 are non-`samehadaku` external (ads + real hosts mixed). Exclude `div[class*="ads-"]` and `#player-ad-overlay` subtrees before any "first external link" heuristic; `div[class*="ads-"] a[href^="http"]` is the precise cut.

13. **Fake-domain warnings are inline content.** `.announcement-box` and several `<p><strong>` blocks contain the words "website palsu" etc. Don't let a naive keyword scan for "anime"/"samehadaku" mis-read them.

14. **HTML entity noise.** Titles are HTML-escaped and use typographic dashes: `Bleach: Sennen Kessen-hen &#8211; Kashin-tan`, `I&#039;m standing on 1,000,000 lives.`, `∬`. Un-escape once; also normalise `’` (U+2019) vs `'` when matching `English`/`Synonyms` fields, since the site mixes both.

### Fallback selectors (defensive ladder)

| Field | Primary | Fallback 1 | Fallback 2 |
|---|---|---|---|
| Grid card | `article.animepost` | `div.animposx` parent | `a[rel]` with `href` matching `/anime/|/-episode-|/batch/` |
| Widget card | `div.post-show li[itemscope]` | `div.post-show ul > li` | `h2.entry-title a` |
| Card title | `.data .title h2` | `h2[title]` `@title` | `a[title]` `@title` |
| Card thumb | `img.anmsa` `@src` | `.content-thumb img` | `img[itemprop="image"]` |
| Episode rows | `div.custom-scrollbar.space-y-2 > div` | `div.max-h-\[520px\] > div` | `a[href*="-episode-"]` dedup-by-href |
| Metadata row | `span.font-semibold` after label | label-text match | `class_list` from REST |
| Servers | `.player-server-btn` | `div[id^="player-option-"]` | `[data-embed]` |
| Downloads | `.download-eps-links a` | `div.download-eps a[href^="http"]` | `a[rel*="nofollow"]` inside `#downloadb` |

---

## 10. Fixtures

`anicli-rs/research/raw/samehadaku/` — 34 files, ~3.0 MB. All fetched 2026-11-14 with a desktop Chrome UA, `--compressed`.

| File | Page type | Bytes |
|---|---|---|
| `home.html` | Home / landing | 97,727 |
| `ongoing.html` | Ongoing (`/anime-terbaru/`, page 1) | 66,760 |
| `completed.html` | `/anime-movie/` — **stub, 0 cards** | 98,248 |
| `search.html` | Search `?s=naruto` p1 | 108,618 |
| `search_p2.html` | Search `?s=naruto` p2 | 108,906 |
| `search_empty.html` | Search with 0 results | 50,310 |
| `genre.html` | `/genre/action/` — **0 cards** | 97,950 |
| `genre_p2.html` | `/genre/action/page/2/` — 0 cards | 98,028 |
| `genre_isekai.html` | `/genre/isekai/` — 0 cards | 97,953 |
| `genre_mecha.html` | `/genre/mecha/` — 0 cards | 97,811 |
| `batch.html` | Batch index `/daftar-batch/` | 126,720 |
| `batch_detail.html` | `/batch/…-episode-1-10-batch/` | 129,996 |
| `daftar_anime.html` | Anime index `/daftar-anime-2/` + filter form | 152,502 |
| `filter_popular.html` | `?status=Currently+Airing&order=popular` | 125,894 |
| `filter_genre.html` | `?genre[]=action&order=latest` | 151,498 |
| `filter_completed.html` | `?status=Finished+Airing&order=latest` | 153,492 |
| `detail.html` | Anime detail (ongoing, 5 eps) | 109,349 |
| `detail_finished.html` | Anime detail (completed, 12 eps, batch CTA) | 120,886 |
| `detail_long.html` | Anime detail (One Piece, 100-row cap) | 258,798 |
| `episode.html` | Episode page (5 servers) | 127,636 |
| `episode_old.html` | Episode page (11 servers, vidlion shortcodes) | 232,903 |
| `top10.html` | Top 10 page | 98,690 |
| `care_home.html` | `.care` JS redirect portal | 9,121 |
| `care_headers.txt` | `.care` response headers | 548 |
| `v2_headers.txt` | `v2` response headers (HSTS, cache) | 690 |
| `robots.txt` | robots.txt (2,794 B) | 2,794 |
| `app.js` | Theme `app.js` (all AJAX call sites) | 84,653 |
| `apk_latest.json` | `GET /wp-json/apk/latest` | 7,398 |
| `apk_schedule.json` | `GET /wp-json/apk/schedule` | 16,761 |
| `rest_anime_one.json` | `GET /wp-json/wp/v2/anime/54641` | 2,502 |
| `rest_post_ep.json` | `GET /wp-json/wp/v2/posts?slug=…-episode-5` | 2,144 |
| `icefile_embed.html` | IceFile embed (HLS `sources` array) | 55,502 |
| `filedon_embed.html` | Filedon embed (route table only) | 118,887 |
| `blogspot_embed.html` | Blogger `video.g` (googlevideo URL) | 24,996 |

**Deliberately not saved**: `.m3u8` playlists (they 302 to ephemeral signed URLs and expire), `googlevideo` URLs (IP-bound), Gofile/Mediafire payloads (auth/RAR).

---

## 11. Unknowns

Stated plainly; each is something I could **not** verify in this session.

1. **Domain durability.** `v2.samehadaku.how` is live *today*. Rotation cadence is unknown; `samehadaku.email` is already lost to an unrelated travel site. I did not find any machine-readable domain-rotation feed — only human-readable banners. A TUI will need a manual/periodic domain update path.
2. **Whether `.care` ever emits a real HTTP redirect.** It did not in any test (always 200 + client-side `location.replace`). Not tested from a mobile UA or from Indonesian IP space.
3. **Krakenfiles, Wibufile, Acefile, Mediafire, mir.cr, limewire** — embed/stream behaviour unverified. I confirmed the *link shapes* only.
4. **Filedon's actual m3u8 retrieval.** The embed ships a router table, not a URL; I did not execute `embed-*.js`, so I never obtained a playable Filedon URL. It is *probably* `https://filedon.co/media/{slug}/hls.m3u8` but I did not confirm.
5. **The `vidlion` shortcode.** No handler exists in theme JS; I could not resolve `[vidlion id=…]` to a URL by any static means. Whether it resolves via a plugin loaded only on some templates is unknown.
6. **Episode-list "load more".** No control was found for the 100-row cap. Whether an `admin-ajax` action exists for it is unknown — I did not find one in `app.js`, but the enumeration came from a single minified bundle.
7. **Genre archive intent.** `/genre/{slug}/` may be intentionally card-free (a hub page) rather than broken. The theme's archive template may have regressed. Unknowable from outside.
8. **Rate limits at higher concurrency.** Only 12 sequential requests tested. Behaviour at, say, 10 concurrent requests over minutes is unmeasured — the Redis FPC suggests the origin is protected, but Cloudflare's rate-limit rules are not observable.
9. **`nonce` lifetime.** Identical across 4 pages within ~10 minutes. TTL unknown. Code should re-read it on 403 rather than assume.
10. **`/jadwal-rilis/` DOM.** Fetched (168,048 B, 200) but **not mapped** — out of the core path; only the JSON `apk/schedule` equivalent was documented.
11. **Mobile UA behaviour.** Not tested; all evidence is desktop Chrome UA.
12. **Whether `v2` is canonical long-term** or whether a `v3` will supersede it (the `v1`→`v2` 301s suggest a versioned rollout).
13. **Login-gated features** (bookmarks, watchlist, comments) — endpoints enumerated from JS but not exercised; they are out of scope for a read-only parser.
