# Oploverz — Technical Reconnaissance for Rust `scraper` Parsers

- **Target**: Oploverz / "Oploverz Plus" (Indonesian anime sub/stream index)
- **Recon date**: 2026-11-15 (Asia/Jakarta, UTC+7)
- **Method**: `curl` desktop Chrome 131 UA, `--compressed`, HTTP/2; raw HTML saved; `lxml` XPath validation of the selectors below
- **Fixtures**: `anicli-rs/research/raw/oploverz/` (12 MB)
- **Scope note**: no Rust code written. Every claim carries an observed status/header/size/sample from a real fetch.

---

## 1. Domain & discovery

### Live hosts probed (2026-11-15)

| URL | HTTP | Final URL | Verdict |
|---|---|---|---|
| `https://oploverz.diy/` | **200** | same | **Active primary.** SvelteKit, server-rendered, `x-sveltekit-page: true` |
| `https://oploverz.diy/series` | 200 | same | Full A–Z catalog (991 series) |
| `https://oploverz.plus/` | 301 | `https://link.oploverz.ltd/` | Link hub, **522** origin down — dead end |
| `https://oploverz.in/` | 301 | `https://link.oploverz.ltd/` | same dead hub |
| `https://oploverzvip.com/` | 200 | same | WordPress 7.1.3 + theme `dramastream`; streaming content but **no official markers** → clone candidate |
| `https://oploverz.website/` | 200 | same | WP clone, title says "Partner Anoboy & Samehadaku" |
| `https://oploverz.live/` | 200 | `https://ww2.oploverz.live/` | WP, `ww2.` wildcard host → parked/clone farm |
| `https://oploverz.net/` | 200 | `https://ww17.oploverz.net/` | `ww17.` pattern → parked |
| `https://oploverz.co/` | 302 | `https://ww38.oploverz.co/` | `ww38.` pattern → parked |
| `https://oploverz.mov/` | 200 | same | 236 KB "portal" page, links out, not a streaming site |
| `https://oploverz.org/` | 200 | same | 7.6 KB portal page |
| `https://oploverz.biz/` | 526 | same | Invalid SSL certificate |
| `https://oploverz.fit/.one/.site/.pw/.top/.xyz/.app` | 000 | — | DNS/connection failures |

`.site` is an alias of `.diy` (200, same 28,968-byte HTML, same title).

### How the active domain is announced

1. **In-band brand text** on `oploverz.diy`:
   > `Web Asli Oploverz hanya oploverz plus. Selalu tonton Anime Garapan Oploverz hanya di oploverz asli yang minim iklan.`
   and the `<title>Oploverz Plus | Web Asli Oploverz`.
2. **Official social links** (identical on every real page):
   `https://t.me/fansuboploverz`, `https://t.me/mitraoploverz`,
   `https://www.facebook.com/oploverz.official`,
   `https://www.instagram.com/oploverz_plus/`, `https://www.threads.com/@oploverz_plus`.
3. **Asset/API origin**: every poster/stream asset is served from `https://backapi.oploverz.ac/...`; `robots.txt` points its sitemap at `https://anime.oploverz.ac/sitemap.xml`.
4. Search-engine result for the brand surfaces `.diy`, `.mov`, `.website`, and the `154.203.162.163` bare-IP mirror — the bare IP and `ww*` hosts are rotator/parking artifacts.

**Discovery recommendation for Rust**: keep `https://oploverz.diy` as default; on failure, probe `oploverz.site` (alias). Do not trust `oploverz.plus` — it redirects to a 522 link hub. Validate a candidate by requiring **all** of: HTTP 200, `x-sveltekit-page: true` header, `backapi.oploverz.ac` assets, and `/series` returning >1 MB.

### Parked / clone detection (observed signals)

| Signal | Real `.diy` | Clone / parked |
|---|---|---|
| Response header | `x-sveltekit-page: true` | absent; `x-opl: dyn1` (vip) or none |
| CMS | SvelteKit `_app/immutable/*` | WordPress `wp-content/themes/dramastream/` |
| URL shape | `/series/{slug}/episode/{n}` | `/az-list/`, `/genres/aksi/`, `?s=` |
| Assets | `backapi.oploverz.ac/uploads/posters/...` | own `/wp-content/uploads/` |
| Hostname | `oploverz.diy` | `wwNN.oploverz.*` (parking wildcard), bare IP |
| SSL / status | 200 | 526 invalid SSL, 301→522 |
| Brand text | "Web Asli Oploverz …" | "Partner Anoboy & Samehadaku", portal prose |

