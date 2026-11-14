# Anoboy — Reconnaissance Teknis (read-only)

- **Target**: Anoboy — portal anime subtitle Indonesia (WordPress, embed pihak ketiga)
- **Tanggal recon**: 2026-11-15, ~04:50–05:15 WIB (UTC+7)
- **Metode**: `curl -s --compressed` dengan Chrome desktop UA + HTTP/2, verifikasi tiap selector langsung di fixture (regex/DOM dump), plus DNS-over-HTTPS (`dns.google`) untuk host embed. `mcp__donsetch__web_search` untuk discovery.
- **Fixture**: `anicli-rs/research/raw/anoboy/` (11 file HTML, lihat §11)
- **Scope**: tidak ada perubahan kode Rust. Semua klaim di bawah punya status code / selector / contoh teks dari fetch nyata.

---

## 1. Domain aktif & cara menemukan

### Hasil uji domain (semua di-fetch 2026-11-15)

| URL | HTTP | Ukuran (ter-dekompres) | Isi | Status |
|---|---|---|---|---|
| `https://anoboy.id/` | **200** | 139.026 B | 49 `<article class="bs">`, katalog lengkap | **UTAMA — pakai ini** |
| `https://anoboy.nl/` | **200** | 18 `article.bs` + 22 link `/series/` | DOM & player identik (blogger), 1 episode diuji 200 | **Mirror aktif (fallback)** |
| `http://154.203.167.221/` | **200** | 155.928 B | Same WordPress, title `ANOBOY - 154.203.167.221 - ...` | Mirror IP (HTTP only, tanpa TLS) |
| `https://anoboy.wiki/` | 200 | 10.773 B | **Bukan katalog** — satu artikel SEO ("Pendahuluan: Gelombang Baru Hiburan Anime"), 0 `class="bsx"`, 0 link `/series/` | Jangan dipakai |
| `https://anoboy.web.id/` | **000** | 0 | DNS gagal (`Could not resolve host`) | Mati |
| `https://anoboy.baby/` | — | — | fixture lama: title blog "Anoboy Baby – Panduan Lengkap…" | Bukan katalog |

Semua domain hidup ada di belakang **Cloudflare** (`server: cloudflare`, `cf-ray: ...-SIN`, `cf-cache-status: DYNAMIC`).

### Cara menemukan domain berikutnya

1. `web_search "Anoboy anime sub indo domain"` → hit yang muncul: sur.ly/anoboy.web.id (mati), `anoboy.wiki` (artikel SEO), mirror IP `154.203.167.221`, dan Google Play "Anoboy: Anime Sub Indo" (aplikasi, bukan situs).
2. **Sinyal paling andal**: cek homepage kandidat — site asli selalu punya `<meta name="generator" content="WordPress 7.1.4" />` + `class="bsx"` ≥ 10 + link `/series/<slug>/`. Domain mati/parked tidak punya ketiganya.
3. `anoboy.wiki` sengaja dibuat menyerupai brand (title "anoBoy - Latest Nonton Anime Subtitle Indonesia") tetapi body-nya artikel; **judul saja tidak cukup**, harus cek jumlah `article.bs`.
4. Tidak ada blok "domain resmi" / redirect portal di situs (beda dengan Samehadaku). Tiap mirror (`anoboy.id`, `anoboy.nl`, IP) berdiri sendiri; tidak ditemukan mekanisme auto-announce.

---

## 2. Platform / infrastruktur

| Aspek | Nilai terverifikasi |
|---|---|
| CMS | **WordPress 7.1.4** (`<meta name="generator">`), Yoast SEO 28.4 |
| Theme | `dramastream` v2.2.2 (semua asset `/wp-content/themes/dramastream/assets/...`), dengan fragment cache `<!--themesia cache start-->` … `<!--themesia cache end 2026-11-15 05:47:51 0.0053-->` |
| CDN/WAF | Cloudflare; `cf-cache-status: DYNAMIC` untuk HTML (tidak di-cache). Tidak ada challenge pada sesi ini |
| HTTP | HTTP/2, `content-type: text/html; charset=UTF-8` |
| REST API | `https://anoboy.id/wp-json/` → 200 `application/json`; CPT `series` **tidak** diekspos (`/wp-json/wp/v2/series` → 404 `rest_no_route`) |
| Cloudflare challenge | **Tidak muncul**. `curl` UA default, `python-requests/2.32`, UA kosong, dan UA Chrome semuanya dapat 200 + HTML penuh |
| Cookie | Tidak perlu cookie/`cf_clearance` untuk HTML |

