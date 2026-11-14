# Kusonime — Technical Reconnaissance Report

**Target:** `https://kusonime.com/`
**Purpose:** offline-testable DOM/endpoint contract for a Rust `scraper` parser
**Recon date:** 2026-11-14 (Asia/Jakarta), all probes live
**Verdict in one line:** Kusonime is a **download-only batch aggregator**. It exposes a fully working WP REST API for search/browse, a clean DOM for series metadata, and a batch download table — but **no episode list, no player, no mp4/m3u8 URL is derivable**. Not usable as a streaming source; usable as a download-catalog source.

---

## 1. Domain & discovery

| URL | Status | Evidence |
|---|---|---|
| `https://kusonime.com/` | **200**, 82,626 B | primary, save as `home.html` |
| `https://www.kusonime.com/` | 301 → `http://kusonime.com/` | note: redirects to **http**, not https |
| `https://kusonime.org/` | 301 → `https://www.kusonime.org/`, then **403** (11…/110 B JS bounce page) | dead/unrelated parking |
| `kusonime.id` / `.net` / `.blog` / `.xyz` / `v2.kusonime.com` | **000** (DNS fail) | no mirrors exist |

- **No domain rotation observed.** Single canonical host. Anti-bot is Cloudflare but *passive* (see §8).
- Both plain curl (no UA) and a Chrome UA return 200 — **UA is not gated**.
- Latency: 0.59–1.13 s for a search page; 0.80 s for a REST search. Consistent, no rate-limit responses observed across ~60 requests.

## 2. Platform

- **WordPress 7.1.4** — `<meta name="generator" content="WordPress 7.1.4" />`.
- Theme: **custom `kusonime` theme** (`/wp-content/themes/kusonime/inc/kuso.js?ver=4`). Class names are Indonesian (`venz`, `venser`, `venkonten`, `detpost`, `kover`, `jdlz`, `lexot`, `smokeddlrh`). Non-standard → not covered by any generic WP scraper.
- Server stack: `server: cloudflare`, `x-powered-by: EasyEngine v4.8.0` (nginx), HTTP/2, Brotli+gzip.
- Plugins visible: Yoast SEO (`sitemap_index.xml`, `yoast_head_json` in REST), WP-PageNavi (`.wp-pagenavi`), wpfront-scroll-top, `activity-log/v1` REST namespace.
- **`<body>` has no class attribute** — you cannot use `body.single-post` etc. Scope on `#venkonten` / `.venser` instead.
- Ad/JS noise to ignore: `zxc.nyaa-tan.my.id/js/-ads-banner.js`, GTM (`UA-98288079-1`, `G-G21JFBZV4Z`), an `#dl-notif` div reading *"Matikan ADBLOCK untuk melihat link download"* which is **decorative only** — the links are present in raw HTML regardless (verified: 5 rows parsed from plain curl).

## 3. Capability matrix

| Capability | Y/N | Where |
|---|---|---|
| Search | **Y** | `/?s=<q>` (HTML) **or** `/wp-json/wp/v2/posts?search=<q>` (**preferred**) |
| Browse latest | **Y** | `/`, `/page/N/` — 10 items/page, **520 pages** (`Page 1 of 520`) |
| Popular | **N** | no popular endpoint; sidebar "New Add Series" (`ul.rtd`) is just recent posts |
| Genre browse | **Y** | `/genres/<slug>/`, index at `/genres/` |
| Season browse | **Y** | `/seasons/<season>-<year>/`, plus `/seasons/ova/`, `/special/`, `/ona/` |
| Type lists | **Y** | `/anime-list-bd/`, `/anime-movie-list/`, `/daftar-live-action/` |
| Full alphabetical index | **Y** | `/anime-list-bd/` — **single 497 KB page, 1,226 series**, no pagination |
| Advanced filter | **Y** | `/advance-search/?years=<ids>&genres[]=<slug>` (multi-select, values are **comma-joined taxonomy IDs**) |
| Detail | **Y** | `/<slug>/` — rich metadata block |
| **Episode list** | **N** | **absent on every page sampled.** No `eplister`/`episodelist`/`listlink`/`bixbox`; no `<ul>` in the download area; word "Episode" appears 0–1 times per page (only in the title / prose) |
| **Per-episode stream** | **N** | no `<video>`, no real `<iframe>`, no player. The 1 "iframe" hit per page is the wpfront-scroll-top `hide_iframe` JSON config, not a player |
| Batch downloads | **Y** | `div#dl` → `div.smokeddlrh` blocks → `div.smokeurlrh` rows → `<a>` host links |
| Direct-playable URL (mp4/m3u8) | **N** | regex `\.(mp4\|mkv\|m3u8\|webm)` over detail **and** 268 KB batch page → **0 matches** |

