# Animeindo — Reconnaissance Report for a Rust (`scraper`) TUI Parser

**Recon date:** 2026-11-15 (Asia/Jakarta), ~04:52–05:00.
**Canonical site verified live:** `https://anime-indo.lol/`
**Target:** document the live theme precisely enough to write and unit-test an offline parser from the saved fixtures.

Every claim below is grounded in an HTTP status code, a saved fixture under `research/raw/animeindo/`, or literal markup quoted from those files. Nothing is inferred from third-party tutorials. This is a **custom theme**, not WordPress — see §2.

---

## 1. Domain & discovery

All four hinted domains return **HTTP 200**, but only one serves anime. Status code alone is useless here — three of the four are landers, one is an unrelated site, and one is a bot-wall.

| Host | Status | Verdict | Distinguishing evidence |
|---|---|---|---|
| **`anime-indo.lol`** | **200**, 22,468 B | **REAL** | `<title>Animeindo - Nonton Streaming Anime Subtitle Indonesia</title>`; canonical + `og:url` self-declare `https://anime-indo.lol/`; `<iframe id="tontonin">` player; 16 `div.list-anime` cards on the home page |
| `animeindo.lol` | 200, 148,314 B | **Unrelated site — Turkish landscaping** | `<title>Animeindo Peyzaj – Animeindo bahce duzenleme ve yesil alan bakimi…</title>`; `generator WordPress 7.1.4`; theme `twentytwentyfive`. The domain was **re-registered and repurposed**; it has zero anime markup. Largest response, so "biggest body" is a trap. |
| `animeindo.site` | 200, 2,969 B | **Parked / for-sale lander** | `<title>animeindo.site - animeindo Resources and Information.</title>`; `server: Parking/1.0`; no theme markup |
| `animeindo.co` | 200, 474 B | **Bot-wall (Joker/JS challenge)** | `<title>Loading...</title>` with `window.location.replace('https://animeindo.co/?ch=1&js=eyJhbGciOiJIUzI1NiIs…')` — a **JWT** (`{"aud":"Joke","js":1,"jti":…}`) plus `sid`. `server: Cowboy` (Erlang). The `?js=` token is the same pattern Otakudesu's `.moe` used. Not fetchable with plain `curl`; needs a JS-capable client. |

### 1.1 Domain provenance