---

## 3. Capability matrix

| Kapabilitas | Y/N | Endpoint / bukti |
|---|---|---|
| Search | **Y** | `GET /?s=<q>` → 200. `?s=Nige` = 5 `article.bs`; `?s=jujutsu` = 1; `?s=naruto` = **0** + teks `<center><h3>Not Found</h3></center>` (bukan error — situs memang tak punya seri itu) |
| Ongoing list | **Y** | `GET /series/?status=ongoing&type=anime&order=update` → 10 item, `.epx` = `Ongoing` |
| Completed list | **Y** | `status=completed&type=anime` → 10 item, ada `<div class="status Completed">Completed</div>` |
| Detail series | **Y** | `GET /series/<slug>/` → 200, `h1.entry-title`, sinopsis, genre, status, daftar episode |
| Episode list | **Y** | `.eplister ul li` (descending, lengkap: seri 8 eps → 8 `<li>`) |
| Episode page | **Y** | `GET /<slug>-episode-N-subtitle-indonesia/` → 200, `<h1 class="entry-title">` |
| Stream — halaman | **Y (iframe)** | `#pembed iframe[src]` selalu ada; **tidak ada** `<video>`/mp4 langsung di halaman Anoboy |
| Stream — direct mp4 | **Parsial** | Embed `blogger.com` memuat JSON `streamingData.formats[].url` (googlevideo progresif, `itag 18` = 360p) — URL ada, tetapi fetch langsung dari environment ini **403** (lihat §7) |
| Download links | **N** | Tidak ada `class="dlbox"`, tidak ada teks `720p`/`360p` di halaman episode (grep 0 hasil) |
| AJAX/JSON katalog | **N** | REST `series` 404; live-search AJAX ada tapi parameternya belum terpetakan (§8) |
| Pagination | **Y** | `?page=N` / `/page/N/`; home & `/series/` 10–49 item per halaman |

---

## 4. URL pattern + contoh nyata

| Tipe | Pattern | Contoh nyata (diverifikasi) |
|---|---|---|
| Home / latest | `/` dan `/page/<n>/` | `https://anoboy.id/` (49 item), `https://anoboy.id/page/2/` (49 item, item pertama beda) |
| List series | `/series/?status=<ongoing\|completed\|upcoming>&type=anime&order=<update\|title\|latest>&genre[]=<slug>&page=<n>` | `https://anoboy.id/series/?status=ongoing&type=anime&order=update` |
| A–Z list | `/az-list/` , `/az-list/page/<n>/` , `/az-list/?show=<A..Z\|0-9>` | `https://anoboy.id/az-list/` (10 item), total 4 halaman = **36 seri** (10+10+10+6) → bukan katalog utama |
| Search | `/?s=<query>` | `https://anoboy.id/?s=Nige` |
| Detail series | `/series/<slug>/` | `https://anoboy.id/series/nige-jouzu-no-wakagimi-2nd-season/` |
| Episode | `/<series-slug>-episode-<N>-subtitle-indonesia/` | `https://anoboy.id/nige-jouzu-no-wakagimi-2nd-season-episode-1-subtitle-indonesia/` |
| Genre | `/genres/<slug>/` | `https://anoboy.id/genres/adventure/` |
| Studio / season / cast | `/studio/<slug>/`, `/season/<slug>/`, `/cast/<slug>/` | `https://anoboy.id/season/summer-2026/` |
| Embed (pihak ke-3) | lihat §7 | `https://www.blogger.com/video.g?token=...`, `https://gdplayer.to/embed/?...`, `https://turbovidhls.com/t/...` |

Catatan: **item di homepage adalah halaman episode, bukan halaman series.** `href` di `article.bs` homepage = URL episode; halaman series baru muncul lewat `link rel="series"`/`div.leftseries` atau search.

---

## 5. DOM map (selector persis + bukti)

### 5.1 List item (home, search, ongoing/completed, az-list)

