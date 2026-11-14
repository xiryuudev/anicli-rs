# KotakAnime — Recon Report (read-only, 2026-11-15)

Status: **PLAYABLE** via publicly accessible HLS. No login, no Cloudflare JS challenge, no Referer/cookie requirement on the media CDN.
Confidence: high — every claim below is backed by a real fetch saved in `research/raw/kotakanime/`.

---

## 1. Domain / how it was found

HINT was correct: `kotakanime.id` and `kotakanime.com` are dead.

| Domain | DNS | HTTP | Verdict |
|---|---|---|---|
| `kotakanime.id` | NXDOMAIN (`getent hosts` empty, curl `(6) Could not resolve host`) | — | **DEAD** |
| `kotakanime.com` | resolves (AWS Global Accelerator `2600:1f18:4ae:...`) | `curl: (28) Connection timed out after 25003 ms` | **DEAD** |
| `kotakanimeid.com` | resolves (`76.223.54.146`, `13.248.169.48`) | `curl: (28) Connection timed out after 25003 ms` | **DEAD** |
| `kotakanimeid.link` | Cloudflare (`2606:4700:...`) | **HTTP/2 200** | **ACTIVE (portal)** |
| `s13.nontonanimeid.boats` | Cloudflare | **HTTP/2 200** | **ACTIVE (real catalog)** |

Discovery path that actually worked:
1. `mcp__donsetch__web_search` query `"kotakanime anime sub indo"` → rank #1 `https://www.kotakanimeid.com/` (dead), rank #3 Telegram.
2. `web_fetch https://t.me/s/kotakanime` → channel **"NontonAnimeID Update"** literally says *"Bookmark domain utama ini: kotakanimeid.link"*.
3. `curl https://kotakanimeid.link/` → 200, but it is **only a domain-tracker landing page** (8.6 KB), not the catalog.
4. The landing page's single outbound CTA is `<a href="https://s13.nontonanimeid.boats" ... class="action-btn main-site-link">` → that is the real site.

**Naming reality:** "KotakAnime" = the brand/domain-tracker; the streaming catalog runs as **NontonAnimeID** on rotating numbered subdomains (`s13.` today). Any client must resolve the current `s*` host, not hardcode it.

## 2. Platform

- WordPress + theme **`kotakanime`** (`/wp-content/themes/kotakanime/`), second variant `kotakanime2`.
- **LiteSpeed Cache** (`x-litespeed-cache: hit`) in front of WordPress.
- Fronted by **Cloudflare** (`server: cloudflare`, `cf-ray: ...-SIN`, HTTP/2 + h3, `zstd` encoding).
- Simple `/` and article pages served as plain HTML to a bare Chrome-UA curl — **no `cf_clearance`, no JS challenge for the HTML tier.**
- A Cloudflare JS-detection iframe script (`/cdn-cgi/challenge-platform/scripts/jsd/main.js`) is injected at the bottom of pages; it did not gate content.
- Images hotlinked through Jetpack/Photon `https://i0.wp.com/<host>/wp-content/uploads/...`.

## 3. Capability matrix

| Capability | Endpoint / method | Status | Evidence |
|---|---|---|---|
| Recent/latest list | `GET /` → `div.misha_posts_wrap article.animeseries` | ✅ works | `nontonanimeid_home.html` |
| Full catalog browse | `GET /anime/` → `a.as-anime-card` | ✅ works | `anime_list.html` |
| Pagination | `GET /anime/page/N/` (N≤195, 20/page ≈ 3.9k series) | ✅ works | `anime_list_page2.html`, `wp-pagenavi` "Page 1 of 195" |
| Search | `GET /?s=<query>` (same `a.as-anime-card`) | ✅ works | `search_onepiece.html` |
| Detail | `GET /anime/<slug>/` | ✅ works, episodes inline | `detail_kashita.html` |
| Episode page | `GET /<slug>-episode-<N>/` | ✅ works | `ep1_kashita.html` |
| Genre | `GET /genres/<genre>/` | ✅ linked in DOM (not separately fetched) | `detail_pretty` genre tags |
| Ongoing | `GET /ongoing-list/` (`/ongoing/` → 301 to it) | ✅ redirect observed | curl `-D -` |
| Server list → embed | `POST /wp-admin/admin-ajax.php action=player_ajax` | ✅ works | `ajax_nume1..3.html` |
| Home "load more" | `POST admin-ajax.php action=loadmore` (urlencoded) | ✅ contract found (not replayed) | `ajaxloadmore_homepage.js` |
| Direct video | `GET cdn2.kotakanimeid.link/.../index.m3u8` → TS | ✅ 200 / 606 KB segment | `m3u8_noref.txt`, `v480.m3u8`, `seg0.ts` |

