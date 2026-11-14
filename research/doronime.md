# Doronime — Recon Report

- Target: `https://doronime.id/` (fansub download anime sub Indo)
- Waktu recon: 2026-11-15 ~04:52–05:15 WIB (semua status code di bawah = hasil fetch nyata saat itu)
- Metode: `curl --compressed` + Chrome UA, cookie jar, tanpa browser. Tidak ada perubahan kode.

## 1. Verdict

**Doronime bukan situs streaming.** Tidak ada player, tidak ada iframe video, tidak ada `mp4`/`m3u8`, tidak ada endpoint "server A/B/C".
Satu-satunya jalur media adalah **download file** (mkv) via GDrive / AceFile / MediaFire yang digerbangi *safelink* ber-CSRF + interstitial pihak ketiga.

- Playable: **TIDAK** (0 elemen `<video>`, 0 iframe non-Discord, 0 referensi m3u8/mp4 di halaman episode).
- Downloadable: **YA**, tapi butuh 4 hop: `episode → /download?id= → POST /safelink → /go/<token> → shortener → file host`.
- `/embed` tidak ada; fixture embed sengaja tidak dibuat karena tidak ada objek embed.

## 2. Domain / platform

| Item | Nilai |
|---|---|
| Domain utama | `doronime.id` (200, Cloudflare, HTTP/2) |
| Mirror terverifikasi | `doroni.me` → 200, `<title>Home | Doronime</title>`, markup identik (dipakai juga untuk host gambar `doroni.me/images/cover.png`) |
| Subdomain | `stream.doronime.id` → 404 "Halaman tidak ditemukan"; `play.doronime.id` → DNS tidak resolve. Tidak ada jalur streaming. |
| Origin | Laravel/PHP (`XSRF-TOKEN` + `doronime_session` cookie, `meta csrf-token`, `/js/app.frontend.js`, Blade class `Content__*`) |
| CDN/WAF | Cloudflare (`server: cloudflare`, `cf-ray`, `cf-cache-status: DYNAMIC`, NEL/report-to). **Tidak** ada JS challenge / `cf-mitigated` pada GET halaman publik. |
| Cookie | `XSRF-TOKEN` (non-HttpOnly) + `doronime_session` (HttpOnly), Max-Age 7200 |
| Cache | `cache-control: no-cache, private` (SSR penuh, bukan SPA) |
| Sitemap | `https://doronime.id/sitemap.xml` → 200, 7.537 URL (6.091 `/anime`, 188 `/studio`, 187 `/genre`, 136 `/ost`, 68 `/season`) |

## 3. Capability matrix

| Kapabilitas | Status | Bukti |
|---|---|---|
| Home + feed episode terbaru | ✅ | `/` 200 |
| Daftar anime (thumbnail, paginated 20/hal) | ✅ | `/anime` 200, `nav.Pagination`, `?page=2` 200 |
| Daftar A–Z (1 halaman, 562 item) | ✅ | `/anime?view=list` 200, 562 `div.List__item` |
| Search | ✅ | `/search?s=janda` 200, 1 hasil `a.Card` |
| Detail anime + daftar episode | ⚠️ | `/anime/janda-cewek-salju` 200; tabel episode hanya 5 baris & urutan tidak kronologis (04v1, 03v1, 02v1, 01v1, 05) |
| Halaman episode | ✅ | `/anime/<slug>/episode-<n>` 200 |
| Link download per resolusi | ✅ | 6 link `/download?id=` (1080p & 720p × GDrive/AceFile/MediaFire), container `H264.mkv` |
| Resolusi link download (safelink) | ✅ | `POST /safelink` 200 → `{"message":"Menuju Link","data":{"url":"/go/OTgyNzc="}}` |
| Redirect ke file host | ✅ | `/go/OTgyNzc=` 302 → `pemendek.mekatik.my.id/st/?api=…&url=…drive.google.com/uc?export=download&id=…` |
| Stream / player / embed | ❌ | Tidak ada; satu-satunya iframe = widget Discord |
| Batch | ✅ | `/batch` 200; `/anime/<slug>/batch` (link di daftar) |
| Jadwal / genre / season / studio / OST | ✅ | `/schedule`, `/genre/aksi`, `/season/fall-2026`, `/studio/studio-hokiboshi`, `/ost` |
| API JSON publik | ❌ | Tidak ditemukan; search & list SSR. Satu-satunya AJAX = `POST /safelink` |