```html
<article class="bs" itemscope="itemscope" itemtype="http://schema.org/CreativeWork">
 <div class="bsx">
  <a href="https://anoboy.id/nige-jouzu-no-wakagimi-2nd-season-episode-8-subtitle-indonesia/"
     itemprop="url" title="Nige Jouzu no Wakagimi 2nd Season Episode 8 Subtitle Indonesia"
     class="tip" rel="17212">
   <div class="limit">
     <div class="status Completed">Completed</div>          <!-- hanya sebagian item az-list -->
     <div class="typez Anime">Anime</div>
     <div class="ply"><i class="far fa-play-circle"></i></div>
     <div class="bt">
       <span class="epx">Ongoing</span>                     <!-- ATAU "Ep 9" di homepage -->
       <span class="sb Sub">Sub</span>
     </div>
     <img src="https://anoboy.id/wp-content/uploads/2026/07/1784307392-5023-156329.jpg"
          class="ts-post-image wp-post-image attachment-post-thumbnail size-post-thumbnail"
          itemprop="image" title="..." alt="..." width="225" height="315"/>
   </div>
   <div class="tt tts">
     Nige Jouzu no Wakagimi 2nd Season                      <!-- teks polos = judul seri -->
     <h2 itemprop="headline">Nige Jouzu no Wakagimi 2nd Season Episode 8 Subtitle Indonesia</h2>
   </div>
  </a>
 </div>
</article>
```

| Field | Selector | Contoh nilai |
|---|---|---|
| container | `article.bs` (varian: `article.bs.styletwo`) | 49 di home, 10 di `/series/` |
| href | `article.bs a.tip[itemprop="url"]` → `href` | `https://anoboy.id/yani-neko-episode-9-subtitle-indonesia/` |
| title panjang | `article.bs .tt h2[itemprop="headline"]` | `Yani Neko Episode 9 Subtitle Indonesia` |
| title seri | `article.bs .tt` (text node pertama, sebelum `<h2>`) | `Yani Neko` |
| thumb | `article.bs img.ts-post-image` → `src` | `.../wp-content/uploads/2026/07/1782989189-6042-156496.jpg` |
| tipe | `article.bs .limit .typez` | `Anime` / `Drama` |
| status/episode | `article.bs .limit .bt .epx` | **`Ep 9`** (home) **atau** `Ongoing`/`Completed` (list series) |
| subtitle badge | `article.bs .limit .bt .sb.Sub` | `Sub` |
| status khusus | `article.bs .limit .status` | `Completed` (class `status Completed`) |
| tanggal (hanya /series/) | `.epl-date` / `.epl-title` | lihat 5.2 |

### 5.2 Detail series (`/series/<slug>/`)

```html
<article id="post-17212" class="post-17212 hentry" itemscope itemtype="http://schema.org/CreativeWorkSeries">
  <div class="bixbox animefull">
    <div class="bigcontent noscr nobigcv">
      <div class="thumbook"><div class="thumb" itemprop="image">
        <img src=".../1784307392-5023-156329.jpg" class="ts-post-image wp-post-image ..."/>
      </div></div>
      <div class="infox">
        <h1 class="entry-title" itemprop="name">Nige Jouzu no Wakagimi 2nd Season</h1>
        <span class="alter">The Elusive Samurai Season 2, Nigewaka, 逃げ上手の若君 第二期</span>
        <div class="info-content"><div class="spe">
          <span><b>Status:</b> Ongoing</span>
          <span><b>Studio:</b> <a href=".../studio/cloverworks/">CloverWorks</a></span>
          <span class="split"><b>Released:</b> 2026</span>
          <span><b>Season:</b> <a href=".../season/summer-2026/">Summer 2026</a></span>
          <span><b>Type:</b> Anime</span>
        </div>
        <div class="genxed"><a href=".../genres/adventure/" rel="tag">Adventure</a> ...</div>
        <div class="desc">Watch streaming <b>...</b> ...</div>
      </div></div>
    </div>
  </div>

  <div class="bixbox synp">
    <div class="releases"><h2>Synopsis Nige Jouzu no Wakagimi 2nd Season</h2></div>
    <div class="entry-content" itemprop="description"><p>Second season of Nige Jouzu no Wakagimi.</p></div>
  </div>

  <div class="bixbox bxcl epcheck">
    <div class="lastend">
      <div class="inepcx"><a href="#"><span>First Episode</span>
        <span class="epcur epcurfirst">Episode 1</span></a></div>
      <div class="inepcx"><a href=".../episode-8-.../"><span>New Episode</span>
        <span class="epcur epcurlast">Episode 8</span></a></div>
    </div>
    <div class="eplister">
      <ul>
        <li data-index="0"><a href=".../episode-8-subtitle-indonesia/">
          <div class="epl-num">8</div>
          <div class="epl-title">Nige Jouzu no Wakagimi 2nd Season Episode 8 Subtitle Indonesia</div>
          <div class="epl-date">September 2, 2026</div></a></li>
        ...
      </ul>
    </div>
  </div>
</article>
```

