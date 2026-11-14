# Otakudesu — Reconnaissance Report for a Rust (`scraper`) TUI Parser

**Recon date:** 2026-11-14 (Asia/Jakarta), ~20:37–20:50.
**Canonical site verified live:** `https://otakudesu.blog/`
**Target:** document the classic Otakudesu theme precisely enough to write and unit-test an offline parser from the saved fixtures.

Everything below is grounded in HTTP status codes, raw HTML saved under `raw/otakudesu/`, or literal sample text quoted from those files. Nothing was inferred from third-party tutorials — those are actively *wrong* for the current build (see §9.1).

---

## 1. Domain & discovery

### 1.1 The real site right now: `otakudesu.blog`

| Check | Result |
|---|---|
| `GET https://otakudesu.blog/` | **HTTP 200**, final URL unchanged (no redirect), 40,996 B decoded |
| `<title>` | `Otaku Desu \| Nonton dan Streaming Anime Subtitle Indonesia` |
| Doctype | `<!DOCTYPE html PUBLIC "-//W3C//DTD XHTML 1.0 Transitional//EN" …` |
| Branding | `<img src="https://otakudesu.blog/wp-content/uploads/2019/08/otakudesu.png">` inside `div#header > .center > .logo` |
| Theme | `wp-content/themes/Animestream/style.css` |
| WordPress | `<meta name="generator" content="WordPress 5.6.23" />` |
| Canonical | `<link rel="canonical" href="https://otakudesu.blog/" />` |
| OG site name | `Otaku Desu` |

### 1.2 The site *self-publishes* its current domain

The home page contains an explicit banner — this is the single most reliable discovery signal:

```html
<div class="infoupdate">Situs resmi Otakudesu hanya otakudesu.blog, selain dari ini adalah situs palsu.</div>
```
*(“Otakudesu's only official site is otakudesu.blog; anything other than this is a fake site.”)*

**Scope caveat (verified):** the *text* appears only on `/` . On `/ongoing-anime/` the string `infoupdate` occurs too, but only inside a `<style>` block (`.infoupdate{overflow:hidden;…}`) — no banner text. So: fetch the **home page** and read `div.infoupdate` to discover the canonical host; do not expect it on every page.

There is no separate "domain moved to X" redirect page in play — the announcement is plain text in the theme.

### 1.3 Candidate-by-candidate verdict (all probed live)

| Host | Status | Verdict | Distinguishing evidence |
|---|---|---|---|
| **`otakudesu.blog`** | **200** | **REAL** | WP 5.6.23 + theme `Animestream`, `venser`/`detpost`/`.epz` markup, `/anime/<slug>-sub-indo/`, canonical self-declaration |
| `otakudesu.cam` | 200 | **Dead / parked** | 6,990 B only; `<h1>This domain is for sale</h1>`, `<meta name="robots" content="noindex,noarchive,nofollow">`, a contact form, **zero** Otakudesu markup. The "200" in prior recon was a false positive — a for-sale lander. |
| `otakudesu.cloud` | **523** | Dead | Cloudflare `error code: 523` (origin unreachable) |
| `otakudesu.moe` | **401** | Bot-wall | `401 Forbidden` + redirect to `http://ww547.otakudesu.moe/?tkn=19vnMT3S…` — Cloudflare-style token gate, no content |
| `otakudesu.buzz` | 200 | **Clone / rebrand** | Own theme `otakudesu-buzz`, `od-*` CSS classes, posters from `static.anipop.xyz`, **random slug suffixes** (`/anime/one-piece-2iinq9w/`), routes carry suffixes (`/complete-anime-pjyfmyt/`, `/genre-list-wfqv6kt/`), **`/episode/…` → 404** (no streaming) |
| `otakudesu.today` | 200 | **Clone (catalogue)** | Identical codebase to `.dev`; routes are MAL-ID based `/anime/21-one-piece/`, `/anime/1535-death-note/`; all classic paths 404; title *"Rekomendasi Anime Sub Indo, Pilih Tontonan 2026"*; describes itself as a **"portal katalog anime"** |
| `otakudesu.dev` | 200 | **Clone (catalogue)** | Byte-identical routing to `.today` (same `/anime/<malid>-<slug>/` list), different marketing copy |
| `otakudesu.digital` | 200 | **Separate Next.js app** (not classic) | `_next/static`, SSR, own routes `/anime/<slug>/ep-<n>#player`, asset API `/api/assets/posters/<hash>`, no WordPress. Has *real* content incl. episode links. Not the classic theme — treat as a distinct parser target, not a mirror. |

### 1.4 Clone-detection heuristic (cheap, ordered — implement as a host validator)

Reject a candidate host if **any** of these fire; accept only if the first three pass:

1. **Hard reject:** body contains `This domain is for sale` or is `< 10 KB` with `<meta name="robots" content="noindex,noarchive,nofollow">` (→ `.cam`).
2. **Hard reject:** response is `523`/`401`/`403` or final URL contains `?tkn=` (→ `.cloud`, `.moe`).
3. **Hard reject:** no `div.venser` **and** no `wp-content/themes/Animestream` in the home document (→ all remaining clones; `.digital` fails here by design).
4. **Hard reject:** every internal `a[href]` matches `/anime/\d+-` (MAL-ID routing) → `.today`, `.dev`.
5. **Hard reject:** internal paths end in a `-[a-z0-9]{7}/` base36 suffix (`/complete-anime-pjyfmyt/`) or the theme path is `themes/otakudesu-buzz` → `.buzz`.
6. **Accept:** home page 200, contains `class="venser"`, `themes/Animestream`, `/ongoing-anime/` in nav, and (best) `div.infoupdate` naming a host — prefer the host named by that banner over `Host:`.

A practical host-picking strategy: keep a static candidate list, fetch `/` for each with a short timeout, run rules 1–6, and cache the winner for the session. The banner is authoritative when present, so **trust the banner over the list**.

---

## 2. Platform

- **CMS:** WordPress **5.6.23** (`<meta name="generator">`).
- **Theme:** `Animestream` — path `https://otakudesu.blog/wp-content/themes/Animestream/`. This is *the* classic Otakudesu theme; the `venser`/`venutama`/`detpost`/`epz`/`mirrorstream` class names all come from it and are stable across years.
- **Plugins detected:** Yoast SEO (`class="yoast-schema-graph"`, `og:`/`twitter:` tags, `yoast-schema-graph` JSON-LD), **WP Rocket** page cache (HTML footer comment: `<!-- This website is like a Rocket, isn't it? Performance optimized by WP Rocket. … Debug: cached@1794170954 -->`), a lazy-load/`data-wpel-link` link rewriter (every internal link carries `data-wpel-link="internal"`).
- **Front-end:** XHTML 1.0 Transitional doctype, **`<BODY>` uppercase** in the source (harmless — html5ever lowercases tag names), `lang="id-ID"`, jQuery **1.12.0** and **3.2.1** both loaded, Bootstrap 3 CSS, Font Awesome, Dashicons.
- **Edge:** Cloudflare (`server: cloudflare`, `cf-ray: …-SIN`, `cf-cache-status: DYNAMIC`, `alt-svc: h3`). **No bot challenge** — see §8.
- **Ads:** heavy. Promo `<div class="iklan">` blocks (`bit.ly/OTAKU*` → Gambling ads) appear above *and* below content on list pages, plus an `#overplay` ad overlay on episode pages. These are the main source of junk text/links.

---

## 3. Capability matrix

| Capability | Available | Where / how |
|---|---|---|
| Search | **Y** | `GET /?s=<q>&post_type=anime` — plain HTML form, **no AJAX**. `post_type=anime` is **required** (omitting it drops from 5 hits to 2 and pollutes with blog posts). Hard cap **12 results**, stated in-page. |
| Ongoing list | **Y** | `GET /ongoing-anime/` (+ `/page/N/`), 25 items/page, 6 pages at recon time |
| Completed list | **Y** | `GET /complete-anime/` (note: `complete-`, **not** `completed-`) |
| Popular | **N** | `/anime-populer/` and `/populer/` both return 200 but are an **interstitial redirect page** (`<title>Otakudesu Official</title>`, “Anda sebentar lagi akan dialihkan ke website Otaku Desu”, a “Klik Di Sini” button). **No popular ranking exists.** Do not implement. |
| Genre browse | **Y** | `GET /genres/<genre>/` (+ `/page/N/`), 48 pages for `action`; genre index at `GET /genre-list/` (36 genres) |
| A–Z browse | **Y** | `GET /anime-list/` — 865 KB, grouped by `#`, `A`…`Z` |
| Detail (synopsis / genre / status / score / studio / type / episodes) | **Y** | `GET /anime/<slug>/` — full `.infozingle` block + `.sinopc` synopsis + episode & batch lists |
| Episode list | **Y** | On the detail page (`.episodelist`), plus a `<select id="selectcog">` jump-menu on every episode page |
| Stream / mirror extraction | **Y** (two-tier) | Default player **statically in HTML** (`#pembed iframe[src]`); *alternate* servers only via `admin-ajax.php` (§6, §7) |
| Direct download links | **Y** | Episode `.download` and batch `.batchlink` rows → `link.desustream.com/?id=…` redirector → real file host (1 hop, no JS) |
| Batch | **Y** | `GET /batch/<slug>/` (linked from every detail page) |
| Release schedule | **N (stale)** | `/jadwal-rilis/` returns 200 but is frozen at **“Jadwal Rilis Anime On-Going (Spring 2023)”** — 16 occurrences of “Spring 2023”, no anime links in the content area. Dead. |
| JSON API | **N** | No `/wp-json` exposure used by the theme; only the two `admin-ajax.php` actions (§6) |

---

## 4. URL patterns (real, live examples)

| Page type | Pattern | Live example (verified 200) |
|---|---|---|
| Home | `/` | `https://otakudesu.blog/` |
| Ongoing | `/ongoing-anime/` , `/ongoing-anime/page/N/` | `https://otakudesu.blog/ongoing-anime/page/2/` |
| Completed | `/complete-anime/` , `/complete-anime/page/N/` | `https://otakudesu.blog/complete-anime/` |
| Search | `/?s=<query>&post_type=anime` | `https://otakudesu.blog/?s=kaguya&post_type=anime` |
| Genre index | `/genre-list/` | `https://otakudesu.blog/genre-list/` |
| Genre browse | `/genres/<genre>/` , `/genres/<genre>/page/N/` | `https://otakudesu.blog/genres/action/page/2/` |
| A–Z index | `/anime-list/` | `https://otakudesu.blog/anime-list/` |
| Detail | `/anime/<slug>/` | `https://otakudesu.blog/anime/super-yani-suu-futari-sub-indo/` |
| Episode | `/episode/<slug>/` | `https://otakudesu.blog/episode/snudysf-episode-1-sub-indo/` |
| Batch | `/batch/<slug>/` | `https://otakudesu.blog/batch/snudysf-batch-sub-indo/` |
| Schedule *(dead)* | `/jadwal-rilis/` | `https://otakudesu.blog/jadwal-rilis/` |

**Slug convention.** Anime slugs are `title-kebab-case` + `-sub-indo` (history shows `-subtitle-indonesia` too — e.g. `kaguya-sama-kokurasetai-subtitle-indonesia` — so **never synthesize slugs**, always take them from `a[href]`). Episode slugs are a **hand-written abbreviation**, not derivable from the anime slug: anime `super-yani-suu-futari-sub-indo` → episode `snudysf-episode-1-sub-indo`, batch `snudysf-batch-sub-indo`. Anime `kusuriya-hitorigoto-s3-sub-indo` → `knh-s3-episode-5-sub-indo`. Extract, never construct.

---

## 5. DOM map

> All selectors below were confirmed against `raw/otakudesu/*.html`. Sample text is verbatim.

### 5.1 List pages (home / ongoing / completed) — identical structure

Container: `.venser > .venutama > .rseries > .rapi > .venz > ul`
Item: `li > .detpost`  (25 per page; `detpost` count == `<li>` count == 25 on all three)

| Field | Selector | Sample 1 | Sample 2 |
|---|---|---|---|
| Item root | `.venz > ul > li > .detpost` | — | — |
| Episode / total eps | `div.epz` | `Episode 6` *(rendered; raw is `<span class="dashicons dashicons-desktop"></span> Episode 6`)* | `Episode 7` |
| Day **or** score | `div.epztipe` | `Jumat` (ongoing) | `8.20` (completed) |
| Date | `div.newnime` | `14 Nov` | `14 Nov` |
| Detail URL | `div.thumb > a[href]` | `https://otakudesu.blog/anime/shuiro-kamen-sub-indo/` | `https://otakudesu.blog/anime/kusuriya-hitorigoto-s3-sub-indo/` |
| Thumbnail | `div.thumb img[src]` | `…/uploads/2026/10/159977.jpg` | `…/uploads/2026/10/160277-1.jpg` |
| Title (clean) | `h2.jdlflm` | `Shuiro no Kamen` | `Kusuriya no Hitorigoto Season 3` |
| Title (with suffix) | `div.thumb img[alt]` | `Shuiro no Kamen Sub Indo` | `Kusuriya no Hitorigoto Season 3 Sub Indo` |

Completed-page samples (`.epz` becomes a *total*, `.epztipe` becomes the *rating*):

| `.epz` | `.epztipe` | `.newnime` | `h2.jdlflm` |
|---|---|---|---|
| `12 Episode` | `8.20` | `03 Nov` | `Super no Ura de Yani Suu Futari` |
| `12 Episode` | `6.88` | `14 Okt` | `Saijo no Osewa` |

> ⚠️ **`.epz`/`.epztipe` are semantically overloaded.** On `/ongoing-anime/` → `.epz` = latest episode number, `.epztipe` = **airing weekday**. On `/complete-anime/` → `.epz` = **total** episode count, `.epztipe` = **score**. A parser must branch on the source page, not guess from the text alone. (`epztipe` text starting with a day name ⇒ ongoing; matching `^\d+\.\d+$` ⇒ completed.)

Pagination: `.pagination .pagenavix` with `span.page-numbers.current`, `a.page-numbers[href]`, `a.next.page-numbers`.
**Inconsistent wrapper:** on `/ongoing-anime/` it is `.pagination > .pagenavix`; on `/genres/<g>/` it is `.pagination > .pagipagi > .pagenavix`. Use the descendant selector `.pagination .page-numbers` and ignore the intermediate div.

### 5.2 Search page

Container: `.venser > .venutama > .page > ul.chivsrc > li` (max 12)
The result count and cap are announced in the heading: `Hasil Pencarian (Max. hanya sampai 12 hasil)`.

| Field | Selector | Sample 1 | Sample 2 |
|---|---|---|---|
| Item | `ul.chivsrc > li` | — | — |
| Thumbnail | `li > img[src]` | `…/uploads/2026/01/Kaguya-sama-wa-Kokurasetai.jpg` | `…/uploads/2022/06/Kaguya-sama-wa-Kokurasetai-Season-3-Sub-Indo.jpg` |
| Title + URL | `li > h2 > a[href]` | `Kaguya-sama wa Kokurasetai: Otona e no Kaidan Sub Indo` | `Kaguya-sama wa Kokurasetai Season 3 (Episode 1 – 13) Subtitle Indonesia` |
| Genres | `div.set:nth-of-type(1) a[href]` | `Comedy, Drama, Romance, School, Seinen` | `Comedy, Psychological, Romance, School, Seinen` |
| Status | `div.set:nth-of-type(2)` | `Status : Ongoing` | `Status : Completed` |
| Rating | `div.set:nth-of-type(3)` | `Rating : ` *(empty)* | `Rating : 9.13` |

Structure detail: each `div.set` is `<div class="set"><b>Label</b> : value</div>`, so label-keyed parsing (`b` text → sibling text) is more robust than positional `.set` indexing. **Rating is frequently empty** — sample 1 above has `Rating : ` with nothing after it.

### 5.3 Detail page (`/anime/<slug>/`)

| Field | Selector | Sample |
|---|---|---|
| Title (full) | `.jdlrx > h1` | `Super no Ura de Yani Suu Futari (Episode 1 – 12) Subtitle Indonesia` |
| Poster | `.fotoanime > img[src]` | `…/uploads/2026/09/Super-no-Ura-de-Yani-Suu-Futari-Sub-Indo.jpg` |
| Info block | `.fotoanime .infozin .infozingle p > span` | see below |
| Synopsis | `.sinopc p` (multiple `<p>`, join) | `Mengisahkan Sasaki, seorang budak korporat yang sudah terlalu lelah…` |
| Episode list | `.episodelist` (2nd such block) | see below |
| Batch list | `.episodelist` (1st block, header contains `Batch`) | see below |

**Info block** — each row is `<p><span><b>Label</b>: value</span></p>` (note: **no space** before the colon in the raw HTML, and a leading newline/indent inside `<span>`). Labels observed, in order:

```
Judul: Super no Ura de Yani Suu Futari
Japanese: スーパーの裏でヤニ吸うふたり
Skor: 8.20
Produser: TBS, Asmik Ace, Nichion, AbemaTV, arma bianca, Bit grooove promotion, ADK Marketing Solutions
Tipe: TV
Status: Completed
Total Episode: 12
Durasi: 23 Menit
Tanggal Rilis: Jul 09, 2026
Studio: Asahi Production
Genre: <a href="…/genres/comedy/" rel="tag">Comedy</a>, <a …>Romance</a>, <a …>Seinen</a>
```

Parse as `(b.text().trim(), span_text_after_b)` and key on the **Indonesian** labels above. ⚠️ The **batch page uses different, English labels** for the same fields (§5.5).

**Episode / batch lists** — two sibling `.episodelist` blocks, each:

```html
<div class="episodelist">
  <div class="smokelister"><span class="monktit">… Episode List …</span></div>
  <ul>
    <li>
      <span><a href="https://otakudesu.blog/episode/snudysf-episode-12-sub-indo/" …>Super no Ura de Yani Suu Futari Episode 12 (End) Subtitle Indonesia</a></span>
      <span class='zeebr' style='float:right'>2 November,2026</span>
    </li>
```

- Item: `.episodelist > ul > li`
- Link + label: `li > span > a[href]`, text `… Episode 12 (End) Subtitle Indonesia`
- Date: `li > span.zeebr` → `2 November,2026` (note: **no space** after the comma, and month name in Indonesian)
- Episode number must be extracted from the anchor text with a regex (`Episode (\d+)`), plus `(End)` for the finale. Do **not** rely on ordering: the list is **newest-first** (12 → 1).
- Distinguish the blocks by their `.monktit` text containing `Batch` vs `Episode List`, **not** by position — the batch block is absent on some titles.

### 5.4 Episode page (`/episode/<slug>/`)

| Field | Selector | Sample |
|---|---|---|
| Title | `h1.posttl` | `Super no Ura de Yani Suu Futari Episode 1 Subtitle Indonesia` |
| Meta | `.kategoz` | `Posted by aoichi` / `Release on 10:59 am` (each in its own `<span>`) |
| Episode jump-menu | `select#selectcog > option[value]` | `value="https://otakudesu.blog/episode/snudysf-episode-2-sub-indo/"` text `Episode 2`; first option is `value="0"` “Pilih Episode Lainnya” |
| Prev/next + back | `.prevnext .flir a[href]` | `See All Episodes` → `/anime/super-yani-suu-futari-sub-indo/`; `Next Eps.` → `/episode/snudysf-episode-2-sub-indo/` |
| **Default stream** | `#pembed iframe[src]` | `https://desustream.com/dstream/otakuwatch7/new/index.php?id=R2p3S0pJRE5Vd0NUT3NyaDhFN3BZUT09` |
| Mirror groups | `.mirrorstream > ul.m360p` / `ul.m480p` / `ul.m720p` | see §7 |
| Download rows | `.download > ul > li` | see §7 |

`#pembed` is `<div class="player-embed" id="pembed"><div class="responsive-embed-stream"><iframe …></div>` — **the default stream is fully present in the static HTML.** No JS needed for the first (and usually only) player.

### 5.5 Batch page (`/batch/<slug>/`) — different layout!

| Field | Selector | Sample |
|---|---|---|
| Title | `.jdlrx > h1` | `Super no Ura de Yani Suu Futari [BATCH] Subtitle Indonesia` |
| Poster | `.animeinfo .kanan .imganime img[src]` | `…/Super-no-Ura-de-Yani-Suu-Futari-Sub-Indo.jpg` |
| Info | `.animeinfo .kiri .data .infos` — `<b>Label</b>: value<br />` lines | `Judul`, `Japanese`, **`Type`**, **`Episodes`**, **`Rating`**, `Genre`, `Duration`, `Studios`, `Producers`, `Aired`, `Credit` |
| Total | `.totalepisode .total` | `12` |
| Back-link | `.totalepisode h3 a[href]` | `…/anime/super-yani-suu-futari-sub-indo/` (“Download Episode!”) |
| Download rows | `.batchlink > ul > li` | `<strong>MP4 360p</strong>` + hosts |

⚠️ **Label drift between detail and batch:** detail uses `Tipe / Skor / Total Episode / Durasi / Tanggal Rilis / Studio / Produser`; batch uses `Type / Rating / Episodes / Duration / Aired / Studios / Producers`. Any shared info model needs both spellings. Also note `.infos` uses `<br />` separators, not `<p>`, and the synopsis block `.deskripsi` is **empty** (`<b>Sinopsis:</b>` followed by nested empty divs) — do not expect a synopsis on batch pages.

### 5.6 Genre browse + A–Z list — a *third* layout

Genre pages and `/anime-list/` use the card layout, **not** `.detpost`:

Container: `.venser > .page > .col-anime-con` → `div.col-anime`

| Field | Selector | Sample 1 | Sample 2 |
|---|---|---|---|
| Card | `.col-anime` | — | — |
| Title + URL | `.col-anime-title > a[href]` | `Tank Chair` → `/anime/tank-chair-sub-indo/` | `Tempal: Item no Chikara` → `/anime/tempal-chikara-sub-indo/` |
| Studio | `.col-anime-studio` | `Polygon Pictures` | `J.C.Staff` |
| Eps | `.col-anime-eps` | `Eps` *(blank!)* | `Unknown Eps` |
| Rating | `.col-anime-rating` | *(empty)* | *(empty)* |
| Genres | `.col-anime-genre a[href]` | `Action, Gore, Super Power` | `Action, Fantasy` |
| Poster | `.col-anime-cover img[src]` | `…/2026/10/Tank-Chair.jpg` | — |
| Synopsis | `.col-synopsis p` | `Walau dijuluki sebagai pembunuh bayaran terkuat…` | — |
| Season | `.col-anime-date` | `Fall 2026` | — |
| Trailer/Download | `.col-anime-trailer > a[href]` | `/anime/tank-chair-sub-indo/` | — |

(49/48 pages exist per genre; the “Eps” column being literally `Eps` with no number is a live bug on the site.)

**A–Z index**: `.daftarkartun > #abtext > .bariskelom` grouped by letter (`a[name='A']` inside `.barispenz`), each group holding `.penzbar > .jdlbar > ul > li > a.hodebgst[href][title]`, e.g. `href="/anime/compass-20-sub-indo/" title="#Compass 2.0: Sentou Setsuri Kaiseki System (Episode 1 – 12) Subtitle Indonesia"`. Text content is polluted by inline `<color>`/`<span>` colour swatches — prefer the `title` attribute.

**Genre index** (`/genre-list/`): `ul.genres > li > a[href]` with **relative** hrefs (`/genres/action/`, `title="View all posts in Action"`). 36 genres. Note the relative-vs-absolute inconsistency with detail pages.

### 5.7 Fragile selectors — flag list

| Risk | Detail | Mitigation |
|---|---|---|
| `.epz` / `.epztipe` overload | episode-vs-total, day-vs-score | branch on source page + regex shape |
| `h2.jdlflm` vs `img[alt]` | alt appends ` Sub Indo`; on genre cards alt is **stale/wrong** (`Tank Chair` card had `alt="Bakemonogatari Subtitle Indonesia"`) | always prefer `h2.jdlflm` / `.col-anime-title a` text |
| `div.thumb a[title]` | often `title=""` on list pages, populated on completed | ignore `title` |
| Label spelling drift | `Tipe` vs `Type`, `Skor` vs `Rating`, `Studio` vs `Studios` | dual label maps |
| Rating often empty | `Rating : ` then nothing (search, genre cards) | `Option<f32>` |
| `.pagipagi` extra wrapper | only on genre pagination | use `.pagination .page-numbers` |
| Ad blocks interleaved | `div.iklan` above/below content, `#overplay` overlay, `div.rvad` headings | scope all queries under `.venser` and prefer `.venz`/`.col-anime`/`.detpost` roots |
| Duplicate `.episodelist` | batch block sometimes absent | select by `.monktit` text |
| **Unclosed `<ul class="m720p">`** | see §7.1 — the 720p mirror `<ul>` is closed by `</div>`, never `</ul>` | html5ever recovers this (the list still parses as a child of `.mirrorstream`), but do not assume a well-formed sibling chain |
| Date formats | `14 Nov`, `03 Nov`, `2 November,2026`, `Jul 09, 2026`, `Fall 2026` | store as string; parse lazily |
| Inline `<style>`/`<script>` in content area | every list page injects CSS inside `.venser` | `text()` concatenation must skip style/script (scraper does this by default for text, but beware `html()` dumps) |

---

## 6. AJAX / JSON endpoints  ★ preferred path for mirrors

The theme ships exactly **two** `admin-ajax.php` actions, both of which are plain `application/x-www-form-urlencoded` POSTs returning `{"data": …}`. Both were reproduced live with `curl`. No cookies, no CSRF token beyond the nonce, no JS execution required.

**Endpoint (both):** `POST https://otakudesu.blog/wp-admin/admin-ajax.php`

### 6.1 Action `aa1208d27f29ca340c92c66d1926f13f` — get nonce

```
POST /wp-admin/admin-ajax.php
Content-Type: application/x-www-form-urlencoded
action=aa1208d27f29ca340c92c66d1926f13f
```
Response: `{"data":"0e1e7d1413"}` — an opaque 10-hex-char nonce. (Saved: `raw/otakudesu/ajax/nonce.json`.)

### 6.2 Action `2a3505c93b0035d3f455df82bf976b84` — resolve a mirror to an embed

```
POST /wp-admin/admin-ajax.php
action=2a3505c93b0035d3f455df82bf976b84
id=<int>&i=<int>&q=<"360p"|"480p"|"720p"> &nonce=<from 6.1>
```
`id`, `i`, `q` come from the base64 `data-content` attribute (§7.1). Response: `{"data":"<base64 of an HTML fragment>"}`; the fragment is

```html
<div class="responsive-embed-stream"><iframe src="https://desustream.com/dstream/otakuwatch7/new/index.php?id=R2p3S0p…" …></iframe></div>
```
i.e. **`base64_decode(data)` → parse `<iframe src>` → that is the embed URL**, which is itself inserted into `#pembed` by the theme's JS. (Saved: `raw/otakudesu/ajax/mirror_480p_i0.json`.)

### 6.3 Error shapes (important for tests)

| Condition | HTTP | Body |
|---|---|---|
| `q=360p` when the episode has no 360p row | 200 | `{"message":"Not found."}` — **no `data` key** |
| invalid / stale nonce | 200 | `{"message":"Forbidden."}` |
| valid | 200 | `{"data":"…"}` |

So: presence of `message` ⇒ failure; presence of `data` ⇒ success. Both errors are **HTTP 200**, so status-code checks are useless — key on the JSON body.

### 6.4 Notes

- The nonce is **session/IP-scoped and reusable** within a short window; fetching it once per run is enough (it survived across 8 consecutive resolve calls in testing).
- `Referer: https://otakudesu.blog/<episode-url>` and `X-Requested-With: XMLHttpRequest` were sent in testing and worked; they were **not** independently proven necessary. Send them — they cost nothing.
- There is **no AJAX for search, pagination, or episode lists**. Those are plain server-rendered HTML. Do not go looking for one.

---

## 7. Stream / mirror extraction

### 7.1 Tier 1 — default stream, **no AJAX** (do this first)

`#pembed iframe[src]` is in the static HTML and is the server the site itself auto-plays:

```
https://desustream.com/dstream/otakuwatch7/new/index.php?id=R2p3S0pJRE5Vd0NUT3NyaDhFN3BZUT09   (480p episode, otakuwatch7)
https://desustream.net/dstream/odcdn/?id=TXdXSlhBdGVKMGNnWkQyZHF4NWR0dVV1U2luU0ZuMXM3SERCazE2MUxpU0FOWmN5NFArbEZDNGN1NGdiV1pyRw==   (720p episode, odcdn)
```

### 7.2 Tier 2 — alternate servers, via the `data-content` blob + AJAX

`div.mirrorstream` holds one `<ul>` per resolution. Each `<a>` has **`href="#"`** and a **base64 JSON payload** in `data-content`:

```html
<ul class="m480p"><span class="dashicons dashicons-desktop"></span> Mirror 480p
  <li><a href="#" data-content="eyJpZCI6MjA2Nzk4LCJpIjowLCJxIjoiNDgwcCJ9" data-default="true">otakuwatch7</a></li>
  <li><a href="#" data-content="eyJpZCI6MjA2Nzk4LCJpIjoxLCJxIjoiNDgwcCJ9">odstream</a></li>
  …
</ul>
```

`base64_decode(data-content)` → `{"id":206798,"i":0,"q":"480p"}`. The **label is the host name in lower-case, often with a trailing space** (`otakuwatch7hd `, `vidhide `) — trim it.

**Resolution groups present:** `ul.m360p`, `ul.m480p`, `ul.m720p` — that is the complete set; **no `m1080p`**. A group may be **present but empty** (the `m360p` `<ul>` on `snudysf-episode-1` has only the header text and zero `<li>`), so an empty group means "no such source", not a parse failure.

**Resolved embed URLs, verified by calling the AJAX for every combination on `snudysf-episode-1`:**

| `q` | `i` | label | embed URL |
|---|---|---|---|
| 480p | 0 | otakuwatch7 | `https://desustream.com/dstream/otakuwatch7/new/index.php?id=R2p3S0pJRE5Vd0NUT3NyaDhFN3BZUT09` |
| 480p | 1 | odstream | `https://desustream.net/dstream/arcg/?id=YVQ4RXNpWHJOZmVDdVdkSWF6djZiYUpnZ3FLakhMOXB6WG5hcXBZNmNhMG5rQjU0VmtLZG5oSDcxTnFXb1dDVQ==` |
| 480p | 2 | vidhide | `https://odvidhide.com/embed/hkan4e5u5b5d` |
| 480p | 3 | mega | `https://mega.nz/embed/bvxDHQyR#IiNU3B4XM6m9LuWuuOTkDzIsNqzYSJGxp2sjkBxVxFQ` |
| 720p | 0 | otakuwatch7hd | `https://desustream.com/dstream/otakuwatch7/new/hd/index.php?id=R2p3S0pJRE5Vd0NUT3NyaDhFN3BZUT09` |
| 720p | 1 | odstreamhd | `https://desustream.net/dstream/arcg/?id=YVQ4RXNpWHJOZmVDdVdkSWF6djZiYUpnZ3FLakhMOXB6WG5hcXBZNmNhMWFLTXA2MWo4T0pndTBMRUM5Z1RyYQ==` |
| 720p | 2 | vidhide | `https://odvidhide.com/embed/wbmt3sj55u7l` |
| 720p | 3 | mega | `https://mega.nz/embed/PvIDkSiR#hOwCrqETxfV6fknqJ9gidML5lMZDfhabVGuX_BkwL04` |
| 360p | 0–3 | — | `{"message":"Not found."}` (no 360p rows on this episode) |

**Host / label vocabulary observed (union over 3 episodes + batch):**

- Embed hosts: `desustream.com`, `desustream.net`, `odvidhide.com`, `mega.nz`
- Mirror labels: `otakuwatch7`, `otakuwatch7hd`, `odstream`, `odstreamhd`, `odcdn`, `ondesu2hd`, `blogs`, `filedon`, `vidhide`, `mega`
- Download hosts: `PDrain`/`Pdrain`, `Viking`, `VikingFile`, `VidHide`, `Mega`, `Mp4Up`, `Filedon`, `Acefile`, `AceFile`, `GoFile`, `OtakuDrive`, `DesuDrive`

**Label sets differ per episode** — this is not a fixed enum. `snudysf-episode-1` offers `otakuwatch7/odstream/vidhide/mega` (480p+720p only); `knh-s3-episode-5` offers `filedon/vidhide/mega` (360p), `odcdn/blogs/filedon/vidhide/mega` (480p), `odstream/ondesu2hd/filedon/vidhide/mega` (720p). Treat server names as opaque strings; never hard-code an index→host mapping.

⚠️ **Malformed HTML:** the `ul.m720p` element is closed by `</div></div>` instead of `</ul>` on every episode page I checked. Exact raw tail:

```html
…<li><a href="#" data-content="…">mega </a></li> </div></div>
```

html5ever recovers this — `ul.m720p li a[data-content]` still matches — but **do not assume `.mirrorstream > ul` siblings are well-formed**, and don't build a test fixture assertion on "3 complete `<ul>` elements".

### 7.3 Download links (episode + batch)

Episode `.download` block:

```html
<div class="download"> <h4>Super no Ura de Yani Suu Futari Episode 1 Subtitle Indonesia</h4>
  <ul><li>
    <strong>360p</strong>
    <a href="https://link.desustream.com/?id=d2JkL0hDRWxReWo4…" target="_blank" rel="nofollow external …">PDrain</a>
    <a href="…">Viking</a> <a href="…">VidHide</a> <a href="…">Mega</a> <a href="…">Mp4Up</a>
    <i>33.1 MB</i>
  </li> …
```
- Row = `.download > ul > li`; resolution = `li > strong`; host links = `li > a[href]` (text = host name); size = `li > i` (`33.1 MB`).
- **`<strong>` label variants — all three coexist in the wild:**
  - bare: `360p`, `480p`, `720p`
  - `Mp4 360p`, `Mp4 480p`, `Mp4 720p` *(lower-case “p”, as in “Mp4”)*
  - `MKV 480p`, `MKV 720p`, `MKV 1080p` *(upper-case “MKV”, and 1080p **does** appear here even though it never appears in the stream mirrors)*
  - batch page: `MP4 360p` *(upper-case “MP4”!)*
  
  ⇒ parse case-insensitively with a regex like `(?i)^(mp4|mkv)?\s*(\d{3,4})p$` and capture the container + height. This is exactly the *old* documented format and it still occurs — but **not on every episode**, so a parser that only matches `Mp4 720p` will silently return zero download links on ~half of episodes.
- `h4` sometimes carries an uploader credit: `Kusuriya no Hitorigoto Season 3 Episode 5 Subtitle Indonesia – [Samehadaku]` (content is partly sourced from Samehadaku).

Batch `.batchlink` block: same row shape, `<strong>MP4 360p</strong>` + hosts `OtakuDrive, DesuDrive, PDrain, Mega, AceFile, Viking`.

**All download hrefs are the redirector `https://link.desustream.com/?id=<base64>`** whose payload is an opaque 44-char token (not client-decodable — server-side mapping). **Following it is a plain 1-hop redirect requiring no JS**, with a browser UA:

```
GET https://link.desustream.com/?id=d2JkL0hDRWxReWo4YUlhaDBYdk5ab1pJbEpWWVBKWmZTRjBxNFc2M3NxenQ=
  → 301/302 → https://pixeldrain.com/u/H4PEEAoC   (PDrain = pixeldrain)
```
So download resolution = `HEAD`/`GET` with `-L` and record `url_effective`. No scraping of the landing page is needed.

### 7.4 From embed page → actual media (what mpv needs)

The `desustream` embed is a **trivial 947-byte page** with a direct `<video><source>` pointing at Google Video — no JS, no obfuscation, no `eval(p,a,c,k,e)`):