---

## 2. Platform & infrastructure

| Property | Value | Evidence |
|---|---|---|
| Frontend | **SvelteKit** (SSR, hydrated) | `x-sveltekit-page: true`; `_app/immutable/entry/start.*.js`; `data-sveltekit-preload-data="hover"` |
| UI | Tailwind + shadcn-style components (`bits-*` ids) + lucide icons + embla carousel | classes `aspect-h-6 aspect-w-4`, `data-embla-slide`, `lucide-circle-play` |
| Backend | **Laravel JSON API** at `backapi.oploverz.ac` | `meta/currentPage/lastPage/perPage/total` envelope; `access-control-allow-origin: *`; Laravel-style 404 pages |
| CDN/WAF | **Cloudflare** | `server: cloudflare`, `cf-ray`, `cf-cache-status: DYNAMIC` on all hosts |
| Bot challenge | **none observed** | plain curl + Chrome UA → 200 on home, list, detail, episode, API |
| HTML weight | home 261 KB, list 1.47 MB, detail **5.3 MB**, episode **4.9 MB** | see fixtures |
| Ads | heavy: popunder `<script src="//lp.duskishwinrace.com/...">`, `100proa.com`, `zen188...`, inline ad-domain lists in the payload | episode/home HTML |

Detail and episode pages are huge because the SvelteKit payload embeds the **entire series + every episode object** (download links, stream links, descriptions) inline as devalue-style JS, not because of images (zero base64 images).

---

## 3. Capability matrix

| Capability | Works | How |
|---|---|---|
| Latest / ongoing list | ✅ | `/` hero + carousel sections |
| Full catalog | ✅ | `/series` (991 titles, grouped A–Z) |
| Search | ✅ | API `GET /api/series?q=<term>` (13 hits for `naruto`) |
| Series detail | ✅ | `/series/{slug}` SSR + `/api/series/{slug}` |
| Episode list | ✅ | all episodes server-rendered on detail page (1,180 anchors for One Piece) + `/api/series/{slug}/episodes` |
| Episode metadata | ✅ | inline payload / episodes API |
| Stream playback | ⚠️ **not direct** | `streamUrl` points at 3rd-party embeds (`upbolt.to`, `filedon.co`, `4meplayer`); no mp4/m3u8 in Oploverz HTML |
| Direct MP4 | ⚠️ conditional | `filedon.co/embed/{id}` embeds a **presigned R2 MP4** with `X-Amz-Expires=3600` |
| Download links | ✅ (links only) | `downloadUrl[].resolutions[].download_links[]` → acefile, vikingfile, filedon, akirabox, pixeldra, gofile, mediafire, mirrored.to |
| Batch download | ✅ | `batchDownloadUrl` on series |
| Schedule | ✅ | `/schedule` (89 KB) |
| Genre browse | ❌ on web | `/genre/action` → 404; genres only as metadata/API fields |
| Bookmarks | client-only | `/bookmarks` route |

---

## 4. URL patterns (real examples)

```
GET /                                              home
GET /series                                        A–Z catalog, 991 series
GET /series/{slug}                                 detail
GET /series/{slug}/episode/{n}                     episode page
GET /movie/{slug}                                  movie detail (200, 12 KB)
GET /schedule                                      release schedule
GET /bookmarks                                     client-side only

GET https://backapi.oploverz.ac/api/series?page=N[&q=term]
GET https://backapi.oploverz.ac/api/series/{slug}
GET https://backapi.oploverz.ac/api/series/{slug}/episodes?page=N
```

Verified examples:

- `https://oploverz.diy/series/one-piece`
- `https://oploverz.diy/series/one-piece/episode/1180`
- `https://oploverz.diy/series/black-clover/episode/177`
- `https://oploverz.diy/series/kusuriya-no-hitorigoto-s3/episode/7`
- `https://oploverz.diy/movie/digimon-adventure-last-evolution-kizuna`
- `https://backapi.oploverz.ac/api/series?q=naruto` → 14,229 B, `total: 13`
- `https://backapi.oploverz.ac/api/series/one-piece/episodes?page=2` → episode `1170`

Routes that **do not exist**: `/search?q=`, `/search?query=`, `/cari?q=`, `/?s=` (returns home), `/genre/action`, `/anime` — all 404 except `/?s=` which silently returns the home page (server ignores it).

---

## 5. DOM map (exact selectors, validated with lxml XPath)