| Field | Selector | Contoh |
|---|---|---|
| ID post | `article[id^="post-"]` → `id` | `post-17212` |
| judul | `h1.entry-title[itemprop="name"]` | `Nige Jouzu no Wakagimi 2nd Season` |
| judul alternatif | `.alter` | `The Elusive Samurai Season 2, Nigewaka, 逃げ上手の若君 第二期` |
| status | `.spe span` yang mengandung `Status:` (parse teks setelah `<b>Status:</b>`) | `Ongoing`, `Completed` |
| studio / season / tahun | `.spe span a[href*="/studio/"]`, `a[href*="/season/"]`, `Released:` | `CloverWorks`, `Summer 2026`, `2026` |
| genre | `.genxed a` (tag) | `Adventure`, `Comedy`, `Historical`, `Samurai`, `Shounen` |
| sinopsis | `.synp .entry-content[itemprop="description"]` | `<p>Second season of Nige Jouzu no Wakagimi.</p>` |
| sinopsis ringkas (opsional) | `.desc` / `.desc.mindes` | deskripsi boilerplate/english |
| jumlah episode | `.eplister ul li` count (atau ambil `.epl-num` terbesar) | 8 (seri ongoing), 3 (Kisaki, Completed) |
| episode pertama / terakhir | `.lastend .epcurfirst`, `.lastend .epcurlast` | `Episode 1`, `Episode 8` — **`epcurfirst` href = `#` (bug situs)** |
| cover | `.thumbook img.ts-post-image` | `.../1784307392-5023-156329.jpg` |

### 5.3 Episode page

```html
<article id="post-..." class="... hentry">
  <h1 class="entry-title" itemprop="name">Ao no Miburo: Serizawa Ansatsu-hen Episode 14 Subtitle Indonesia</h1>
  ...
  <span class="entry-author-name">admin</span> · Series <a href="https://anoboy.id/series/ao-no-miburo-serizawa-ansatsu-hen/">Ao no Miburo: Serizawa Ansatsu-hen</a>
  ...
  <div class="video-content">
    <div id="embed_holder" class="lowvid">
      <div class="player-embed" id="pembed">
        <iframe src="https://www.blogger.com/video.g?token=AD6v5dw..." height="380" width="100%"
                frameborder="0" scrolling="no"
                sandbox="allow-scripts allow-pointer-lock allow-forms allow-same-origin"
                allowfullscreen></iframe>
      </div>
    </div>
  </div>
  <div class="item video-nav"><div class="mobius">
    <select class="mirror" name="mirror" onchange="loadMi(this);" aria-label="mirror">
      <option value="">Select Video Server</option>
      <option value="PGlmcmFtZSBzcmM9..." ></option>   <!-- base64 HTML iframe -->
    </select>
  </div></div>
  ...
  <div class="single-info bixbox">                  <!-- kartu seri di bawah player -->
    <div class="thumb"><img .../></div>
    <div class="infox"><div class="infolimit"><h2>Yani Neko</h2><span class="alter">...</span></div>
      <div class="spe"><span><b>Status:</b> Ongoing</span> ...</div>
      <div class="genxed">...</div>
      <div class="desc mindes">sinopsis english ...</div>
    </div>
  </div>
```

| Field | Selector | Contoh |
|---|---|---|
| judul episode | `h1.entry-title[itemprop="name"]` | `Blue Lock vs. U-20 Japan Episode 14 Subtitle Indonesia` |
| nomor episode | **tidak ada elemen khusus** — regex judul `Episode\s+(\d+)` (atau `.epl-num` di list detail) | `14` |
| seri induk | link teks `Series <a href=".../series/<slug>/">`; lebih aman `a[href*="/series/"]` pertama di `.entry-meta`/header | `https://anoboy.id/series/yani-neko/` |
| player | `#pembed iframe` (fallback `.player-embed iframe`) → `src` | `https://www.blogger.com/video.g?token=...` |
| mirror base64 | `select.mirror option[value]` — decode base64 → potongan `<iframe src=...>` | 2 option: placeholder kosong + 1 server nyata |
| status seri | `.single-info .spe span` `Status:` | `Ongoing` |
| genre | `.single-info .genxed a` | `Adult Cast`, `Comedy`, `Seinen` … |
| sinopsis | `.single-info .desc.mindes` | teks english panjang |
| prev/next | **tidak ditemukan** markup `.naveps` nyata di halaman episode (hanya aturan CSS) | — |