## 4. URL pattern + contoh

```
/                                             home + carousel + feed "Anime Terbaru"
/anime                                        daftar thumbnail, 20/hal, ?page=N
/anime?view=list                              daftar A–Z (562 item, tanpa paginasi)
/anime/<slug>                                 detail
/anime/<slug>/episode-<n>[v<k>][-uncen|-dosa] halaman episode
/anime/<slug>/batch  |  /batch                 batch
/search?s=<query>                             pencarian (GET, SSR)
/genre/<slug>  /season/<slug>  /studio/<slug> /producer/<slug> /artist/<slug>
/schedule  /movie  /ost  /ost/<slug>
/download?id=<laravel-crypt>                  halaman safelink (noindex, robots-disallow)
POST /safelink                                {id} → {data:{url:"…/go/<token>"}}
/go/<token>                                   butuh Referer → 302 shortener → file host
```

Contoh nyata:
- detail: `https://doronime.id/anime/janda-cewek-salju`
- episode: `https://doronime.id/anime/janda-cewek-salju/episode-05-dosa`
- episode varian: `…/episode-04v1-dosa`, `…/episode-06-uncen`, `…/episode-01v0`
- slug = lowercase, spasi → `-`, karakter non-ASCII dibuang/diromanisasi; suffix `v0/v1` dan `uncen/dosa` ikut ke URL
- download: `https://doronime.id/download?id=eyJpdiI6IlFtUkIyTEFvQi9hRVFjQ3ZrQ3Jkcmc9PSIs…` (Laravel `Crypt::encryptString`, base64, deterministik per baris)

## 5. DOM map (selector persis)

### 5.1 Home
```css
/* feed terbaru (12 kartu) */
div.Content__tabs-body.Card__container > div.col-6 > div.Card
div.Card > div.Card__image > a[href]          /* href = /anime/<slug>/episode-<n>, title = judul */
div.Card > div.Card__caption > h6.Card__caption-title > a[href]
div.Card small[itemprop="author"] author       /* uploader */
div.Card small                                 /* teks "Rilis: 3 jam yang lalu" (relatif) */

/* carousel anime (Card--narrow) */
div.owl-carousel a.Card.Card--column[href="/anime/<slug>"]
a.Card > div.Card__image picture source[srcset]  /* webp lebih dulu */
a.Card > div.Card__caption > small               /* judul */
div.Card__badge > span.Badge                     /* status: Ongoing/Completed/Delay */
div.Card__badge--bottom span.Badge               /* tipe: TV/Movie, BD */
div.Card__badge--right span.Badge--warning       /* skor MAL, mis. "7.50" */
```

### 5.2 List thumbnail (`/anime`)
```css
div.Content__tabs-body.Card__container > div.col-6.col-sm-3.col-md-3 > a.Card.Card--column
a.Card[title]                                     /* judul */
a.Card > div.Card__caption > small
nav.Pagination > ul.Pagination__page > li > a.Pagination__page-link[href]  /* ?page=N, rel="next" */
```

### 5.3 List A–Z (`/anime?view=list`)
```css
div.List__container
div.List__group[id]            /* id="0-9" … id="Z" (heading huruf) */
div.List__group + div.List > div.List__item > span > a[href="/anime/<slug>"]
div.List__item a > small       /* judul */
/* catatan: tidak ada Pagination di mode ini; 562 item dalam satu dokumen */
```

### 5.4 Search (`/search?s=`)
```css
div.Content__tabs-body.Card__container > div.col-6.col-sm-3.col-md-3 > a.Card.Card--column
/* struktur identik list thumbnail; 1 hasil untuk "janda" */
```

### 5.5 Detail
```css
article.Content__tabs-content
h5.Content__title[itemprop="name"]              /* judul ID */
h5.Content__title + span                        /* judul Jepang */
div.Content__header-image picture img[src]
div.Content__header-caption-item                /* baris metadata */
  div.Content__header-caption-item > b          /* label: Romaji/English/Alternatif/Status/Durasi/Genre/Season/Studio/Tanggal Tayang/Total Episode/Skor MAL */
  div.Content__header-caption-item > span       /* nilai (bisa berisi <a>) */
div.Content__tabs-body div.Content__tabs-content--small p   /* sinopsis */

/* daftar episode */
div.Content__table > div.Content__table-body.form-row
  div.col > a[href]                             /* "Episode 05 [Dosa]" + href episode */
  div.col-9.col-md-7 > a                        /* judul panjang */
  div.d-none.d-md-block.col                    /* tanggal rilis "Nov 09, 2026" */
  a.Button.btn.btn-primary[href]               /* tombol Download → halaman episode */
```