## 4. URL patterns

```
BASE      = https://s13.nontonanimeid.boats     # s<N> rotates; discover from portal
home      = BASE/
catalog   = BASE/anime/                         # list, 20/page
page      = BASE/anime/page/{n}/                # n = 1..195
detail    = BASE/anime/{slug}/
episode   = BASE/{slug}-episode-{ep}/           # NOTE: no /anime/ prefix, no trailing id
genre     = BASE/genres/{genre}/
ongoing   = BASE/ongoing-list/                  # /ongoing/ 301s here
search    = BASE/?s={query}
ajax      = BASE/wp-admin/admin-ajax.php
embed     = https://s{1,2}.kotakanimeid.link/video-embed/?vid={v3_...}
stream    = https://cdn2.kotakanimeid.link/stream/{token}/{expiry}/v/{hash}/index.m3u8
  variant = .../480p/index.m3u8 | 720p | 1080p
  segment = .../480p/segment_00000.ts
```

`{slug}` is WordPress-sanitized, may contain percent-encoding (`ranma-%c2%bd-2024-3rd-season`). Episode slug == detail slug + `-episode-N`.

## 5. DOM map (exact selectors, verified)

### List — home (component A)
```
div.misha_posts_wrap > article.animeseries[class*=post-<ID>]
  > div.sera > a[href]                       # href = detail URL
    > div.limit
      > span.types.episodes   -> "7"         # episode count (may be "??" / absent)
      > span.types.status     -> "UC"/"Ongoing"
      > img[src]             # cover (i0.wp.com)
      > h3.title > span[data-title-default][data-title-en][data-title-jp]  # title
```
Post ID is encoded in the article class: `post-165706`.

### List — `/anime/` and `?s=` (component B — DIFFERENT from home)
```
a.as-anime-card[href][data-series-id]
  > div.as-card-thumbnail > img[src]
  > div.as-card-content
    > h3.as-anime-title[data-title-default|data-title-en|data-title-jp]
    > div.as-quick-info > span.as-rating > span.value, span.as-type, span.as-season
    > p.as-synopsis
    > div.as-genres > span.as-genre-tag
pagination: div#archive-pagination-container nav.pagination .wp-pagenavi
  span.pages "Page X of Y"; a.nextpostslink[rel=next]; a.last
```
⚠️ Two list components exist; a single-card parser will fail on one of the two pages.

### Detail (`/anime/{slug}/`)
```
h1.entry-title.cs                                   # "Nonton <TITLE> Sub Indo"
  span[data-title-default|data-title-en|data-title-jp]
div.anime-card__score  > span.value (rating), span.type (TV/ONA/...)
div.anime-card__genres.in-tab > a.genre-tag[href]
div#tab-synopsis div.synopsis-prose > p
div.anime-card__quick-info.v4 > span.info-item[.status-airing]  # status, "?? Episodes", duration, season
section.anime-card__episode-list-section
  div.episode-list-header > input.episode-search-input
  div.episode-list-items > a.episode-item[data-episode-url]
      > span.ep-title ("Episode 7")  + span.ep-date ("Nov 15, 2026")
div.anime-card__meta.v7 div.meta-episodes > a.ep-link  # first/last shortcuts
```
Episodes are fully server-rendered — **no AJAX needed to get the episode list.** `a.episode-item` order is newest → oldest.

### Episode (`/{slug}-episode-{N}/`)
```
div#video-content div.boxtab.serverinfo
ul.tabs1.player > li.kotak_player_option
    [data-post=<postId>][data-type=<serverName>][data-nume=<1..7>]
    > span ("S-Lokal-c", "S-Kotakvideo", "S-Streamku", ...)
div#pembed.player-embed
  > div.playerload
  > div#videoku.player_embed        # AJAX HTML fragment is injected HERE
div.banner_player > div.close_banner   # in-player ad overlay to strip
```
Servers seen for ep1: `Lokal-c`(1), `Kotakvideo`(2), `Streamku`(3), `Nontonku`(4), `Nonton`(5), `Ok-uhd`(6), `Lokal-1080`(7).

## 6. AJAX contract