**Kusonime confirms the known context: one page per *series*, not per episode.** A 265 KB page is large purely because it holds 18 batch blocks × 3 rows = 54 host rows (One Piece), not because it holds episodes.

## 4. URL patterns

```
home          https://kusonime.com/
page N        https://kusonime.com/page/2/                      # 520 pages
search        https://kusonime.com/?s=naruto                    # also &post_type=post (no effect)
search page N https://kusonime.com/page/2/?s=naruto             # 8 items on p2 (!), p1 had 10
genre         https://kusonime.com/genres/mystery/
genre index   https://kusonime.com/genres/
season        https://kusonime.com/seasons/winter-2025/
category      https://kusonime.com/category/anime/
adv-search    https://kusonime.com/advance-search/?years=3549,3412&genres%5B%5D=action
detail        https://kusonime.com/ameku-md-batch-subtitle-indonesia/
batch  (multi)https://kusonime.com/one-piece-0001-1150-batch-subtitle-indonesia-4/
```

- **Detail slugs are unstable and non-derivable** — they are editorial, e.g. `ameku-md-batch-subtitle-indonesia`, `11-eyes-subtitle-indonesia-12`, `one-piece-0001-1150-batch-subtitle-indonesia-4`. **Never construct a detail URL from a title; always take `href` from listing/REST.**
- Missing trailing slash → 301 to the slashed form. www → http 301.
- **`https://kusonime.com/anime/<slug>/` is NOT a route.** `/anime/ameku-.../` 301s to the bare post; `/anime/solo-leveling/` 301s to a **JPEG** (`/wp-content/uploads/2024/01/solo-leveling.jpg`). Do not use an `/anime/` prefix.
- **Soft-404 trap:** any unknown slug returns `HTTP/2 404` *with* `Location: https://kusonime.com/`; following redirects silently yields a **200 homepage**. Always disable auto-redirect (or compare final URL) or every bad slug becomes "success".

## 5. DOM map

### 5.1 Listing item (home / page N / search / genre / category — *identical markup*)

```css
Container:  div.venz > ul                       /* note: <ul> is a DIRECT child of .venz */
Item:       div.kover                          (1 per card)
  thumb:    div.kover > div.detpost > div.thumb > a
  img:      .thumbz > img.wp-post-image        (src, srcset, width=150 height=~91)
  title/href: div.content > h2.episodeye > a   (a[href], a[title])
  meta P's:  div.content > p                   (3 p's, in order)
  genres:    div.content p:nth-of-type(3) a[rel="tag"]
  pagination: div.pagination > div.wp-pagenavi   (span.pages "Page 1 of 520", a.page.larger)
```

Sample item 1 (home):
- title `Ameku Takao no Suiri Karte BD Batch Subtitle Indonesia`
- href `https://kusonime.com/ameku-md-batch-subtitle-indonesia/`
- img `https://kusonime.com/wp-content/uploads/2025/01/Ameku-Takao-no-Suiri-Karte.jpg`
- `<p><i class="fa fa-user"></i> Posted by Admin</p>`
- `<p><i class="fa fa-clock-o"></i> Released on 12:00 pm</p>`
- `<p><i class="fa fa-tag"></i> Genre <a href=".../genres/mystery/">Mystery</a>, <a href=".../genres/slice-of-life/">Slice of Life</a></p>`

Sample item 2 (search `?s=naruto`):
- title `Boruto: Naruto Next Generations 001-293 END Batch Subtitle Indonesia`
- href `https://kusonime.com/boruto-batch-subtitle-indonesia-8/`
- genres `action, adventure, martial-arts, shounen, super-power` (5 links)
- meta `Released on 8:00 pm`

