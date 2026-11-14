# AWSubs — Recon Report (read-only, no code changes)

**Recon date:** 2026-11-15 ~04:57–05:05 WIB (UTC+7).
**Live site verified:** `https://www.awsub.web.id/` — **Blogger/Blogspot**, no Cloudflare.
**One-line verdict:** AWSubs is a **batch-download fansub archive**, *not* a streaming site. Per-episode *playback* exists only on its sister site `puristream.blogspot.com`, and each streamable post is flagged with a literal `[S]` suffix in the title.

Every claim is backed by a saved fixture in `research/raw/awsubs/`, an HTTP status, or literal markup quoted below.

---

## 1. Domain / discovery — the hint's `200` is a trap

`awsubs.id` does answer **200**, but it is a **blank nginx default page** (615 B, `<title>Welcome to nginx!</title>`). Status code alone is worthless here.

| Host | Result | Verdict |
|---|---|---|
| **`www.awsub.web.id`** | **200**, 58,006 B gzip / 215,607 B decoded | **REAL** — `<title>AWSubs - Anime x265/HEVC Subtitle Indonesia</title>` |
| `awsub.web.id` | 301 → `https://www.awsub.web.id/` | apex → www |
| `awsubs.id` | 200, 615 B | **nginx placeholder**, zero anime markup |
| `awsubs.web.id` | curl exit 6, NXDOMAIN | dead (confirms hint) |
| `awsubs.tv` | curl exit 6, NXDOMAIN | dead |
| `awsubs.co` | 301 → `https://new.dicel.co/` | **repurposed** — Vietnamese WooCommerce site, unrelated |
| `awnime.com` | 200, 3,267 B | **"This domain is for sale"** parking, `robots: noindex`, even though the AWSubs footer still links it for About/Privacy |
| `awsekai.blogspot.com` | 200 but final URL is `www.awsub.web.id` | alias/redirect of the same blog |
| **`puristream.blogspot.com`** | 200, 65,869 B | **streaming sister site**, `<title>… \| AWStreaming` |
| **`purinime.web.id`** | 200, 41,639 B | **download link wrapper** (real GDrive/MediaFire live here) |

Accept-marker set (≥2 required): `<meta content='blogger' name='generator'/>`, `/feeds/posts/default`, `section class='stream'`, `div id='extra-info'`, `h2>Link Download:`, `class='change-video'` (puristream only).

## 2. Platform

- **Blogger / Blogspot** hosted (Google). Evidence: `<html … xmlns:b='http://www.google.com/2005/gml/b'>`, `<meta content='blogger' name='generator'/>`, `www.blogger.com/static/v1/widgets/*`, comment frames `https://www.blogger.com/comment/frame/8530925824447434087?...`, feed `openSearch$totalResults = 177`.
- Response headers: `server: GSE` (Google Frontend), `cache-control: private, max-age=0`, `etag: W/"…"`. **No `cf-ray`, no `cf-cache-status`.**
- Front-end: jQuery 3.7.0 + Flowbite 1.6.4/1.6.5 from cdnjs, AdSense `ca-pub-4661380463787037`.
- **No Cloudflare, no JS challenge, no captcha, no cookie wall.** Plain `curl` + Chrome UA gets full HTML.

## 3. Capability matrix

| Capability | Available | How |
|---|---|---|
| Search | ✅ | `GET /search?q=<term>` (Blogger). Also JSON: `/feeds/posts/default?q=<term>&alt=json` |
| Ongoing | ✅ | `/search/label/Ongoing` |
| Completed | ✅ | `/search/label/Completed` |
| A–Z index | ⚠️ | `/search/label/<A-Z>`; `/p/anime-list.html` is **JS-rendered** ("Loading…" description) |
| Detail | ✅ | `/YYYY/MM/<slug>.html` |
| Episodes (per-ep) | ❌ on awsub | Post groups **ranges** only: `Eps 01 - 06`, `Eps 07 - 12 End` |
| Streams | ⚠️ off-site | Only `[S]` posts → `puristream.blogspot.com`, **per-episode** `data-embed` spans |
| Batch download | ✅ | `Link Download:` → `purinime.web.id/dl-*.html` → GDrive / MediaFire / `sfl.gl` |
| Metadata | ✅ | Type, Status, Eps, Studios, Aired, Premiered, Subtitle, Rating, Genres, Score |

`[S]` is documented on-page: *"Anime yg bisa streaming ditandai huruf S dibelakang judul.."* (widget Text3).

## 4. URL patterns (verified)

```
home      https://www.awsub.web.id/
post      https://www.awsub.web.id/2026/10/dandadan-s2.html
post [S]  https://www.awsub.web.id/2026/11/toaru-no-anbu-sub-indo.html
search    https://www.awsub.web.id/search?q=Science%20SARU
label     https://www.awsub.web.id/search/label/Completed
feed      https://www.awsub.web.id/feeds/posts/default?alt=json&max-results=5
dl wrap   https://www.purinime.web.id/2026/10/dl-016.html
stream    https://puristream.blogspot.com/2026/11/anime-l01.html
```