### 5.6 Episode (download-only)
```css
h5.Content__title                                /* judul + "Subtitle Indonesia" */
div.Content__tabs-content-title > span[itemprop="author"]   /* "Oleh … , Rilis <time>" */
div.Content__description-title
div.Content__description-caption                 /* note softsub + sinopsis + translator/KFX */

div#contentLink.Content__link
  div.Download__title                            /* container: "H264.mkv" */
  div.Download__group
    div.Download__group-title                    /* "1080p" | "720p" */
    div.Download__link span > a[href^="https://doronime.id/download?id="]
      a > span.d-none.d-md-block                 /* label host: GDrive | AceFile | MediaFire */
      a > span.d-block.d-md-none                 /* label mobile: GD | AF | MF */
div#contentLinkInfo                              /* notice "Matikan AdBlock…" (d-none default) */
/* TIDAK ADA: video, source, iframe player, data-stream, m3u8 */
```

### 5.7 Halaman safelink (`/download?id=`)
```css
meta[name="csrf-token"]                          /* 40 char, WAJIB untuk POST /safelink */
button#SafelinkChecker[data-url="https://doronime.id/safelink"][data-id="<id>"][data-interval]
#SafelinkChecker #SafelinkCountdown[data-interval="1000"]   /* hitung mundur 4 detik */
a#SafelinkGenerate                               /* href diisi hasil AJAX lalu diklik */
<style> halaman noindex,nofollow </style>
```

## 6. AJAX / rantai download (terverifikasi)

```
GET  /anime/<slug>/episode-<n>                 → HTML, 6 anchor /download?id=<enc>
GET  /download?id=<enc>                        → HTML safelink + meta csrf-token (noindex)
POST /safelink                                 → 200 JSON
     headers: X-CSRF-TOKEN: <meta>, X-Requested-With: XMLHttpRequest,
              Referer: https://doronime.id/download?id=<enc>, Cookie: session+XSRF
     body:    id=<enc>            (application/x-www-form-urlencoded)
     resp:    {"message":"Menuju Link","data":{"url":"https://doronime.id/go/OTgyNzc="}}
GET  /go/<token>                               → 302
     Referer: https://doronime.id/download?id=<enc>   ← tanpa ini 302 ke homepage
     Location: https://pemendek.mekatik.my.id/st/?api=<32hex>&url=<urlencoded file-host URL>
GET  shortener /st/…                           → 302 /<hash>  ("MekatikLink", /assets/guard.js, noindex)
GET  shortener /<hash>                         → 200 gate ads → akhirnya file host
     contoh target: https://drive.google.com/uc?export=download&id=1nERV-…
```

Detail penting:
- `POST /safelink` **tanpa** `X-CSRF-TOKEN` dari meta halaman `/download` → `404 {"message":"Link tidak valid"}` (bukan 419). Token CSRF tidak ada di halaman episode, hanya di halaman `/download`.
- `/go/<token>` **deterministik per baris download** (`OTgyNzc=` sama di beberapa request), bukan sekali pakai; gerbangnya adalah header `Referer`. Tanpa Referer → `302 Location: https://doronime.id`.
- Host file yang terlihat di DOM: GDrive, AceFile, MediaFire (3 host × 2 resolusi). Tidak ada direct `.mp4`/`.m3u8`; yang dibagikan adalah file `.mkv` (softsub, "wajib mpv/vlc").
- Interstitial terakhir (`pemendek.mekatik.my.id`, judul "MekatikLink") adalah gerbang iklan pihak ketiga, bukan milik Doronime; ada `guard.js` + `noindex`.

## 7. Header / anti-bot