**⚠ Fragile:** there is **no** type/status/episode-count/score on cards. `Released on 12:00 pm` is a **time only — no date** (and it is the *post* publish time, not the anime air date). Item count per page is **10 on page 1, 8 on page 2** — never hardcode a page size.

### 5.2 Full alphabetical index (`/anime-list-bd/`)

```css
Wrap:   div.venser > div.daftarkartun
A-Z bar: div.abjtext a[href="#A"]           (also "#%23" for '#')
Row:    div.bariskelom
  anchor:  div.barispenz > a[name='A']
  list:    div.jdlbar > ul > li > a.kmz     (rel="<post_id>", href, title)
```
- 1,226 `a.kmz`, 32 `div.bariskelom` (letters `. 1 2 3 4 8 9 A–Z`). Single page, **no pagination** (`grep wp-pagenavi` → 0).
- `a.kmz[rel]` carries the **numeric WordPress post ID** — the only place on the site it is exposed in HTML; useful for joining with REST `id`.

### 5.3 Detail page metadata

```css
Page root:   #venkonten
Breadcrumb:  .venser > .breadcrumbs          (EMPTY in all samples)
Poster:      .post-thumb img.wp-post-image   (559x340)
Title:       .post-thumb > h1.jdlz
Byline:      .venutama > .kategoz            ("Posted By Admin on 14 November, 2026" + span.viewoy "2098 Views")
Body wrap:   .venutama > .lexot
Info block:  .lexot > .info                  ← all metadata lives here
Synopsis:    .lexot > p (after .info)        ← untagged, see warning
Download:    .lexot > .dlbodz > #dl
```

`.info` is a flat `<p><b>Label</b>: value</p>` list. Observed keys, **identical set on 7/7 posts sampled**:

| `<b>` label | Example value | Notes |
|---|---|---|
| `Japanese` | `天久鷹央の推理カルテ` | native title |
| `Genre ` | `<a .../genres/mystery/>Mystery</a>, <a .../>Slice of Life</a>` | **trailing space in the label**; links comma-separated |
| `Seasons ` | `<a .../seasons/winter-2025/>Winter 2025</a>` | trailing space; **fallback for "season"** |
| `Producers` | `Aniplex, Magic Capsule, BS11, ...` | plain text |
| `Type` | `BD` / `TV Series` | |
| `Status` | `Completed` / `Ongoing` | |
| `Total Episode` | `12` — **or `?`** (One Piece) | not always numeric |
| `Score` | `7.14` / `8.57` | float, string |
| `Duration` | `25 min. per ep.` | |
| `Released on` | `Jan 02, 2025` | human date, `%b %d, %Y` |

**Title alternates** are *not* in a labelled field. English/romaji alternates appear (a) inside the `h1.jdlz` post title itself, and (b) as repeated sentences in the SEO spam `<h1>` near the page bottom (`... anibatch, ... animeindo, ... samehadaku, ... MKV 720P`). There is no `Alternative Titles` row — do not look for one.

**⚠ Critical fragility:** the synopsis paragraphs are **siblings of `.info`, not children of a container**. The only structural delimiter is `<div class="clear"></div>` immediately after `.info`. Parsing `p` siblings until you hit `.dlbodz` is the only robust approach.

### 5.4 Episode list

**Does not exist.** Documented here so no one wastes time:
- `eplister`, `episodelist`, `epcheck`, `listlink`, `bixbox`, `eps-list` → **0 occurrences** on detail, batch, and 8 random posts.
- No `<ul>` anywhere inside `.dlbodz`. No `<table>`, no `<video>`, no real `<iframe>`.
- `Total Episode: 12` is a *number in the info block*, not a set of links.

### 5.5 Batch / download section

```css
Wrap:      .dlbodz
Notif:     #dl-notif          ("Matikan ADBLOCK..." — ignore)
Root:      #dl
Block:     #dl > div.smokeddlrh        (1..18 per page)
  header:  div.smokettlrh              ← "Download <Series> Batch Subtitle Indonesia"
  row:     div.smokeurlrh              (2..6 per block)
    label: <strong>                    ← resolution OR episode-range
    links: <a href title>              ← " | " separated, mix of live and <del>
```

**Label format — two distinct shapes, sometimes on the same page:**