### 5.1 Cards (home carousels, recommendation rows)

```css
a[href^="/series/"]                              /* card container */
a[href^="/series/"] img[alt]                     /* poster + TITLE in @alt */
a[href^="/series/"] img[src*="/uploads/posters/"]/* poster URL (backapi) */
a[href^="/series/"] span.rounded-full            /* badge, e.g. "18/20" */
h1 span.line-clamp-2                             /* hero title only */
```

Observed on `home.html`:

- **75** `a[href^="/series/"]` anchors but only **34 unique** hrefs — the carousel duplicates each card (one `hidden lg:block`, one mobile variant). **Always dedupe by href.**
- Poster: `<img alt="Dragon Ball Super: Beerus" src="https://backapi.oploverz.ac/uploads/posters/fb4d7e0b-....jpg" class="absolute inset-0 h-full w-full object-cover">`
- Card title lives in `img@alt` (cards often have no text node title). Hero titles are `<h1>` (`Black Clover`, `One Piece`, `Bleach`).
- Badge string seen: `18/20` — semantics **unverified** (episode progress vs. date), do not parse as episode count.
- No `<h2>`/`<h3>` section headings; section labels are `<p class="text-lg font-bold ...">` text ("Tonton Anime Terbaru", "Rekomendasi", etc.).

### 5.2 List page `/series`

```css
main a[href^="/series/"]          /* 991 titles, grouped under #, A..Z */
```
Group headings are plain text nodes/`<p>`; safest is to scrape all anchors and dedupe by href. Slug is the canonical ID.

### 5.3 Detail page `/series/{slug}`

```css
h1                               /* series title (present in payload, not always rendered) */
ul.list-disc > li                /* metadata, "Label: value" strings */
a[href^="/series/{slug}/episode/"] p          /* [0]= "Episode N", [1]= release date */
```

Verified on One Piece:

```html
<ul class="grid list-inside list-disc ...">
  <li>Tipe: Serial TV</li>
  <li>Studio: Toei Animation</li>
  <li>Tanggal Rilis: 26 July 2026</li>
  <li>Status: Berlangsung</li>
  <li>Genre: Action, Drama, Fantasy, Shounen, Super Power, Comedy, Adventure</li>
  <li>Skor: 8.48 / 10.00</li>
  <li>Durasi: 24 min</li>
</ul>
```

Episode item:

```html
<a href="/series/one-piece/episode/1180">
  <p>Episode 1180</p>
  <p>28 September 2026</p>
</a>
```

1,181 episode anchors on the page → the entire run is in the initial HTML. Dates are Indonesian (`Januari … Desember`).

### 5.4 Episode page `/series/{slug}/episode/{n}`

```css
button                            /* stream selectors: "Nonton Online 480p", "... 720p", "... 1080p" */
iframe, video                     /* ZERO in SSR HTML — player is injected by JS */
```

Verified: `iframe = 0`, `video = 0`, `h1 = 0` on `episode.html`. Server buttons:

```html
<button ...>Nonton Online 480p</button>
<button ...>Nonton Online 720p</button>
<button ...>Nonton Online 1080p</button>
```

The **machine-readable truth is the inline payload**, not the DOM:

```js
episode:{
  id:26252, subbed:"Sub", title:"Episode 1 - Awal Arc Romance Dawn", episodeNumber:"1",
  downloadUrl:[{format:"One Piece Episode 001 Subtitle Indonesia",
    resolutions:[{quality:"HD", download_links:[{host:"FILEDON",url:"https://filedon.co/view/cvoHlGlA0E"}]}]}],
  streamUrl:[{source:"sd",url:"https://www.blogger.com/video.g?token=AD6v5..."}],
  releasedAt:"1970-01-01T00:00:00.000Z", createdAt:"...", updatedAt:"...",
  series:{ id, seriesId, title, japaneseTitle, slug, description, ..., genres:[] }
}
```

Recent episode (1180) instead has:

```js
streamUrl:[
 {source:"Nonton Online 480p", url:"https://upbolt.to/e/vytmfjb1rfti"},
 {source:"Nonton Online 720p", url:"https://upbolt.to/e/3y9ii66027ww"},
 {source:"Nonton Online 1080p",url:"https://upbolt.to/e/org2vvml37ao"}
]
```

The payload uses **unquoted keys** (Svelte devalue), so a strict JSON parser fails; the API JSON is the cleaner path.

---

## 6. AJAX / JSON API (preferred path)