- `robots.txt` (saved) is a permissive three-liner: `User-agent: *` / `Allow: /` / `Sitemap: https://anime-indo.lol/sitemap.xml`. The sitemap self-declares the canonical host.
- The **home page `<meta name="keywords">` still advertises `animeindo, animeindo.cc`** — the `.cc` host is a *dead* bot-wall (`474 B`, `<title>Loading…</title>`, `Joke` JWT), i.e. the historical brand domain, now behind a challenge. Treat `animeindo.cc` as the legacy/origin brand name, `anime-indo.lol` as the live host.
- The footer carries no self-declaration banner (unlike Otakudesu's `div.infoupdate`). **Do not look for a "situs resmi hanya X" banner — this site has none.** Discovery must be by content marker.

### 1.2 Clone/redirect detection heuristic (cheap, ordered)

Accept a candidate host **only** if it passes all of these; reject on the first failure:

1. **Hard reject** if `<title>` lacks both `Animeindo` and `Streaming`/`Nonton` (→ `.lol` Peyzaj, `.site` parking).
2. **Hard reject** if the body is `< 1000 B` **or** contains `window.location.replace(` with a `?js=`/`?ch=` query (→ `.co`, `.cc` bot-walls).
3. **Hard reject** if `server:` is `Parking/1.0` or the title contains `Resources and Information` (→ `.site`).
4. **Accept-**only markers (need **at least two**): `id="tontonin"`, `class="list-anime"`, `class="ngiri"`, `class="kotakcari"`, `class="kaki1"`, `/logos.png`.
5. **Structural reject:** WordPress `generator` meta present → wrong site entirely (→ `.lol`).

Note the naming trap: the **live** host uses a **hyphen** (`anime-indo`), while the repurposed one does not (`animeindo.lol`). A parser pinned to the un-hyphenated form will hit the Turkish landscaping site and see a 200.

---

## 2. Platform & hosting

| Signal | Value |
|---|---|
| `server` | `cloudflare` |
| `cf-ray` | `a5a93de9fcb8f910-SIN` → **Cloudflare, Singapore edge** |
| `cf-cache-status` | `DYNAMIC` (HTML is not cached; every page is origin-rendered) |
| CMS | **None.** No `wp-content`, no `generator`, no `_next`. Hand-rolled **PHP** |
| Theme | Custom, inline `<style>` in `<head>` (no external theme CSS) |
| JS | `jquery.min.js`, `lazyload.min.js` from `/js/`; jQuery-driven server switcher |
| Images | Local `/img/<Name>.jpg`, lazy-loaded via `data-original` + `/loading.gif` placeholder |
| Analytics | Histats (`s10.histats.com`) |
| Ads | Popunder script injected inline (`siteId`, base64-obfuscated hosts) + `//qj.brigsgunster.com/...` |

**Cloudflare is present but not challenging.** Plain `curl` with the stock curl UA, no UA at all, and a Googlebot UA **all returned the identical 22,468 B home page**. There is no Turnstile, no `cf_clearance` cookie, no `?tkn=` gate on the main site. The only interstitial is a third-party **popunder ad** script (which a headless client may or may not execute — irrelevant to parsing).

---

## 3. Capability matrix

| Capability | Verdict | Evidence |
|---|---|---|
| Home / latest updates | **Yes** | 16 `div.list-anime` cards, `div.ngiri` column |
| A–Z anime list | **Yes** | `/anime-list/`, 32 `div.letter` blocks, 425 KB |
| Genre index | **Yes** | `/list-genre/` |
| Genre per-tag | **Yes** | `/genres/<tag>/` links on detail pages |
| Search | **Yes**, but **302 → pretty URL** | `/search.php?q=` → `302 /search/<slug>/` |
| Anime detail | **Yes** | `/anime/<slug>/`, 170 episode links for Black Clover |
| Episode list | **Yes** | single `div.ep` block, `<a>` per episode |
| Episode page | **Yes** | `/…-episode-N/`, `h1.title`, `iframe#tontonin` |
| Stream resolution | **Yes — direct MP4, no cipher** | `play.xtwap.top` → `<source src="…googlevideo…">`, HTTP 206 |
| HLS | **Yes** | `hls.php` link → 480p/720p/1080p master; `segment_00000.m4s` = 398,826 B, 200 |
| Movie catalogue | **Yes** | `/movie/`, **different** markup (`table.otable`) |
| Schedule | **Yes** | `/jadwal/` |
| Download links | **Yes** | `a[href*="dl.xtwap.top"]` + `gdplayer.to/f/` |
| RSS | Not found | — |

---

## 4. URL patterns (verified)

| Page | Pattern | Example |
|---|---|---|
| Home | `/` | `https://anime-indo.lol/` |
| Detail | `/anime/<slug>/` | `/anime/black-clover-2nd-season/` |
| **Episode** | `/<slug>-episode-<N>/` — **root level, not nested** | `/black-clover-2nd-season-episode-7/` |
| A–Z list | `/anime-list/` | — |
| Genre index | `/list-genre/` | — |
| Genre tag | `/genres/<tag>/` | `/genres/shounen/` |
| Movie | `/movie/` | — |
| Schedule | `/jadwal/` | — |
| Search (form) | `/search.php?q=<term>` → **302** |  |
| Search (canonical) | `/search/<slug>/` | `/search/one-piece/` |

### 4.1 Episode-slug rules that will bite you

1. **Episodes live at the site root**, *not* under `/anime/<slug>/`. A parser that concatenates detail-URL + episode slug will 503.
2. **The episode slug is not always the detail slug.** Verified pair, same series:
   - `/anime/black-clover-2nd-season/` → episodes `/black-clover-tv-episode-01/` … `/black-clover-episode-170-tamat/`
   - So the season detail page links to a **differently-named** episode family (`-tv-`, and a `-tamat` suffix on the finale).
   **Never derive episode URLs — always read them from the detail page's `div.ep`.**
3. **Zero-padding varies by series**: Naruto Shippuden uses **3 digits** (`/naruto-shippuden-episode-001/` → 200), Black Clover uses **2** (`-episode-01`), and current-season shows use **unpadded** (`-episode-7`). `/naruto-shippuden-episode-01/` (2-digit) → **503**. Again: scrape the links, don't construct them.
4. **Non-ASCII slugs are percent-encoded with lowercase hex**: `/ranma-%c2%bd-2024-3rd-season-episode-7/`, `/magical%e2%98%85explorer-episode-8/`. Preserve the href verbatim; do not re-encode or decode-then-reencode (case of `%c2%bd` matters to the origin).
5. **Search slug is a naive lowercase + spaces→hyphens transform, and it does NOT strip punctuation**:
   - `One Piece` → `/search/one-piece/`
   - `jujutsu kaisen` → `/search/jujutsu-kaisen/`
   - `one_piece` → `/search/one_piece/` (underscore passes through untouched)
   - `Black Clover!!` → `/search/black-clover!!/` (`!` survives)
   So a client must **follow the 302** rather than build the slug, or it will mis-encode.

---

## 5. DOM map (exact selectors)

All verified by re-parsing the saved fixtures. Selectors are CSS as consumed by the Rust `scraper` crate; the theme is a flat `<div>` soup with **no `<article>`, no `id` on content containers**, so class-based selection is mandatory.

### 5.1 Shared chrome (every page)

```css
#header > .logo > a > img[src="/logos.png"]     /* site logo */
table.tableup td.notd a                          /* 4 nav buttons: List/Genre/Movie/Jadwal */
.kotakcari form[action="/search.php"] input[name="q"]   /* search box */
.kaki1 a                                         /* Disclaimer / Privacy */
#footer                                          /* "© 2026 Animeindo" */
```

### 5.2 Home — `/`

```css
div.ngiri                    /* main column, 65% width */
div.ngiri > div.menu         /* the card grid wrapper */
div.ngiri a                  /* card link -> /<slug>-episode-N/  (ROOT level!) */
div.ngiri div.list-anime     /* one card, 16 of them */
div.list-anime > img.lazy[data-original]   /* poster; src is /loading.gif placeholder */
div.list-anime > p           /* series title */
div.list-anime > span.eps    /* episode number, bare digits e.g. "7" */
div.nganan                   /* sidebar, 35% */
div.nganan table.ztable      /* "Popular" rows */
table.ztable td.zvithumb img /* sidebar poster */
table.ztable td.zvidesc a    /* sidebar title + genres after <br> */
```

**Lazy-load gotcha:** posters are `src="/loading.gif"` with the real URL in **`data-original`**. Selecting `img` without reading `data-original` yields 16 identical placeholder URLs.

### 5.3 Detail — `/anime/<slug>/`

Container is **`div.ngirix`** (note the `x` — home uses `div.ngiri`). This asymmetry is a real trap.

```css
div.ngirix h1.title                          /* series title */
div.ngirix div.menu div.detail img           /* poster */
div.ngirix div.menu div.detail h2            /* title repeated */
div.ngirix div.detail li a[href^="/genres/"] /* genre tags */
div.ngirix div.detail p                      /* synopsis (Indonesian) */
div.ngirix div.ep                            /* ONE wrapper div holding ALL episode links */
div.ngirix div.ep a                          /* one <a> per episode, text is the padded number */
```

**Critical shape:** there is exactly **one** `div.ep`, and it contains ~170 sibling `<a>` elements (Black Clover). It is **not** one `div.ep` per episode. Selector must be `div.ep a`, not `div.ep` iterated as episodes.

The detail page has **no** server/iframe — streams exist only on episode pages.

### 5.4 Episode — `/<slug>-episode-<N>/`

```css
div.ngiri h1.title                       /* "Black Clover 2nd Season Episode 7 Subtitle Indonesia" */
div.nonton iframe#tontonin               /* the active player; src = first server */
iframe#tontonin[src]                     /* also carries allowfullscreen/scrolling attrs */
div.servers a.server                     /* all mirrors; class "server cur" marks active */
div.servers a.server[data-video]         /* ABSOLUTE or root-relative player URL */
div.servers a.server[data-video]::text   /* label: B-TUBE, CEPAT, GDRIVE, MP4, MP4 HD, YUP, YUP HD */
div.nav div.navi a                       /* Prev / Semua Episode / Next */
div.nav div.navi a[href$="-episode-"]    /* episode nav; text may be « Prev / Next » */
div.nav a[href*="dl.xtwap.top"]          /* download page */
div.nav a[href*="gdplayer.to/f/"]        /* GDrive download */
h2.title + div.menu div.detail           /* episode blurb + poster + synopsis */
```

Episode pages contain **no** `div.ep` (0 occurrences) — they are leaves, so a "next episode" crawl must use `div.nav a` or go back to the detail page.

**Mirror count is variable and not fixed:** Black Clover S2E7 → **3** servers (`B-TUBE`, `CEPAT`, `GDRIVE`); One Piece E1000 → **5** servers (`GDRIVE`, `MP4`, `MP4 HD`, `YUP`, `YUP HD`). Newer episodes prefer `mp4upload`/`yourupload`; older catalog episodes prefer `xtwap`/`gdplayer`. **Do not hardcode a server name** — iterate `a.server` and resolve whatever is present.

The switcher is trivial jQuery, so the `data-video` attribute is the whole contract:

```js
$("a#allmiror").click(function(e){ e.preventDefault();
  $("a.server").removeClass("cur"); $(this).addClass("cur");
  $("#tontonin").attr("src", $(this).attr("data-video")); $("#tontonin").show(500); });
```

Note all mirrors share the **duplicate `id="allmiror"`** — invalid HTML. Select by `class="server"`, never by that id.

### 5.5 Movie — `/movie/` (different markup)

```css
table.otable                 /* row per movie; NOT div.list-anime */
td.vithumb img               /* poster */
td.videsc a                  /* title -> /anime/<slug>/ */
td.videsc span.label         /* "Movie", year, etc. */
```
Movie rows reuse `div.ngiri` but use `table.otable`, **not** `div.list-anime` — a shared home/movie card selector will return zero on one of them.

### 5.6 A–Z — `/anime-list/`

```css
div.pilih a                  /* jump bar # A–Z */
div.anime-list               /* wrapper */
div.letter                   /* 32 letter sections; the <a name> value is malformed */
div.letter + li a            /* entries are bare <li> (no <ul>!) */
```
The letter anchors are **broken HTML**: `<a name="""></a>"` — quote corruption. Do not rely on `a[name]` for section labels; read the visible text sibling instead.

### 5.7 Search — `/search/<slug>/`

Result container is `div.menu`; entries are `a[href^="/anime/"]`. **`div.list-anime` count is 0** — the search results page does **not** use the home card component. Select `a[href^="/anime/"]` and de-duplicate (every hit appears twice in the markup).

### 5.8 Genre — `/list-genre/`

Not fully mapped (fixture saved, `genre.html`). The `.ngiri` container is present; genre links are `/genres/<tag>/`.

---

## 6. AJAX / dynamic behaviour

**There is no XHR API.** No `fetch`, no `XMLHttpRequest`, no JSON endpoint, no `_next/data`. Everything is server-rendered HTML.

The only client-side behaviour:
1. **Server switcher** — swaps `iframe#tontonin.src` from `a.server[data-video]` (§5.4). Pure DOM, no network.
2. **LazyLoad** — `img.lazy[data-original]` → `src`, plus a `.lazy1` variant.
3. **Popunder ad injection** — inline obfuscated script; may `window.open`/rewrite. Irrelevant to parsing, but a headless browser must tolerate popups.

Consequence: **the parser needs no JS engine.** A single GET of the episode page yields every mirror URL in `data-video`. This is strictly simpler than Otakudesu or Samehadaku.

---

## 7. Stream extraction

### 7.1 The chain

```
episode HTML  →  a.server[data-video]  →  player.php  →  <source src>  →  media
```

`data-video` hosts observed, all **live and 200**:

| Server label | `data-video` shape | Resolves to |
|---|---|---|
| `B-TUBE` | `https://play.xtwap.top/btube3.php?url=<opaque>` | **progressive MP4** (googlevideo) |
| `CEPAT` | `https://play.xtwap.top/cepat2.php?url=<opaque>` | **HLS** `.m3u8` (jwplayer) |
| `GDRIVE` | `https://gdplayer.to/embed/?<opaque>` | Google Drive third-party player (JS) |
| `GDRIVE` (newer) | `https://play.xtwap.top/hls.php?link=<opaque>` | HLS |
| `MP4` / `MP4 HD` | `https://www.mp4upload.com/embed-<id>.html` | third-party embed iframe |
| `YUP` / `YUP HD` | **root-relative** `/yup.php?url=https://www.yourupload.com/embed/<id>` | iframe → yourupload |

**Watch the relative form:** `YUP` links are `/yup.php?url=…` — their token is **not** opaque (it is the plaintext yourupload URL, nested once). Resolve relative to `https://anime-indo.lol`.

### 7.2 Direct vs iframe

- **`play.xtwap.top` (B-TUBE / CEPAT / hls.php) = DIRECT.** The player page is a thin HTML wrapper whose `<source>` / jwplayer `sources[].file` is the real media URL. Two GETs: episode → player → media. **No cipher, no `?url=` decoding needed** — the opaque token is *passed through*, not decrypted by the client, so a Rust client only has to forward the whole `data-video` URL.
- **`gdplayer.to`, `mp4upload.com`, `yourupload.com` = IFRAME/JS.** Their embed pages need JS or a further redirect chain. Lower priority; `xtwap` covers the catalogue.

### 7.3 B-TUBE → progressive MP4 (verified playable)

`https://play.xtwap.top/btube3.php?url=…` returns ~920 B:

```html
<video id="mediaplayer" class="video-js vjs-default-skin" …>
  <source src="https://rr1---sn-u125g5-5q.googlevideo.com/videoplayback?expire=…&ip=…&itag=18&source=blogger&…&sig=…" type="video/mp4">
</video>
```

- Host is **`*.googlevideo.com`**, `source=blogger`, `itag=18` → **360p MP4 (video+audio muxed)**, `dur=1430.093`.
- **Playable from a server with no browser context:** `Range: bytes=0-2047` → first response **302**, following it → **HTTP 206, 2,048 bytes**. So the googlevideo `ip=` parameter did **not** block this datacenter egress (`ipbypass=yes` / `mip=` appear in the redirect). **Do not assume IP-lock, but do re-test from the deployment host** — this is the one result most likely to differ by network.
- Expiry is short: `expire=1794722032` ≈ **8 h** from fetch. Never cache a media URL across sessions.

### 7.4 CEPAT → HLS (verified playable)

`cepat2.php` returns a jwplayer setup with
`"file":"https://hls.xtwap.top/video/data2/<64-hex>/<64-hex>/index.m3u8"`.

Master manifest (221 B) offers **480p / 720p / 1080p**:
```
#EXT-X-STREAM-INF:BANDWIDTH=496000,…,RESOLUTION=854x480,CODECS="avc1.4d401f,mp4a.40.2"   → 480p/index.m3u8
#EXT-X-STREAM-INF:BANDWIDTH=1078000,…,RESOLUTION=1280x720                              → 720p/index.m3u8
#EXT-X-STREAM-INF:BANDWIDTH=1960000,…,RESOLUTION=1920x1080,CODECS="avc1.640028,…"      → 1080p/index.m3u8
```
Media playlist uses **fMP4**: `#EXT-X-MAP:URI="init_2.mp4"`, `segment_00000.m4s`, `#EXT-X-PLAYLIST-TYPE:VOD`. First segment fetched: **HTTP 200, 398,826 B, `application/octet-stream`**. **HLS is the quality path; B-TUBE is capped at 360p.**

### 7.5 Required headers

| Request | Needs |
|---|---|
| Site HTML | **Nothing special.** No UA, curl UA, and Googlebot UA all returned identical 200s |
| `play.xtwap.top`, `hls.xtwap.top` | **No `Referer` required** — 200 with and without (919 B vs 916 B). Send a normal desktop UA anyway |
| `googlevideo.com` | No `Referer` needed; honours `Range`. Sent UA was a desktop Chrome string |
| `mp4upload` / `yourupload` | Untested beyond status; assume a browser UA + `Referer: https://anime-indo.lol/` |

There is **no hotlink protection** on the `xtwap` stack — a plain `GET` works. This is the easiest extraction target of the sites surveyed so far.

---

## 8. Anti-bot

- **Cloudflare present, not enforcing.** `cf-cache-status: DYNAMIC`, edge SIN. No challenge on any HTML route, including from a bare `curl` with no UA.
- **No rate limiting observed** across ~25 requests in 8 minutes (no 429).
- **No `?tkn=` gate**, no `cf_clearance` requirement, no cookie handshake.
- The real friction is **not** bot defence but **server flakiness**: see §9.1.
- `play.xtwap.top` sets `<meta name="referrer" content="no-referrer">` in its own player pages — a deliberate anti-hotlink measure at the *media* layer that, empirically, is not enforced.

---

## 9. Risks

### 9.1 `503` is the generic error status — and it fires on valid URLs (HIGH)

`GET /naruto-shippuden-episode-1/` → **503, 0 bytes**, twice, back-to-back. The **correct** slug `/naruto-shippuden-episode-001/` → **200, 7,257 B**. Also:

| Request | Result |
|---|---|
| `/this-does-not-exist-episode-999/` | 503, 0 B |
| `/anime/this-does-not-exist/` | 503, **288 B** |
| `/naruto-shippuden-episode-01/` | 503, 0 B |

So **503 does double duty as 404 and as transient origin failure**, and the two are **indistinguishable by status code** (only a 288 B body on the detail-404 hints at a rendered error page). A naive client that maps 503 → "not found" will silently drop legitimate episodes; one that maps 503 → "retry" will hammer on genuinely missing slugs. **Mitigate:** treat 503 as *retry-then-skip* with a bounded backoff, and prefer the **links scraped from the detail page** over any constructed URL.

### 9.2 URL derivation is unsafe (HIGH)

Three independent traps (§4.1): root-level episodes, season-slug ≠ episode-slug, and inconsistent zero-padding. **Always crawl `div.ep a` from the detail page.** This is the single largest correctness risk in the design.

### 9.3 Two nearly-identical container classes (MEDIUM)

Home/sidebar use `div.ngiri`; detail uses **`div.ngirix`**. A typo'd selector silently returns the sidebar instead of the detail column, or empty.

### 9.4 Duplicate `id="allmiror"` (MEDIUM)

Every mirror `<a>` carries the same id. Selecting `#allmiror` picks the first server only; the switcher's own jQuery relies on this invalid markup. Use `a.server`.

### 9.5 Lazy-loaded posters (LOW, but a silent-wrong-answer bug)

`img.lazy` has `src="/loading.gif"`; the real poster is in `data-original`. Reading `src` yields 16 identical placeholder URLs — a failure that produces *plausible-looking* output.

### 9.6 Media URLs expire (~8 h) (MEDIUM)

`googlevideo` `expire=` is ~8 h out. HLS paths contain two 64-hex opaque segments that rotate. Never persist resolved media URLs; resolve per playback.

### 9.7 Stream-host concentration & volatility (MEDIUM)

Nearly all playable content funnels through **`play.xtwap.top` / `hls.xtwap.top`** — a single third party. If it changes path shape (`btube3.php` → `btube4.php`) every stream breaks. `btube3`/`cepat2` are **versioned filenames**, i.e. explicitly unstable. Prefer discovering the player URL from `data-video` over templating it.

### 9.8 Malformed HTML (LOW)

`<a name=""">`, bare `<li>` outside any `<ul>`, duplicated `id`. The `scraper` crate's html5ever recovery handles these, but **fixture-based unit tests must be written against the real saved HTML**, not cleaned-up hand-written samples.

### 9.9 Naming trap between real and repurposed domains (MEDIUM)

`animeindo.lol` (no hyphen) = Turkish landscaping site, HTTP 200, 148 KB. A fallback list that probes un-hyphenated variants will "succeed" with garbage. Validate by content marker (§1.2), never by status.

---

## 10. Fixtures

All under `research/raw/animeindo/`. Size is on-disk bytes.

| File | Source | Bytes | Notes |
|---|---|---|---|
| `probe/anime-indo.lol.html` | `GET /` | 22,468 | **REAL** home |
| `probe/anime-indo.lol.headers` | same | 574 | `cf-ray …-SIN`, `cf-cache-status: DYNAMIC` |
| `probe/animeindo.lol.html` | `GET /` | 148,314 | Turkish Peyzaj — **negative** fixture |
| `probe/animeindo.site.html` | `GET /` | 2,969 | parking — **negative** fixture |
| `probe/animeindo.co.html` | `GET /` | 474 | Joke JWT bot-wall — **negative** fixture |
| `probe/*.headers` | 4 hosts | — | status/redirect evidence |
| `episode.html` | `/black-clover-2nd-season-episode-7/` | 19,715 | **primary stream fixture**; 3 servers, `div.nonton`, `div.nav` |
| `episode.headers` | same | 574 | |
| `e_one-piece-episode-1000.html` | `/one-piece-episode-1000/` | 21,187 | **5-server** fixture; `mp4upload` + `yup.php` relative links |
| `e_black-clover-2nd-season-episode-7.html` | same as `episode.html` | 19,687 | independent capture |
| `detail.html` | `/anime/black-clover/` | 23,982 | `div.ngirix`, 1 `div.ep`, 170 links, 5 genres |
| `naruto_detail.html` | `/anime/naruto-shippuden/` | 40,872 | 3-digit episode slugs |
| `s2.html` | `/search.php?q=one+piece` **followed** | 59,523 | final `/search/one-piece/`; no `div.list-anime` |
| `search_redirect.html` | `/search.php?q=one+piece` | 0 | the **302** itself (empty body) |
| `animelist.html` | `/anime-list/` | 425,882 | 32 `div.letter`, malformed anchors |
| `movie.html` | `/movie/` | 29,239 | `table.otable` variant |
| `jadwal.html` | `/jadwal/` | 17,319 | |
| `genre.html` | `/list-genre/` | 22,404 | |
| `stream/xtwap_btube3.html` | player page | 1,320 | **MP4 `<source>`** (360p googlevideo) |
| `stream/xtwap_cepat.html` | player page | 2,019 | **HLS** jwplayer `sources[].file` |
| `stream/gdplayer_embed.html` | gdplayer | 100,090 | JS-heavy; only `drive-thirdparty.googleusercontent.com` visible statically |

Raw working copies (incl. the `naruto-shippuden-episode-1` 503 capture) remain in `raw/animeindo/work/`.

---

## 11. Unknowns / not verified

1. **Playability from the final deployment host.** Media resolved and `206`-ed from *this* machine; the googlevideo `ip=` parameter suggests possible geo/IP sensitivity (`ipbypass=yes` was present, which is why it worked). **Re-test §7.3 on the target host before committing to MP4.**
2. **`gdplayer.to`, `mp4upload`, `yourupload` extraction.** Status 200 confirmed; the actual media URL behind them was **not** extracted (needs JS). If `xtwap` is ever deprecated, this becomes the blocker.
3. **`/genres/<tag>/` page structure** not mapped (only linked from detail).
4. **`/jadwal/` and `/list-genre/` internals** not mapped beyond capture.
5. **Pagination.** No `div.pag` observed on home or A–Z (home shows 16 "Update Terbaru" only; there is a `.pag` **CSS rule** in the inline stylesheet but no pagination markup in any captured page). **Unknown whether an "older updates" route exists** — home appears to be a fixed 16-card window.
6. **`sitemap.xml`** referenced by `robots.txt` but not fetched.
7. **Rate-limit ceiling** unknown beyond ~25 requests (no 429 seen).
8. **Whether `animeindo.cc` (Joke wall) would yield the same origin** if solved — not attempted; it is a bot-wall, and `.lol` already serves content.
9. **Episode count ceiling** — Black Clover detail lists 170 links; larger series (One Piece) untested for whether `div.ep` truncates.