| Shape | Examples | Meaning |
|---|---|---|
| Resolution | `360P`, `480P`, `720P`, `1080P`, and **`720p` lowercase** | quality tier |
| Episode range | `000-025`, `026-050`, `751-775`, `1100-1122` | arc chunk (One Piece block 1 uses these **instead of** resolutions) |
| Special | `Subtitle` | font/subs `.rar`, no video |

Observed across 29 rows / 10 recent posts: `{'360P':5, '480P':10, '720P':7, '1080P':5, 'Subtitle':2}` — **`720p` vs `720P` casing is inconsistent within a single page** (verified on `batch.html`: 17 lowercase `720p`, and `detail.html` has `720P`). Normalise with `to_ascii_uppercase()`.

Two real block samples:

```
Block 1 (multi-arc page, label = range):
  <div class="smokettlrh">Download One Piece Batch Subtitle Indonesia</div>
  <div class="smokeurlrh"><strong>000-025</strong>
      <a href="https://acefile.co/f/31593027/opk-000-025-rar">Acefile</a> |
      <a href="https://acefile.co/f/12071409/tjks-000-025-rar">Google Sharer</a> |
      <a href="https://lbx.to/f/iOJ8yUx">Linkbox</a> |
      <del datetime="2020-11-12T07:34:47+00:00">Hxfileco</del></div>

Block 2 (single-batch page, label = resolution):
  <div class="smokettlrh">Download Ameku Takao no Suiri Karte Batch BD Subtitle Indonesia</div>
  <div class="smokeurlrh"><strong>720P</strong>
      <a href="https://acefile.co/f/112460959/...-720p-rar">Google Sharer</a> |
      <a href="https://drive.usercontent.google.com/download?id=11-MYeTIsVEnBlb64Gmr2cOb5IE-x7UtP&export=download">Google Drive</a> |
      <a href="https://pixeldrain.com/u/H1F12W3b">Pixeldrain</a> |
      <a href="https://1024terabox.com/s/1qWGc7Wo4MJZZ37nMHbJ3GA">Terabox</a> |
      <a href="https://itsfileditchfiles.st/echo13/98afdbcef5fc518532d7/[Kusonime] ... 720p.rar">Fileditch</a> |
      <a href="https://mega.nz/file/Ci5QXRSZ#w5GRj0TOhfR0aq4j3fUJppvo5am_dmIqHBsw6Sisr3c">Mega.nz</a> |
      <a href="https://megaup.net/11037ae956e8a8da156096837513299e/">Megaup</a></div>
```

**⚠ Fragile: `"Google Sharer"` is a mislabel.** Its href is always `acefile.co` (a second Acefile mirror), never Google. Two different `acefile.co` links in the same row are normal. **Match on host, not on anchor text.**

**⚠ `"Google Drive"` has two host shapes:** legacy `drive.google.com/file/d/...` (older posts) and `drive.usercontent.google.com/download?id=...&export=download` (newer).

## 6. AJAX / JSON endpoints

**`/wp-json/wp/v2/posts?search=` works — and it is excellent.** This is the single most valuable finding for the Rust client.

```
GET https://kusonime.com/wp-json/wp/v2/posts?search=ameku&per_page=2
    &_fields=id,date,modified,slug,link,title,excerpt
→ HTTP/2 200, 0.80 s, 1,587 B
```
Response (real, trimmed):
```json
[{"id":44045,"date":"2026-11-14T12:00:24","modified":"2026-11-14T15:35:31",
  "slug":"ameku-md-batch-subtitle-indonesia",
  "link":"https://kusonime.com/ameku-md-batch-subtitle-indonesia/",
  "title":{"rendered":"Ameku Takao no Suiri Karte BD Batch Subtitle Indonesia"},
  "excerpt":{"rendered":"<p>Ameku Takao no Suiri Karte berpusat di RSI Tenikai...&#8230;</p>","protected":false}}]
```