## 5. DOM map — awsub.web.id post page (exact)

```html
<section class='stream'>                     <!-- article, NOT a player -->
  <div class="separator"><img src="…blogger.googleusercontent.com…/w600/….webp"/></div>
  <p id="synopsis"><b>Sinopsis:</b>…</p>
  <div id="extra-info">                      <!-- moved into #extra-target by JS -->
     <div>Studios: <span><a href="/search?q=…">…</a></span></div>
     <h2>Link Streaming:</h2>                <!-- ONLY on [S] posts -->
       Eps 01-04: <a href="https://puristream.blogspot.com/…">Streaming</a>
     <h2>Link Download:</h2>
     <h3 class="heading">&#8226; Episode 01 - 12 End</h3>
     480p x265<br><b>BATCH</b> | <a href="https://www.purinime.web.id/…">MediaFire</a> | …
  </div>
</section>
<section id='info' itemscope itemtype="http://schema.org/Movie">
  <div class='thumb'><div><img itemprop='image' …/></div>
    <div class='bookmark' data-id='5904660200712660163'>Bookmark</div></div>
  <div class='info'>
    <aside class='score'> <script type='application/ld+json'>{…"ratingValue":"8.3"…}</script>
      <p class='mb-05 …'>8.3 <span>2890</span> vote</p></aside>
    <h1 class='title' itemprop='name'>Dandadan S2 x265 Subtitle Indonesia &amp; English</h1>
    <p class='alias m-0'></p>
    <div class='shorting' id='syn-target'></div>
    <div class='meta' id='extra-target'>
      <div class='type'>Type: <span><a href='/search/label/TV'>TV</a></span></div>
      <div>Genre: <span><a href='/search/label/Action' rel='tag'>Action,</a> …</span></div>
      <div class='status'>Status: <span><a href='/search/label/Completed'>Completed</a></span></div>
      <div>Country: <span><a href='/search/label/Japan'>Japan</a></span></div>
      <div>Episode: <span><span class='eps'>Eps 12</span></span></div>
    </div>
  </div></section>
```

Template JS hard-codes the same contract: `$('.info .title')`, `$('#info .thumb div img')`, `$('.meta .status span')`, `$('.meta .type span a')`, `$('.bookmark').data('id')`.

Feed-level metadata is richer than the DOM and **much cheaper**: labels carry score, eps, status.
```json
"title":"Toaru Anbu no Item x265 Subtitle Indonesia [S]",
"category":[{"term":"7.5"},{"term":"Action"},{"term":"Eps 04"},
            {"term":"Ongoing"},{"term":"Japan"},{"term":"TV"},…]
```
⚠️ Do **not** parse "Eps 12" as the episode count of files — it is the series length; the actual download block groups ranges.

## 6. AJAX / JSON

Blogger's JSON-in-script feed is the only useful API and needs **no auth**:

```
/feeds/posts/default?alt=json&max-results=5           → 200, JSON (verified)
/feeds/posts/default?alt=json-in-script&max-results=150&callback=<cb>
/feeds/posts/summary?start-index=N&max-results=1&alt=json-in-script&callback=showLucky
/feeds/posts/default/-/<label>?alt=json               (label filter)
```
Template callbacks seen: `showLucky`, `readLucky`, `finddatepost`, `totalcountdata`.
Also a form `action="ajax/anime/report"` (report/abuse) and `#server`, `.DagPlayOpt`, `#pembed`, `.serverEpisode` CSS/JS — **template support for a player that AWSubs never uses**: no `class="DagPlayOpt"` element, no `<video>`, no player `<iframe>` exists in any awsub page (only the cbox chat + Blogger comment iframe).

## 7. Stream extraction (puristream, `[S]` posts)

Player DOM on `puristream.blogspot.com/YYYY/MM/anime-*.html`:

```html
<div id="kaurukavideo">
  <div class='vi-on' data-embed=''></div>
  <div class="servideo">
    <span class="change-video c-aktif" data-embed="https://www.blogger.com/video.g?token=AD6v5…&amp;origin=www.awsulih.my.id">Eps 01</span>
    <span class="change-video"         data-embed="…token=…">Eps 02</span>
  </div>
</div>
```
Selector: `#kaurukavideo .servideo span.change-video[data-embed]`, label = `Eps NN`, `.c-aktif` = default. One span per episode → **true per-episode streams**.