```html
<title>Test Otakudesu - 360p</title>
<video controls>
  <source src="https://rr4---sn-a5msenle.googlevideo.com/videoplayback?expire=…&amp;id=f67be6dda094ec6e&amp;itag=18&amp;source=blogger&amp;…&amp;sig=…" type="video/mp4">
</video>
```
(Saved: `raw/otakudesu/extras/embed-desustream.html`, URL in `extras/media_url.txt`.)

**Verified playback requirements:**

| Header | Result |
|---|---|
| Browser **UA only**, no `Referer`, following the 1 redirect | **`206 Partial Content`, `content-type: video/mp4`** ✅ |
| `Referer: https://otakudesu.blog/` added | also `206 video/mp4` (harmless, not required) |
| **Empty UA** | **`403`** ❌ |

So for mpv: a **User-Agent is mandatory**, `Referer` is not. `mpv --user-agent="Mozilla/5.0 … Chrome/131.0.0.0 Safari/537.36" "<source url>"`.

**Three caveats that make this the most fragile part of the whole pipeline:**

1. The URL is **IP-bound**: `…&ip=176.97.117.248…` (the requester's egress IP) — it only plays from the IP that fetched the embed page, and the redirect adds `ipbypass=yes`. Resolve and play from the same host/IP.
2. It has a **hard expiry**: `expire=1794692624` (≈6 h from issue). Cache the *embed URL*, never the googlevideo URL.
3. The `<title>` said `360p` while the `#pembed` iframe was the 480p group — the embed page's own title is not a reliable resolution indicator.
4. `desustream.com` vs `desustream.net` (`/dstream/arcg/`) are two different resolvers; `odvidhide.com` and `mega.nz` embeds were **not** resolved further — `mega` in particular cannot be played from a direct URL without MEGA-specific decryption. Practical guidance: **try the `desustream` servers only** and treat `mega`/`vidhide` as "open in browser".

---

## 8. Anti-bot & politeness

| Probe | Result |
|---|---|
| `curl` with **no UA at all** | **200**, 40,996 B, full page — *no challenge* |
| `curl -A "curl/8.23.0"` | 200, 40,996 B |
| Browser UA | 200, 40,996 B |
| Empty UA against **googlevideo** media | **403** (the only place a UA matters) |
| `/episode/…` with browser UA + `Referer` | 200 |
| Cloudflare challenge (JS/`cf_clearance`/`Just a moment`) | **absent** — Cloudflare is a pure reverse proxy here (`cf-cache-status: DYNAMIC`) |
| HTTP/2 | ✅ `HTTP/2 200` |
| HTTP/1.1 (forced) | ✅ 200, comparable latency |
| Compression | `content-encoding: zstd` (also `gzip`/`br` accepted); `--compressed` is required to get the 41 KB decoded body, otherwise 7.9 KB compressed |
| Latency (5 sequential `/ongoing-anime/`) | 0.32 s, 0.61 s, 0.30 s, 0.30 s, 0.32 s (≈0.3 s median, SIN edge) |
| Rate limiting | **none observed** at ~20 requests over 10 min; no 429, no `Retry-After` |
| `robots.txt` | Minimal and effectively allow-all: `User-agent: *` (no rules), then `User-Agent: Googlebot / Allow: *.js / Allow: *.css`. No `Crawl-delay`, no `Disallow`. |

**Recommendation:** a browser UA + `--compressed` (or reqwest with `gzip`/`brotli`/`zstd`) is sufficient; a `Referer` of the referring otakudesu page is polite and free. There is **no Cloudflare bypass work to do** — `bladebro`/headless is unnecessary for this site. Still throttle to ~1 req/s per host: the site is ad-supported and small, and `robots.txt` gives you no cover to hammer it. WP Rocket means repeated identical GETs may be served from cache, so `cf-cache-status`/`last-modified` can verify freshness if you ever need to detect a content change.

---

## 9. Parsing risks

### 9.1 Tutorials and old parsers are wrong about this site

Historically documented Otakudesu mirror format is `Mp4 720p (ZippyShare)` as a **link label**, and `#pembed` was described as containing a `mirrorstream` iframe. Current reality:

- **ZippyShare is gone**; hosts are now bot-upload CDNs (`filedon`, `acefile`, `gofile`, `mp4up`, `pixeldrain`, `vikingfile`).
- Mirror `<a>` elements have **`href="#"`** and **no useful href** — an old parser that harvests `a[href]` from `.mirrorstream` gets `#` for everything and silently produces zero streams. The real payload is **base64 in `data-content` + an AJAX call**.
- `.mirrorstream` now sits **outside** `#pembed` (it's a sibling of `#lightsVideo`), so `#pembed .mirrorstream` matches nothing.
- Download labels are `<strong>res</strong>` + `<a>host</a>`, not `Mp4 720p (ZippyShare)` text.

Assume any pre-2023 selector set is broken and derive from the fixtures.

### 9.2 What changes most often (ranked)

1. **Mirror server labels and their counts** — per-episode, unpredictable. Never enumerate-validate.
2. **`data-content` `id`** — a per-episode integer (206798, 207137…). Must be read from HTML.
3. **Download `<strong>` label formatting** — `360p` / `Mp4 360p` / `MKV 1080p` / `MP4 360p`, mixed case, mixed container prefix.
4. **Ad markup** — `div.iklan`, `#overplay`, `div.rvad`, `#iklantengah1`, `div.bloxkln` come and go, and inject text/links into the content area.
5. **Episode slugs** — uploader-chosen abbreviations; not derivable.
6. **`.epz`/`.epztipe` semantics** per list page (see §5.1) — this is a *stable* overload, but easy to get wrong once and never notice.
7. **Ratings/eps missing** — `Rating : ` empty, `.col-anime-eps` = `Eps`, genre-card `img[alt]` stale.

### 9.3 Defensive-parsing guidance

- **Scope every query** under the content root (`.venser`, then the specific container) so ad blocks can't leak in.
- **Every field is `Option`**: score, rating, day, total-episode, size, synopsis, `Japanese`/`Producers`/`Credit`. All are genuinely absent somewhere in the saved fixtures.
- **Parse labels, not positions.** `div.set > b` and `.infozingle b` are label-keyed; positional `.set:nth-of-type(n)` breaks the moment a field is omitted.
- **Accept both label spellings** (`Tipe|Type`, `Skor|Rating`, `Studio|Studios`, `Total Episode|Episodes`, `Tanggal Rilis|Aired`, `Durasi|Duration`).
- **Trim and normalise**: `.epz` text arrives with a leading space and an inline `<span>` icon; mirror labels arrive with a trailing space (`vidhide `); info values arrive with a leading newline.
- **Decode entities**: `&#8211;` (–), `&#8230;`, `&hellip;` (…), `&amp;` in URLs (`googlevideo` URLs are `&amp;`-escaped in the embed HTML — must be unescaped before use).
- **Fallback selectors** worth wiring:
  - list item: `.detpost` → fallback `.venz > ul > li`
  - title: `h2.jdlflm` → fallback `.thumb a[title]` → `img[alt]` (strip trailing ` Sub Indo`)
  - detail title: `.jdlrx h1` → fallback `h1`
  - detail info: `.infozingle p span` → fallback `.fotoanime .infozin` full text + regex
  - stream: `#pembed iframe[src]` → fallback `div.player-embed iframe[src]` → any `iframe[src*="desustream"]`
  - mirrors: `.mirrorstream a[data-content]` (ignore the `ul.m*p` wrapper entirely — it's the malformed part)
  - downloads: `.download li` → fallback `.batchlink li`
  - pagination: `.pagination .page-numbers` (ignore `.pagenavix`/`.pagipagi`)
- **Fixture-test the two label formats**: `episode.html` (bare `360p`) and `extras/episode-samehadaku-format.html` (`Mp4 360p` + `MKV 1080p`) must both yield 3 and 6 rows respectively. Same for the two list semantics (ongoing vs completed) and the two info-label vocabularies (detail vs batch).
- **Host validation before parsing** (§1.4) — a parser pointed at a clone returns *plausible HTML with zero items*, which is the worst failure mode.

---

## 10. Fixtures

Base: `anicli-rs/research/raw/otakudesu/`

| File | Page type | Source URL | Bytes |
|---|---|---|---|
| `home.html` | Home (25 items, `div.infoupdate` banner) | `https://otakudesu.blog/` | 40,996 |
| `ongoing.html` | Ongoing list (25 items, 6 pages) | `/ongoing-anime/` | 42,826 |
| `completed.html` | Completed list (25 items) | `/complete-anime/` | 45,028 |
| `search.html` | Search results (5 items, cap 12) | `/?s=kaguya&post_type=anime` | 18,563 |
| `detail.html` | Anime detail (info + batch + 12 episodes) | `/anime/super-yani-suu-futari-sub-indo/` | 34,409 |
| `episode.html` | Episode (bare `360p` labels, m480p/m720p mirrors) | `/episode/snudysf-episode-1-sub-indo/` | 41,463 |
| `extras/batch.html` | Batch page | `/batch/snudysf-batch-sub-indo/` | 31,851 |
| `extras/genre.html` | Genre browse, card layout (48 pages) | `/genres/action/` | 40,118 |
| `extras/genre-list.html` | Genre index (36 genres) | `/genre-list/` | 22,900 |
| `extras/anime-list.html` | A–Z index | `/anime-list/` | 865,681 |
| `extras/episode-samehadaku-format.html` | Episode, **`Mp4`/`MKV` labels**, m360p present | `/episode/knh-s3-episode-5-sub-indo/` | 45,583 |
| `extras/popular.html` | `/anime-populer/` — interstitial, proves Popular is N/A | `/anime-populer/` | 15,677 |
| `extras/jadwal.html` | `/jadwal-rilis/` — stale Spring 2023 | `/jadwal-rilis/` | 40,089 |
| `extras/embed-desustream.html` | desustream embed w/ direct googlevideo URL (`&amp;`-escaped) | `desustream.com/dstream/otakuwatch7/new/index.php?id=…` | 1,440 |
| `extras/media_url.txt` | Extracted `<source src>` (IP-bound, expiring) | — | 750 |
| `extras/s2.html` | Search **without** `post_type=anime` — proves the param is required (2 vs 5 hits) | `/?s=kaguya` | 19,297 |
| `extras/kd.html` | Kusuriya detail (used to derive `knh-s3-…` slugs) | `/anime/kusuriya-hitorigoto-s3-sub-indo/` | 31,562 |
| `ajax/nonce.json` | Action `aa12…` response | — | 21 |
| `ajax/mirror_480p_i0.json` | Action `2a35…` response (base64 embed HTML) | — | 363 |
| `ajax/probe/*.html` | Six candidate home pages used for clone analysis | — | 3 KB–224 KB |

---

## 11. Unknowns

Stated plainly — these were **not** established and should not be assumed:

1. **Whether `otakudesu.blog` is "official" in any ownership sense.** The evidence is that the site *declares itself* the only official domain (and the layout/theme is the canonical one). Whether it is the same operator as historical `otakudesu.*` domains cannot be determined from outside.
2. **`otakudesu.digital`'s relationship to `.blog`.** It reuses the identical `-sub-indo` slug convention and has real episode content, but is a separate Next.js app with its own asset API. It could be an official rebuild, a scraper-frontend, or a mirror. Not determined. Its selectors are entirely different from the classic theme and it was **not** mapped in this report.
3. **`mega.nz` and `odvidhide.com` embed resolution.** Only `desustream.com` / `desustream.net` were followed to a playable `video/mp4`. MEGA requires client-side key derivation; VidHide likely uses its own obfuscated player. Both are correctly *identified* but not *playable* from the data gathered.
4. **Non-Google-Video sources.** The one embed resolved happened to be `source=blogger` (Google Video). Whether every `desustream` embed is a plain `<video><source>`, or some use an HLS `m3u8` / packed JS player, was not sampled across enough episodes to conclude. Only **one** embed page was fetched.
5. **Lifetime of the `admin-ajax` action hashes.** `aa1208d27f29ca340c92c66d1926f13f` and `2a3505c93b0035d3f455df82bf976b84` are baked into the theme's inline JS; they will change if the theme is updated. No versioning signal exists to detect that in advance — the failure mode is `{"message":"Forbidden."}` / `"Not found."`, so treat those as "actions rotated, re-scrape the inline script".
6. **Nonce TTL.** Verified reusable for ~1 minute across 8 calls; the exact expiry window was not measured.
7. **Whether the `id` in `data-content` is globally unique or per-post.** It varies per episode (206798, 207137) but the numbering scheme was not reverse-engineered — read it, never compute it.
8. **Rate limits at scale.** No 429 observed at ~20 requests. Behaviour at hundreds of requests/minute is unknown; the site sits behind Cloudflare and could start challenging under load.
9. **Geo-blocking.** All requests originated from one Indonesian-adjacent egress; behaviour from other regions (or whether `desustream`/`googlevideo` are geo-restricted) is untested.
10. **The `/jadwal-rilis/` content.** The page shell is stale (Spring 2023) but whether a *current* schedule lives elsewhere (e.g. a sidebar widget on home) was not investigated beyond confirming the dedicated page is dead.
11. **`otakudesu.moe`.** Behind a `?tkn=` gate; whether it serves the classic site or is parked could not be determined without solving the challenge (deliberately not attempted).