Verified behaviours:
- `X-WP-Total: 5199`, `X-WP-TotalPages: 52` at `per_page=100` → **~5,199 posts total**, full pagination over JSON.
- `orderby=date&order=desc` → 200. `categories=1` filter → 200. Empty/garbage query → `[]` (200, not error).
- `/wp-json/wp/v2/search?search=ameku` → 200, returns `{id,title,url,type,subtype}` — lighter, but **no date/excerpt**.
- `/wp-json/wp/v2/categories` → 200 with `{id,name,slug,count,parent}` (see §3), though the theme's real taxonomy is custom (`genres`, `seasons`) and **is not exposed** as a REST taxonomy — `/wp-json/wp/v2/taxonomies` lists only `category`, `post_tag`, `nav_menu`, `wp_pattern_category`.
- `wp/v2/types` = **`post` only** for content. No custom post type.

**⛔ The critical REST limitation:** `content.rendered` is **1,431 B for a 98 KB page** — it is only the synopsis prose. The entire download table is **absent** from REST:

```
smokeddlrh  → False      smokeurlrh → False
<strong>720P → False     dlbodz     → False
```
The theme injects `.dlbodz` at render time (not a shortcode in `content`, not in `meta` — `meta` only contains `{"footnotes": ""}`). **So: use REST for search/browse/index, and HTML for the host links.** There is no JSON path to the download table.

- `admin-ajax.php` exists but only for `action=wpdAddSubscription` (a plugin widget). **No anime-related AJAX.** The theme JS (`kuso.js`) only handles dark-mode toggle and genre-checkbox limiting (max 4 genres) — **zero data fetching**. No XHR is needed to render anything.

## 7. Stream / mirror extraction

**There is no stream — only archives.** Everything is `.rar` film batches (or `.mkv` inside). Confirmed by `mime_type` from an actual host API: `application/vnd.rar`, 488,950,671 bytes.

Host census (from `detail.html`, `batch.html`, 10 recent posts, 1,226-slug index):

| Anchor text | Host(s) seen | Direct file? | Playable by mpv? |
|---|---|---|---|
| `Acefile` | `acefile.co/f/<id>/<name>-rar` | ❌ needs login | ❌ |
| `Google Sharer` | **`acefile.co`** (always) | ❌ needs login | ❌ |
| `Google Drive` | `drive.google.com/file/d/…`, `drive.usercontent.google.com/download?id=…` | ⚠️ quota-gated | ❌ rar |
| `Pixeldrain` | `pixeldrain.com/u/<id>` | ⚠️ hotlink-blocked | ❌ rar |
| `Terabox` | `1024terabox.com`, `teraboxapp.com`, `terabox.com` | ❌ login/app | ❌ |
| `Mega.nz` | `mega.nz/file/<id>#<key>` | ❌ encrypted, needs Mega SDK | ❌ |
| `Megaup` | `megaup.net/<hash>/` | ⚠️ | ❌ |
| `Fileditch` | `itsfileditchfiles.st/<...>.rar` | ✅ **the only direct HTTP** | ❌ rar |
| `Linkbox` | `lbx.to/f/<id>` | ❌ | ❌ |
| `Hxfileco` | `hxfile.co` | ⚠️ often `<del>` (dead) | ❌ |
| `Uptobox` | `uptobox.com` | ❌ **DNS dead** (curl 000) | ❌ |
| `Mir.cr` | `mir.cr` | mirror-list wrapper | ❌ |

**No ZippyShare, no GoFile, no Krakenfiles** on Kusonime — those appear on *other* Indonesian sites, not here. Do not code for them.

**Newer posts (>= 2026-11) hide every host behind two redirector hops.** Real chain, fully resolved:

```
kusonime detail page
  <a href="https://s.id/coC9j">Pixeldrain</a>          ← ALL 6 hosts share only 2 s.id tokens!
   → 303  https://duniakode.com/vkcpkowjeu             ← JS pastebin ("Loading... Please wait")
   → GET  https://duniakode.com/raw/vkcpkowjeu         ← plain text, 653 B
        360P
        Google Sharer : https://srnky.com/yUe5Yvo00mb
        Pixeldrain    : https://srnky.com/zEJUWRqsJtb
        Mega.nz       : https://srnky.com/Ypz
        ... (480P / 720P / 1080P, same 3 hosts each)
   → https://srnky.com/<token>                          ← 822 KB page, "please click on below captcha box"
        contains g-recaptcha AND cf-turnstile, POSTs to /b/<token>?d=<base64>&s=<hmac>
```