Config is **base64-embedded** as a `data:text/javascript;base64,...` script tag (not a global `.js` file):
```js
var kotakajax = {"url":"https://s13.nontonanimeid.boats/wp-admin/admin-ajax.php",
 "loading":"Loading..","autoload":"1","nonce":"5f5a217642",
 "serverNames":["lokal-c","kotakvideo","streamku","nontonku","nonton","ok-uhd","lokal-1080"],
 "is_preview":"0"}
```
Nonce is per-page-render (base64 `var kotakajax=...` blob) → must be re-extracted per episode fetch.

### 6a. Server switch / embed retrieval
```http
POST {BASE}/wp-admin/admin-ajax.php        (multipart/form-data, credentials: same-origin)
action=player_ajax
post=<data-post>          # e.g. 164678
nume=<data-nume>          # 1..7
serverName=<data-type>    # e.g. "Lokal-c"  (verbatim attribute, case-sensitive)
nonce=<kotakajax.nonce>
did=<cookie _kotak_did>   # optional, may be absent
[is_preview=1]            # only when kotakajax.is_preview === "1"
```
Response: `200 text/html` = a fragment containing the `<iframe>`:
```html
<iframe title="Server Nonton" class="lazy" src="https://s1.kotakanimeid.link/video-embed/?vid=v3_...">
```
No nonce, no cookie and no Referer was required for 3 sequential calls (nume 1,2,3) — all returned 200.

### 6b. Home load-more
```http
POST {BASE}/wp-admin/admin-ajax.php   (application/x-www-form-urlencoded)
action=loadmore & nonce=<misha_loadmore_params.nonce>
displayed_posts[]=<id>&... & offset=<count>
```
Returns raw `<article>` HTML appended to `.misha_posts_wrap`; body `"0"`/empty ⇒ end.

## 7. Stream extraction

Chain: `player_ajax` → **iframe** `https://s{1,2}.kotakanimeid.link/video-embed/?vid=v3_<long>` → page holds an **obfuscated JS blob** → JW Player 8.51.7 + hls.js 1.5.8 (+ p2p-media-loader) → **direct HLS**.

The `vid` is opaque and the iframe page reveals nothing in plain HTML. Source is hidden by base64+XOR+`eval`:
```js
var _k26b594=function(){var s='175,48,33,133,82,137,61,44,70,85,217,116,217,122,242,53'.split(',');
  var r=[];for(var j=0;j<s.length;j++){r.push((parseInt(s[j],10)-65*(j+3))&255);}return r;};
... _w['at'+'ob']('<base64, sometimes split into 3 concatenated literals>' ... )
... _oae2820[i] = _d051a2a.charCodeAt(i) ^ _bb05672[i % 16]
... (0,eval)(TextDecoder.decode(...))
```
Decoding (see `embed_decoded.js`) yields:
```js
window.videoId  = '729591279f0705dee203c843bc039d0d';
window.isHLS    = true;
var __activeSources = [{label:"AUTO", type:"application/vnd.apple.mpegurl",
  file:"https://cdn2.kotakanimeid.link/stream/{token}/{expiry}/v/{hash}/index.m3u8"}];
```
P2P is also wired: `initKotakP2P("novage","", "wss://tracker.kotakanimeid.link", {...})` — only a tracker websocket, not the media transport.

**Verified playable, anonymously:**
```
GET https://cdn2.kotakanimeid.link/stream/plN2zcuQqOafMiOdN6Gqvg/1795651200/v/180a...455d/index.m3u8
→ 200, master playlist: 480p (496k) / 720p (1078k) / 1080p (1960k), VOD
GET .../480p/index.m3u8  → 200, #EXT-X-PLAYLIST-TYPE:VOD, segment_00000.ts
GET .../480p/segment_00000.ts → 200, 606488 bytes, "MPEG transport stream data"
```
Headers needed on the CDN: **none.** Identical `200`/`221 bytes` with and without `Referer: https://s1.kotakanimeid.link/`; no cookies sent. `{expiry}` is a Unix epoch inside the path (`1795651200` = 2026-11-26 00:00 UTC), so embed URLs are time-limited (~10 days at capture) and must be re-derived per playback.

Server 3 (`Streamku`) is a plain external iframe `https://s1.rpmvip.com/#b1l8yp` — an ad page, not a usable stream.

## 8. Anti-bot / defenses