---

## 6. AJAX / JSON

- **REST WordPress aktif** tetapi tanpa CPT katalog: `GET /wp-json/wp/v2/types` → hanya `post, page, attachment, nav_menu_item, wp_block, wp_template, ...`; `GET /wp-json/wp/v2/series?per_page=1` → **404 `rest_no_route`**. Jadi tidak ada jalur JSON resmi untuk series/episode.
- **Live search AJAX** (inline script di setiap halaman):
  ```js
  jQuery(sf_input).ajaxyLiveSearch({"expand":false,
      "searchUrl":"https:\/\/anoboy.id\/?s=%s",
      "ajaxUrl":"https:\/\/anoboy.id\/wp-admin\/admin-ajax.php","delay":500, ...});
  ```
  Percobaan `admin-ajax.php?action=ajaxy_sf&sf_value=nige` → **400**. Parameter aksi belum terpetakan (butuh inspeksi `wp-content/plugins/ajaxy-search-form/*.js`). **Rekomendasi: jangan pakai AJAX, pakai `/?s=` HTML** — hasilnya sudah lengkap dalam HTML statis.
- Tidak ada endpoint JSON untuk episode/stream; semua data ada di HTML.

---

## 7. Stream extraction

### 7.1 Rantai

```
episode page  →  #pembed iframe[src]  →  host pihak ke-3  →  (mungkin) mp4/m3u8
```

Tidak ada direct `<video>`/`.m3u8` di domain Anoboy. Tiap episode **satu** server (select hanya berisi 1 option nyata, sisanya placeholder).

### 7.2 Host yang teramati (5 sampel episode, 2026-11-15)

| Host | Jumlah sampel | Bentuk | Status |
|---|---|---|---|
| `www.blogger.com/video.g?token=<opaque>` | 3/5 | Halaman player Blogger/Google Video | **200**, 9.353 B; berisi JSON `streamingData` |
| `gdplayer.to/embed/?<base64-ish>` | 1/5 | Player JS | DNS resolve publik (185.248.171.2), **tidak bisa di-fetch dari sandbox ini** (DNS lokal memblokir); donsetch tier-1 dapat 200 tapi render JS timeout |
| `turbovidhls.com/t/<id>` | 1/5 | Iframe player | **Domain mati**: DoH `dns.google` → `Status:0` **tanpa Answer record** (NXDOMAIN), `getent hosts` gagal. Episode lama yang menunjuk ke sini **tidak bisa diputar** |

### 7.3 Blogger embed — detail (paling penting)

`GET https://www.blogger.com/video.g?token=...` **tanpa Referer, dengan UA `curl/8.5.0`** → **200**, 9.334 B (kompres). Tidak butuh header khusus, tidak butuh cookie.

Di dalam HTML-nya ada string JSON ter-escape (`&quot;` + `\u003d`) berbentuk:

```
{"streamingData":{"formats":[{"itag":18,
  "url":"https://rr4---sn-npoldne7.googlevideo.com/videoplayback?expire=1794721945&ei=...&ip=157.85.206.96&id=38909bca53e5c8bf&itag=18&source=blogger&requiressl=yes&xpc=...&cps=246&met=1794693145,...",
  "mimeType":"video/mp4","qualityOrdinal":"QUALITY_ORDINAL_360P", ...}],
 "serverAbrStreamingUrl":"https://rr4---.../videoplayback?..."}}
```

Artinya: **mp4 progresif 360p tersedia sebagai URL langsung** (bukan HLS). Namun:

- Ekstraksi naif (`html.unescape` + regex) menghasilkan URL 754 char yang bila di-GET dengan `Range: bytes=0-1023` → **HTTP 403** (`server: gvs 1.0`, body kosong). Sudah dicoba 2 varian (dengan/tanpa koma) — tetap 403.
- Penyebab paling mungkin: URL terpotong saat decode bertingkat (`\u003d`, `&quot;`, koma pemisah) **atau** parameter `ip=157.85.206.96` mengikat token ke IP peminta; request lanjutan dari IP lain → 403.
- **Kesimpulan jujur**: jalur mp4 ada dan dapat diparse, tetapi **belum terbukti playable dari environment ini**. Perlu implementasi parser JSON yang benar (unescape 2 lapis lalu `serde_json`) dan uji di klien target.

### 7.4 Header yang dibutuhkan

| Request | Referer | UA | Hasil |
|---|---|---|---|
| HTML Anoboy | tidak perlu | apa pun (termasuk `python-requests`) | 200 |
| Blogger embed | **tidak perlu** (diuji tanpa Referer) | `curl/8.5.0` cukup | 200 |
| googlevideo mp4 | belum jelas | Chrome UA | **403** (lihat di atas) |
| gdplayer / turbovid | belum teruji | — | tidak tercapai / mati |

---

## 8. Anti-bot / robots

`https://anoboy.id/robots.txt` (200):

```
User-agent: *
Content-Signal: search=yes,ai-train=no,use=reference
Allow: /

User-agent: Amazonbot
Disallow: /
User-agent: Applebot-Extended
Disallow: /
User-agent: Bytespider
Disallow: /
User-agent: CCBot
Disallow: /
User-agent: ClaudeBot
Disallow: /
User-agent: Diffbot
Disallow: /
User-agent: Google-Extended
Disallow: /
User-agent: GPTBot
Disallow: /
User-agent: omgili
Disallow: /
```

- Untuk klien biasa: `Allow: /`, tanpa crawl-delay, **tidak ada Disallow** untuk path yang kita pakai.
- Blokir hanya untuk bot AI generatif — jika project ini dianggap "AI agent", secara harfiah termasuk yang di-`Disallow`. Catat sebagai keputusan produk, bukan hambatan teknis.
- Tidak ada rate-limit yang teramati (≈25 request dalam 20 menit, semua 200 tanpa 429).
- Tidak ada Cloudflare challenge, tidak perlu JS, tidak perlu cookie.

---

## 9. Parsing risks (diurutkan dari paling rapuh)

1. **`.epx` ambigu** — di homepage berisi `Ep 9` (nomor episode), di `/series/` berisi `Ongoing`/`Completed`. Parser harus regex `^Ep\s+\d+$` lalu fallback ke status. Ini jebakan paling mudah salah.
2. **`/series/` tanpa `type=anime` rusak** — `GET /series/` dan `/series/?status=ongoing` (tanpa `type`) mengembalikan 10 item **judul kosong** (`<h2 itemprop="headline"></h2>`), slug numerik (`/series/2422/`), thumbnail `noimg165px.png`, dan `.typez` = `Drama`. Selalu kirim `type=anime` (terbukti: hasil normal).
3. **Homepage = episode, bukan series** — `href` menuju halaman episode; butuh langkah ekstra ke `/series/<slug>/` untuk metadata seri.
4. **Nomor episode tidak punya elemen** di halaman episode → harus regex dari `h1.entry-title`; judul mengandung entity (`&#8220;`, `&#8211;`) yang harus di-decode.
5. **`.epcurfirst` href = `#`** — "First Episode" tidak bisa diklik/di-scrape; ambil episode 1 dari `.eplister li` terakhir.
6. **Daftar episode hanya dari `.eplister`** — 8 `<li>` untuk seri 8 episode (lengkap, descending), tetapi `data-index` tidak dijamin berurutan; jangan andalkan index, pakai `.epl-num`.
7. **Host embed mati** — `turbovidhls.com` NXDOMAIN; sebagian episode lama tidak punya stream yang bisa diputar. Rancang `Option<stream>` dan fallback UX.
8. **`select.mirror` menipu** — 2 option tapi hanya 1 server nyata; label option kosong. Decode base64 wajib.
9. **Ukuran `size_download` menyesatkan** — dengan `--compressed`, 20 KB "di download" = 139 KB HTML setelah dekompresi.
10. **Fragment cache** — komentar `<!--themesia cache end ...-->` menunjukkan HTML bisa berasal dari cache; konten bisa tertinggal beberapa menit dari DB.
11. **AZ list bukan katalog penuh** — hanya 36 seri di 4 halaman; `/series/?type=anime&order=update` punya ≥40 halaman (10/halaman, hal. 40 masih 10 item, hal. 80 kosong) → katalog riil ratusan judul.
12. **Cloudflare bisa berubah** — sesi ini bebas challenge; jangan hardcode asumsi "selalu 200 tanpa JS". Sediakan jalur browser (bladebro) sebagai fallback.