So: **`s.id` → duniakode paste → `srnky.com` → reCAPTCHA/Turnstile → (never reached) real host.** Automating past step 4 requires solving a CAPTCHA; I did not attempt it. Note the anchor text at step 1 is **pure fiction** (all 6 links share one token; the true host mapping only appears in the paste text).

**Consequence for a TUI player:** Kusonime yields **0 playable URLs**. Best case you resolve `Fileditch` to a direct `.rar` HTTP link (no captcha), which mpv still cannot play — you would need to download + unrar + pick a `.mkv`. **Treat Kusonime as a download-catalog/metadata source only.** If the TUI needs streaming, route that to a different provider and use Kusonime purely for `title / score / genres / status / total_episode / poster`.

**Referer/UA:** `acefile.co` and `pixeldrain.com` both serve fine to plain curl *for the landing page*, but their file endpoints reject:
- `acefile.co/d/<token>` → `307` to `/login?...` even with a browser UA **and** `Referer: https://kusonime.com/`.
- `pixeldrain.com/api/file/<id>` → `403`; its own info API self-reports `availability: "file_rate_limited_captcha_required"` with `"We have detected the use of hotlinking… Hotlinking is only supported when either the uploader or the downloader has a paid subscription"`. Direct `GET`/`HEAD` both 403; `pixeldrain.com/u/<id>` page is 200 but `pixeldrain.com/file/<id>` is 404.

## 8. Anti-bot & politeness

- **Cloudflare, passive only.** Headers: `server: cloudflare`, `cf-ray`, `cf-cache-status: DYNAMIC`, `nel`, `report-to`. **No `cf-mitigated`, no 403/503 challenge, no JS challenge, no cookie wall.** ~60 requests with a plain curl UA and with no UA at all → 200 every time.
- The `challenge-platform` string present in HTML is the **passive bot-detection beacon** (`/cdn-cgi/challenge-platform/scripts/jsd/main.js`, injected in a 1×1 iframe), not a block. Harmless for `reqwest`.
- **No `cf_clearance` cookie needed.** No Turnstile on kusonime.com itself (Turnstile appears only on the third-party `srnky.com`).
- **robots.txt is maximally permissive:** `User-agent: *` / `Disallow:` (empty) + `Sitemap: https://kusonime.com/sitemap_index.xml`. Nothing disallowed.
- Transport: **HTTP/2**, `accept-encoding: br, gzip` (`vary: Accept-Encoding`). Page is 82,256 B raw → **15,318 B gzip → 14,672 B brotli**. Enable compression; ~5.6× saving. `reqwest` with `gzip`+`brotli` features covers it.
- Rate-limit posture: no `Retry-After`, no 429 across the session. Still, be polite — 1 req/s with a 250 ms floor is more than enough; a 520-page crawl is unnecessary given REST.

## 9. Parsing risks

1. **Soft-404 → 200 homepage.** Unknown slug = `404` + `Location: /`. `reqwest` follows redirects by default → you parse the homepage and report a bogus success. **Disable redirects on detail fetches**, or assert `<h1 class="jdlz">` exists. Highest-severity trap.
2. **No `<body>` class.** `body.single`, `body.category` selectors will silently match nothing.
3. **Synopsis is untagged**, delimited only by `<div class="clear">` after `.info`. Grab `p` siblings of `.info` until `.dlbodz`.
4. **`.info` labels have trailing spaces** (`Genre `, `Seasons `, `Released on`). Match case-insensitively *and* trim — or match on `<b>` prefix before `:`.
5. **`Total Episode` can be `?`** (One Piece). Parse as `Option<u32>`.
6. **`720p` vs `720P`** in the same document. Uppercase before compare.
7. **`<del>` rows are dead links.** `<del datetime="…">Hxfileco</del>` has **no href at all** — a naive `row a[href]` skips it (good), but `row strong` + text-scanning will invent phantom hosts.
8. **`s.id` shortlink aliasing (new posts).** 6 different host labels point to the same 2 tokens; the anchor text is wrong. Detect `s.id` and either resolve through `duniakode.com/raw/<id>` (no captcha) or discard with a clear "unresolvable" state. Do **not** trust `anchor_text` as a host for `s.id` rows.
9. **`Google Sharer` ⇒ acefile.co**, always. Host must be read from the URL.
10. **Page size varies** (10 vs 8 items). Don't assume 10.
11. **`Released on` on cards is time-only** (`12:00 pm`), while detail's `Released on` is a real date (`Jan 02, 2025`). Different semantics, same-ish label.
12. **No score/type/episode on cards** — a list view showing scores *must* fetch detail or use REST `excerpt` (which also lacks them). Budget N detail requests for N scores.
13. **Slugs are not derivable** from titles (`11-eyes-subtitle-indonesia-12`). Always follow `href`.
14. **Ads/markup churn.** Theme is bespoke and unversioned (`kuso.js?ver=4`); class names like `smokeddlrh` are typing-error-grade (`smoke` + `ddl` + `rh`). Expect breakage; keep selectors in one module with fallbacks (`#dl div.smokeddlrh`, `.dlbodz #dl`, and a last-resort `a[href*="acefile.co"], a[href*="mega.nz"]` scan).
15. **SEO spam block near the footer repeats the title** inside a second `<h1>` — `h1` selectors must be `.post-thumb > h1.jdlz`, never bare `h1` (a bare `h1` will also match the sidebar/`Search Results` heading).