Base: `https://backapi.oploverz.ac` — Cloudflare, `content-type: application/json; charset=utf-8`, `access-control-allow-origin: *`. **No auth, no cookies, no CSRF.**

### `GET /api/series?page={n}&q={term}`

```json
{"meta":{"currentPage":1,"firstPage":1,"lastPage":100,"perPage":10,"total":991},
 "data":[{"id":2104,"seriesId":55559,"title":"Cyberpunk: Edgerunners S2",
   "japaneseTitle":"...","slug":"cyberpunk-edgerunners-s2","description":"...",
   "releaseDate":"2026-10-20T00:00:00.000Z","status":"Completed",
   "poster":"https://backapi.oploverz.ac/uploads/posters/....jpg","duration":"23 min.",
   "releaseType":"TV","score":8.62,
   "genres":[{"id":90,"name":"Action","slug":"action"}],
   "censored":false,"mature":false,"hot":false,
   "batchDownloadUrl":[{"format":"BATCH HD","resolutions":[...]}],
   "createdAt":"...","updatedAt":"...",
   "studio":{"id":208,"name":"Trigger","slug":"trigger"},
   "season":{"id":331,"name":"Fall 2026","slug":"fall-2026"},
   "totalEpisodes":10}]}
```

- Pagination: **`page` works** (`page=2` → `currentPage: 2`, different data).
- **`perPage` is ignored** — `perPage=100` still returns `perPage: 10`. Plan for 100 requests per full crawl (991 series), or use `/series` HTML (1 request, 1.47 MB).
- **Search param is `q`**; `search=`, `title=`, `keyword=`, `filter=` are all ignored (they return the unfiltered 991 total).
- `status=ongoing` is also ignored (same 991) — filter client-side on `status` / `hot`.

### `GET /api/series/{slug}` → single object (1,550 B for `one-piece`).

### `GET /api/series/{slug}/episodes?page={n}`

```json
{"meta":{"currentPage":1,...,"perPage":10,"total":1180},
 "data":[{"id":45792,"subbed":"Sub","title":null,"episodeNumber":"1180",
   "downloadUrl":[{"format":"mp4","resolutions":[{"quality":"480p",
     "download_links":[{"host":"GD","url":"https://acefile.co/f/..."},
                       {"host":"VIKING","url":"https://vikingfile.com/f/..."},
                       {"host":"FILEDON","url":"https://filedon.co/view/..."}]}]}],
   "streamUrl":[{"source":"Nonton Online 480p","url":"https://upbolt.to/e/..."}],
   "content":null,"releasedAt":"2026-09-27T17:00:00.000Z",
   "createdAt":"...","updatedAt":"...","series":{...}}]}
```

Episodes are returned **newest-first** (1180 at page 1, 1170 at page 2). `perPage` is fixed at 10 here too.

---

## 7. Stream extraction

| Layer | Observation |
|---|---|
| Oploverz HTML | **No** `.m3u8`, **no** `.mp4`, **no** `<iframe>`/`<video>` in SSR. Only `streamUrl` JSON entries. |
| Player entries | `upbolt.to/e/{id}`, `filedon.co/embed/{id}`, `oploverz.4meplayer.com/#{code}`, `oplo2.4meplayer.pro/#{code}`, legacy `www.blogger.com/video.g?token=...` |
| `4meplayer` | `https://oploverz.4meplayer.com/` → connection failure (000) during recon; `oplo2.4meplayer.pro` referenced but rotating. Treat as unreliable. |
| `upbolt.to` | `/e/{id}` → 200, 5.7 KB JS shell; no direct media URL in HTML; file-code form posts to its own API. |
| `filedon.co` | `/embed/{id}` → 200, 123 KB Laravel page. Contains a **presigned R2 MP4**: `https://<hash>.r2.cloudflarestorage.com/filedon/users/5/One%20Piece/.../OPLOVERZ%20PLUS%20-%20OP%201149%20%5Bmp4%20480p%5D.mp4?...&X-Amz-Expires=3600&X-Amz-Signature=...`. Also exposes `hls_url`/`media.hls` routes (null for the sampled file). |
| Blogger legacy | `www.blogger.com/video.g?token=...` — playable only through Google's iframe player, token-bound. |

**Verdict**: Oploverz is **not directly playable** from its own HTML. A Rust client can either (a) surface the third-party embed URL to a browser/webview, or (b) fetch `filedon.co/embed/{id}` and extract the signed MP4 — valid **1 hour**, tied to the signature, and may still require `Referer: https://filedon.co/` (not tested; no header check performed). No `Referer`/`Authorization` was needed to fetch any Oploverz HTML or the API.