| Layer | Detail | Impact |
|---|---|---|
| Cloudflare | CDN + JSD challenge script on HTML; **not enforced** for curl HTML/media | low |
| Server-side embed ticket | `vid` opaque token, fetched via `player_ajax`; player source is obfuscated in-page | medium — cannot skip `player_ajax` |
| Turnstile / challenge gate | JS handles `429` + `X-Kotak-Challenge: required` → redirect to `?kotak_player_challenge=1` | **not triggered** in our 3 calls |
| Rate limit | `429` + `Retry-After` → client-side cooldown ("Terlalu cepat berganti server") | throttle server switches; parse `Retry-After` |
| Device token | `X-Kotak-Device-Token` response header + `_kotak_did` cookie, echoed back as `did` | absent on first call; optional |
| Ad overlays | `div.banner_player`, `div.notice-iklan`, popunders (`orangarab.fun`, `dw.zeus.fun`, `menujupenta.site`) | strip `#pembed` overlays; server 3 is ad-only |
| Explicit anti-detection note in code | Comment states sources are *never* replaced by dummy on `navigator.webdriver`, and gate is server-side | no client-side fingerprint check to defeat |
| URL expiry | `{expiry}` epoch in HLS path | cache ≤1h, re-derive on 403 |

## 9. Risks

1. **Domain rotation is the top risk.** The `s<N>.nontonanimeid.boats` host is expected to change; only the portal `kotakanimeid.link` is stable. Hardcoding `s13` will break.
2. Two different list card components (home vs `/anime/`) — divergent parsers.
3. `post`/`nume`/`serverName`/`nonce` are all per-episode-page and nonce is per-render; caching them is unsafe.
4. Obfuscation key is hardcoded per embed page (`_k26b594` literal) — it can change; the decoder must re-read the key from the page, not hardcode it.
5. HLS path token expiry + possible hosted-token rotation.
6. Aggressive ads/popunders; server 3 is unusable.
7. Content is unlicensed Indonesian-subtitle re-hosting (legal/ToS risk for any consumers).
8. Cloudflare may escalate to a real challenge under load/abuse; no Turnstile bypass was exercised or needed.

## 10. Fixtures — `research/raw/kotakanime/`

| File | What it proves |
|---|---|
| `home.html` | portal `kotakanimeid.link` = tracker page, CTA to `s13.nontonanimeid.boats` |
| `nontonanimeid_home.html` | real home, `misha_posts_wrap`/`article.animeseries` cards |
| `anime_list.html`, `anime_list_page2.html` | `/anime/` `a.as-anime-card` + `wp-pagenavi` (195 pages) |
| `search_onepiece.html` | `?s=` results use `a.as-anime-card` |
| `detail_kashita.html` | detail metadata + `a.episode-item` list |
| `ep1_kashita.html` | `li.kotak_player_option` attrs + `#videoku` target, base64 `kotakajax` config |
| `front.ajax.js` | full `player_ajax` contract, 429/challenge/cooldown handling |
| `ajax_nume1.html`, `ajax_nume2.html`, `ajax_nume3.html` | actual AJAX responses: embed iframes (`s1/s2`), plus `s1.rpmvip.com` for server 3 |
| `embed1.html` | embed page with obfuscated blob + jwplayer/hls.js/P2P wiring |
| `embed_decoded.js` | deobfuscated source → `cdn2.kotakanimeid.link/.../index.m3u8` |
| `m3u8_noref.txt`, `m3u8_ref.txt` | master playlist identical with/without Referer |
| `v480.m3u8`, `seg0.ts` | VOD variant playlist + real 606 KB MPEG-TS segment |
| `ajaxloadmore_homepage.js` | `action=loadmore` contract |
| `footer-init.js`, `more_fitur.js`, `history.js`, `marathon.js`, `preferences.js` | theme JS (no player action; confirms `front.ajax.js` is the one) |

## 11. Unknowns / not verified

- Whether the `s<N>` subdomain rotation is scheduled, and how a client should discover it (portal scrape vs Telegram).
- `/genres/<genre>/` and `/ongoing-list/` page DOM — only redirect/link observed, bodies not parsed.
- Behavior when `429 + X-Kotak-Challenge: required` actually fires (no Turnstile flow exercised).
- `did`/`X-Kotak-Device-Token` semantics: not required for 3 calls, but unclear when it becomes mandatory.
- Whether `Lokal-1080` / `Ok-uhd` (nume 6,7) return different CDN hosts or higher-quality tokens.
- `loadmore` action not replayed (contract only, from JS).
- Long-run rate limits and Cloudflare escalation threshold.