Resolution chain (verified end-to-end):
1. `GET https://www.blogger.com/video.g?token=…` → **200**, 9,261 B Google player shell (`BloggerVideoPlayerUi`), `content-security-policy` present.
2. The shell embeds signed **`https://rr1---sn-…googlevideo.com/videoplayback?…&source=blogger&itag=18&…`** URLs as `\u003d`-escaped / HTML-escaped JSON. Unescape `unicode_escape` **then** `html.unescape`, else the URL keeps trailing `&quot;` and fails.
3. Direct fetch: **`HTTP 200` only with `Referer: https://www.blogger.com/`** (or `Origin`); plain UA + Range → **403**. Verified 2,326,540 B received of `66,996,426 B` (itag=18, 360p MP4) before timeout.

URL is **IP-bound + expiring**: `ip=157.85.206.96` (== requester's public IP) and `expire=1794722381` (~7 h). So `video.g` must be resolved **by the same client/IP that will stream**, and the result must be cached per-session, not persisted.

`origin=www.awsulih.my.id` appears in the token — a **third** AWSubs-ish domain; not fetched (out of time).

## 8. Download topology

```
awsub post  →  https://www.purinime.web.id/2026/10/dl-016.html
                 ├─ h2 Link Download → div.dl → ul[h3 "Eps 01 - 06 | x265/HEVC"]
                 │    └─ li b "480p [391 MB±]" → a href drive.google.com/file/d/<ID>/view  (labelled MediaFire/Alternative — labels lie, host is GDrive)
                 ├─ ul[h3 "Eps 01 - 12 End"] → sfl.gl shortener  (https://sfl.gl/iNwyDhl)
                 └─ "Tersedia Saat Anime sudah End" when still ongoing
```
Container is `<div class="bixbox download" id="Download">`; link rows are `div.dl > ul > h3` (range) + `li > a`. Note the **decoy link labels**: anchors say *MediaFire* / *Alternative* but point at `drive.google.com`. Placeholder dead links `href="HHH"` exist on fresh posts (must be filtered).

## 9. Anti-bot

None on `awsub.web.id` / `purinime.web.id` / `puristream.blogspot.com`. The only friction is (a) Google's `video.g` requiring `Referer: https://www.blogger.com/`, (b) cbox/Disqus iframes that are irrelevant, (c) the anime-list page needing JS. No rate-limit signal observed at ~10 requests.

## 10. Risks

1. **Domain churn is the norm** — 4 of 6 candidates are dead/repurposed; `awnime.com` (still linked in the footer) is for sale. Hardcode `www.awsub.web.id` + probe.
2. **Blogger ToS** — download-link blogspot properties get deleted without notice; `awsekai` already collapsed into the main blog.
3. **Streams are off-site and fragile** — `puristream.blogspot.com` is a separate blog with its own lifecycle; a post's `[S]` flag can point at a missing page.
4. **IP-bound, 7-hour stream URLs** — no caching across IP change / restart; `Referer` mandatory.
5. **Batch-only granularity** on downloads — no per-episode downloads; ranges only.
6. Decoy anchors (`MediaFire` → GDrive), `href="HHH"` placeholders, and `Episode: Eps 12` ≠ files.
7. Mass of irrelevant iframes (cbox, comments, AdSense) — a naive "first iframe" player heuristic grabs the **chatbox**.

## 11. Fixtures (`research/raw/awsubs/`)

| File | Bytes | What |
|---|---|---|
| `home.html` / `home.headers` | 215,607 | real home, 200, GSE |
| `post_dandadan-s2.html` | 184,342 | download-only post (Completed, batch) |
| `post_toaru_s.html` | 184,488 | `[S]` ongoing post, has `Link Streaming:` |
| `post_occulticnine-bd-x265-subtitle-indonesia.html` | 184,566 | BD batch post |
| `dl_wrapper.html` / `.headers` | 146,474 | purinime `dl-016` with real GDrive + sfl.gl |
| `puristream_l01.html` / `.headers` | 65,869 | player page, 4× `span.change-video[data-embed]` |
| `blogger_video.html` / `.headers` | 24,970 | `video.g` shell, signed googlevideo URLs |
| `stream_urls.txt` | 2 urls, 1,414 B | unescaped direct MP4 (itag=18) |
| `anime_list.html` | 160,794 | JS-rendered A–Z page ("Loading…") |
| `feed_posts.json` | 36,516 | Blogger JSON feed, 5 entries, labels |
| `awsekai.html` | 215,607 | byte-identical size to home → same page/alias |
| `awnime.html` | 6,990 | "This domain is for sale" parking |

## 12. Unknowns / not verified

- `www.awsulih.my.id` — role unknown (appears as `origin=` in video tokens); not fetched.
- `pos`/`puristream` mapping for non-`[S]` series, and how `[S]` is assigned (manual in title, no label).
- Whether `/p/anime-list.html` can be reproduced from feeds alone (its filter JSON shape wasn't captured).
- Actual per-episode download availability for long-running shows (only range-batches observed).
- No test of Blogger's JSON feed paging past 150 entries or `?q=` search quality.