| Kebutuhan | Status |
|---|---|
| User-Agent Chrome | dipakai; tidak diuji tanpa UA (belum known-block) |
| Cookie sesi | `XSRF-TOKEN` + `doronime_session` (2 jam), wajib konsisten sepanjang rantai |
| CSRF | wajib untuk `POST /safelink` (`X-CSRF-TOKEN` dari `meta[name=csrf-token]`) |
| Referer | wajib untuk `POST /safelink` (dipakai) dan **wajib** untuk `/go/<token>` |
| X-Requested-With | dikirim (AJAX) |
| Cloudflare | proxy saja; GET publik 200 tanpa challenge. Halaman 404 CF-styled. Belum diuji dari IP datacenter / rate tinggi. |
| robots.txt | `User-agent: *` → `Disallow: /download`, `Disallow: /go/`; `Sitemap: https://doronime.id/sitemap.xml` |
| Lain-lain | countdown 4 dtk, cek AdBlock (`#contentLinkInfo`), ID terenkripsi, iklan (alwingulla tag.min.js zone 86246, Histats, GTM, FB SDK, simpli.fi, chatango) |

Implikasi: satu-satunya jalur media (`/download`, `/go/`) **secara eksplisit dilarang robots**. Publik boleh diindeks; pengambilan file tidak.

## 8. Risks

1. **Tidak playable** — tidak ada stream; kalau provider anicli-rs hanya mendukung "play", Doronime harus di-skip atau dipetakan ke mode download.
2. **Robots disallow** pada `/download` dan `/go/` → risiko etis/legal & potensi blokir; jangan jadikan default.
3. **Rantai rapuh**: CSRF per-halaman + cookie 2 jam + Referer-gated `/go` + shortener pihak ketiga (MekatikLink) yang bisa berubah/ganti domain kapan saja.
4. **Kuota file host** (GDrive abuse/quota, AceFile/MediaFire mati) — link bisa 404 walau halaman 200.
5. **HTML Laravel/Blade** dengan class BEM (`Content__*`, `Card__*`, `Download__*`); rawan berubah saat tema di-update. Tidak ada `data-*` stabil kecuali `data-id`/`data-url` di safelink.
6. **Daftar episode di detail tidak lengkap/berurut** (contoh: hanya 5 baris, `episode-05` di akhir) → enumerasi episode harus lewat sitemap/probe `episode-<n>` atau halaman batch, jangan andalkan tabel detail.
7. **Konten NSFW** bercampur (banyak `uncen`/`hentai`); perlu filter.
8. **Tanggal situs anomali** (rilis 2026, "3 jam yang lalu") — jangan dipakai untuk sorting ketat.
9. Volume: `/anime?view=list` = 562 item/323 KB dalam satu request; `/anime` 20/hal lebih ramah.

## 9. Fixtures

`research/raw/doronime/`:
- `home.html` (106 KB), `list.html` (`/anime?view=list`), `list_thumb.html` (`/anime`), `search.html` (`?s=janda`), `detail.html`
- `episode.html` + `episode_raw.html` (dua fetch `/anime/janda-cewek-salju/episode-05-dosa`), `ep_norm.html` (sampel kedua: Magical★Explorer ep08)
- `download.html` (halaman safelink + `meta csrf-token`), `safelink.json` (respons 200), `safelink_invalid.headers` (404 "Link tidak valid"), `safelink_ok.headers`
- `shortener.html` (gate MekatikLink)
- `go_redirect.headers` (302 → shortener), `go_without_referer.headers` (302 → homepage)
- `robots.txt`, `sitemap.xml` (7.537 URL), `batch.html`, `movie.html`
- `home.headers.txt`, `episode.headers.txt`, `download.headers.txt`
- `app.js`, `app.frontend.js` (bundle, untuk bukti handler safelink)
- **Tidak ada** `embed.*` — memang tidak ada objek embed/player.

## 10. Unknowns

- Apakah AceFile/MediaFire mengembalikan direct file juga (baru GDrive yang ditelusuri sampai `uc?export=download`).
- TTL / batas klik `/go/<token>` (deterministik + Referer-gated; belum diuji paralel/berulang jarak jauh).
- Apakah `POST /safelink` selalu butuh sesi yang sama dengan halaman `/download` (diuji dengan sesi sama; sesi beda belum).
- Rate limit / Cloudflare challenge pada burst request atau IP VPS.
- Mirror resmi lain selain `doroni.me` (tidak ada di sitemap/robots; `stream.*` 404, `play.*` tidak resolve).
- Apakah `/anime/<slug>/batch` memakai skema link yang sama (belum difetch detail batch-nya).
- Apakah ada API internal tersembunyi selain `/safelink` (tidak ditemukan di bundle JS).