## 10. Fixtures

Saved to `anicli-rs/research/raw/kusonime/` (Chrome UA, `curl -sL`, no compression requested — byte-exact server output):

| File | Page type | Bytes |
|---|---|---|
| `home.html` | `/` latest listing | 82,626 |
| `list.html` | `/anime-list-bd/` full A–Z, 1,226 series | 497,263 |
| `search.html` | `/?s=naruto` results | 71,692 |
| `detail.html` | `/ameku-md-batch-subtitle-indonesia/` single batch | 98,977 |
| `batch.html` | `/one-piece-0001-1150-batch-subtitle-indonesia-4/` 18 blocks | 268,369 |

Supporting evidence (not core fixtures, safe to delete): `_home_headers.txt`, `_rest_post.json` (REST single post), `_slugs.json` (1,226 slug↔post-id pairs), `_genre.html`, `_cat.html`, `_acefile.html`, `_ace_dl_url.txt`, `_shortlink.html`, `_paste_raw.txt`, `_srnky.html`, `_avatar_tmp.html`.

## 11. Unknowns

- **Whether *any* Kusonime post ever had per-episode pages.** Absent on all 12 sampled (oldest 2017, newest 2026-11) and every structural marker is missing. I found no evidence of one — treat "never" as likely but unproven.
- **Whether `srnky.com`'s Turnstile is solvable headlessly** and what real host sits behind it. Not attempted; would need a CAPTCHA solve.
- **`advance-search` parameter semantics.** `years=` takes comma-joined *term IDs* (`3549,3412` for 2025), and `genres[]` takes slugs. I confirmed 200 + 70 KB output but **did not verify the filter actually narrows results**.
- **Exact `per_page` ceiling** on REST. 100 worked (headers present); WP caps at 100 by design, not re-probed.
- **Whether `genres`/`seasons` taxonomies are reachable via REST at all.** `/wp/v2/taxonomies` does not list them → assume no.
- **Long-term host stability.** `uptobox.com` is already DNS-dead and `acefile` now demands login; the host set demonstrably decays. A parser should tolerate unknown hosts rather than enum-match.
- **Whether new posts will keep using the `s.id` → duniakode → srnky chain** or revert to direct host links. Mixed in 2026-11 (2 of 6 recent posts used shortlinks, 4 used direct).

---

### Recommended Rust architecture (from the evidence)

1. **Discovery/search/list → REST** (`/wp-json/wp/v2/posts`), not HTML. Cheaper, paginated, `X-WP-Total` for free, no selector churn.
2. **Metadata → HTML detail** (`.lexot > .info`), because REST `content` omits everything except prose.
3. **Download table → HTML detail** (`#dl div.smokeddlrh > div.smokeurlrh`), because REST omits it entirely.
4. **Series index → `/anime-list-bd/`** (one request gets all 1,226 series + post IDs).
5. **Do not build a player path.** Build a "download catalog + metadata" provider; if the TUI needs playback, mark Kusonime's stream capability as `Unavailable` and surface the mirror list as external links.