---

## 10. Rekomendasi implementasi (ringkas, tanpa kode)

- Base URL: `https://anoboy.id` (fallback `https://anoboy.nl`).
- Search: `GET /?s={q}` → parse `article.bs`; deteksi empty state dengan `h3` teks `Not Found`.
- List: `GET /series/?status=<...>&type=anime&order=update&page={n}` (page 1-based; tidak ada markup pagination → iterate sampai 0 item).
- Detail: `GET /series/{slug}/` → `h1.entry-title`, `.spe` status, `.genxed a`, `.synp .entry-content`, `.eplister li`.
- Episode: `GET {episode_url}` → `#pembed iframe[src]`; simpan host + URL embed apa adanya, resolve per-host (blogger = parse JSON, gdplayer = JS, turbovid = mati).
- User-Agent Chrome desktop + `Accept-Language: id-ID,id;q=0.9` sudah cukup; tidak perlu cookie.

---

## 11. Fixtures (`research/raw/anoboy/`)

| File | Sumber (status) | Ukuran | Isi |
|---|---|---|---|
| `home.html` | `https://anoboy.id/` (200) | 139.026 B | 49 `article.bs`, latest release |
| `list.html` | `/series/?status=ongoing&type=anime&order=update` (200) | 148.030 B | 10 item, `.epx=Ongoing` |
| `list_az.html` | `/az-list/` (200) | 97.622 B | 10 item + pagination 4 halaman |
| `search.html` | `/?s=Nige` (200) | 89.644 B | 5 hasil |
| `search_empty.html` | `/?s=naruto` (200) | 84.318 B | `<h3>Not Found</h3>` |
| `detail.html` | `/series/nige-jouzu-no-wakagimi-2nd-season/` (200) | 104.809 B | status Ongoing, 5 genre, 8 episode |
| `detail_completed.html` | `/series/kisaki-kyouiku-kara-nigetai-watashi/` (200) | 104.784 B | status Completed, 3 episode |
| `episode.html` | `/<ao-no-miburo...>-episode-14-.../` (200) | 120.161 B | iframe **blogger** |
| `episode_gdplayer.html` | `/<ballpark...>-episode-12-.../` (200) | 115.986 B | iframe **gdplayer.to** |
| `episode_turbovid.html` | `/yani-neko-episode-9-.../` (200) | 106.774 B | iframe **turbovidhls** (host mati) |
| `embed.html` | `https://www.blogger.com/video.g?token=...` (200, tanpa Referer) | 25.860 B | JSON `streamingData` + URL googlevideo |

Fixture probe lama (tidak dipakai): `probe/*.html` (anoboy.id, anoboy.nl, anoboy.wiki, anoboy.baby, 2 IP).

---

## 12. Unknowns / belum terverifikasi

1. **Apakah mp4 googlevideo benar-benar bisa diputar** — 403 dari environment ini; perlu uji di klien target dengan parser URL yang benar.
2. **Isi `gdplayer.to`** — DNS lokal memblokir; donsetch tier-1 200 tapi render JS timeout. Belum diketahui apakah mengembalikan m3u8 atau iframe lain.
3. **Berapa banyak episode lama memakai turbovidhls** (mati) vs blogger (hidup) — sampel 5 episode terlalu kecil untuk rasio.
4. **Parameter AJAX live-search** (`action=` apa) — belum dipetakan; tidak diperlukan jika pakai `/?s=`.
5. **Batas katalog** — antara halaman 41 dan 79 dari `/series/?type=anime&order=update`; jumlah total seri belum dihitung.
6. **Perilaku Cloudflare saat traffic tinggi / dari IP datacenter** — sesi ini bebas challenge, belum diuji di bawah beban.
7. **Apakah `anoboy.nl` katalognya identik** dengan `anoboy.id` (hanya dicek homepage + 1 episode; keduanya berfungsi).
8. **Endpoint download** — halaman episode yang dicek tidak punya `.dlbox`; belum dipastikan apakah ada halaman `/download/` terpisah.