---

## 8. Anti-bot, robots, legal

`robots.txt` (verbatim, 70 bytes):

```
User-agent: *
Allow: /

Sitemap: https://anime.oploverz.ac/sitemap.xml
```

- `https://anime.oploverz.ac/sitemap.xml` did **not** return a body during recon (empty download) — unverified.
- No Cloudflare challenge, Turnstile, JS cookie, or rate-limit header observed on ~25 requests over 10 minutes.
- No `Referer`/`Origin` enforcement on API or HTML.
- The site is an unlicensed re-distributor of anime with heavy ad/popunder monetization; scraped content and third-party file hosts carry copyright risk. Keep the client read-only, no bulk downloading, identify with a normal UA, and expect domain churn.

---

## 9. Risks / fragility

1. **Domain rotation is the top risk.** `.plus`/`.in` already dead-end at a 522 link hub; at least 8 lookalike hosts exist, several on `ww*` parking wildcards.
2. **5 MB detail/episode pages** — the inline devalue payload duplicates the whole series. A naive full-body parse per request is slow and memory-heavy; the JSON API is 1–2 KB per call.
3. **`perPage` is server-fixed at 10** — crawling 991 series or 1,180 episodes requires 100/118 requests. Respect a delay.
4. **Playback is third-party and ephemeral.** `upbolt`/`4meplayer` rotate; `filedon` signed URLs expire in 1 h; Blogger tokens are opaque.
5. **Selectors**: no semantic `data-*` hooks on cards; titles live in `img@alt`; duplicate carousel cards; no `<h2>` sections. Tailwind classes are long and can be reordered by a rebuild — prefer structural selectors (`a[href^="/series/"]`, `img@alt`, href regex) over class names.
6. **Indonesian dates** ("28 September 2026") and label-value `<li>` strings need locale-aware parsing.
7. **Payload format is devalue, not JSON** — keys unquoted; regex/XPath extraction is brittle. Use the REST API whenever possible.
8. **Ads/popunders** in HTML can trip naive link extractors (e.g. `t.me`, `sflink.ink`, `100proa.com`, `tajir.la`).

---

## 10. Fixtures (`research/raw/oploverz/`)

| File | Size | Proves |
|---|---|---|
| `home.html` | 261 KB | hero + 34 unique cards, `img@alt` titles, backapi posters, ad scripts |
| `list.html` | 1.47 MB | 991-series A–Z catalog |
| `search.html` | 11 KB | `/search?q=` → **404** "404 Not Found" (no web search route) |
| `detail.html` | 5.1 MB | One Piece detail: metadata `<li>`s + 1,181 episode anchors + full devalue payload |
| `episode.html` | 4.8 MB | One Piece ep 1180: "Nonton Online" buttons, `streamUrl` upbolt, zero iframe/video |
| `embed.html` | 121 KB | `filedon.co/embed/9I2kAq614W` → presigned R2 MP4, `X-Amz-Expires=3600` |
| `embed_upbolt.html` | 5.6 KB | `upbolt.to/e/...` JS shell, no media URL |
| `clone_wordpress_home.html` | 124 KB | `oploverzvip.com` WP 7.1.3 + `dramastream` theme clone |
| `api_series_q_naruto.json` | 14 KB | working `?q=` search (total 13) |
| `api_series_search.json` | 11 KB | `?search=` ignored (total 991) |
| `api_eps.json` | 59 KB | episodes API, newest-first, `streamUrl` + `downloadUrl` |
| `schedule.html` | 88 KB | `/schedule` |
| `robots.txt`, `*.hdr` | small | Cloudflare/SvelteKit headers, JSON API headers |

---

## 11. Unknowns

- Whether `anime.oploverz.ac/sitemap.xml` is live (empty response).
- Genre/type filter endpoints (web `/genre/*` is 404; API filter params ignored).
- Real web search UI route (site likely filters `/series` client-side against the API `q`).
- `4meplayer` DNS/availability and whether it ever exposes HLS.
- Whether the `filedon` signed MP4 requires a `Referer`/`Origin` at download time.
- Semantics of the card badge (`18/20`).
- Rate limits under sustained crawl (none hit at ~25 requests).
- Whether `oploverz.diy` or `oploverz.site` is the long-term canonical; both currently serve identical HTML.
