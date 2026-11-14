# Hunter No Sekai — Recon Report (read-only)

- **Tanggal recon:** 2026-11-15 (Asia/Jakarta), ~04:51–05:05
- **Target:** Hunter No Sekai (fansub/streaming anime sub Indo)
- **Status akhir:** **TIDAK AKTIF** — tidak ada domain HnS yang menyajikan katalog/streaming.
  Yang hidup hanya `huntersekai.site` (landing page statis ±6.6 KB, dibuat ±2026-11-07) yang
  mengarahkan ke dua situs lain (manga & hentai). Domain lama `.biz` / `.bio` / `.website` = parked,
  `.info` / `.me` = DNS mati.
- **Playable:** **Tidak.** Satu-satunya jejak player (arsip 2023) memakai embed Streamtape yang
  sekarang **HTTP 404**; domain player beta `player.anitoki.biz.id` sudah tidak resolve.
- **Kode:** tidak ada perubahan kode (sesuai instruksi recon-only).

---

## 1. Cara menemukan domain (jejak lengkap)

1. `web_search "hunter no sekai anime sub indo"` → tidak ada domain resmi di hasil (kebanyakan
   iQiyi/Dailymotion/YouTube/Facebook). Query kedua `"hunter no sekai" fansub domain` menemukan
   **linktr.ee/hunternosekai** ("Nonton Anime Sub Indo Di Hunter No Sekai") dan **about.me/hunternosekai**.
2. `curl https://linktr.ee/hunternosekai` → di HTML mentah ada:
   `https://huntersekai.biz`, `/anime-list/`, `/status/finished-airing/`, `/watch`.
3. `about.me/hunternosekai` → memuat tiga domain: **huntersekai.bio**, **huntersekai.site**,
   **huntersekai.web.id**.
4. Probe langsung: `.biz` → 302 ke `ww17.huntersekai.biz` (parking Bodis-style, 27 KB,
   consentmanager CMP, bukan situs). `.bio` → 302 ke `ww38.huntersekai.bio` (parking).
5. Sweep TLD `huntersekai.<tld>` (±50 TLD: me, id, my.id, com, net, org, xyz, info, site, bio,
   online, website, ws, best, cyou, sbs, vip, cc, tv, fun, store, shop, club, online, press, art,
   dev, pro, asia, cloud, space, today, life, world, moe, pw, bid, icu, buzz, wiki, one, lol, cfd,
   bar, bond, cf, ml, ga, gq, tk, …). Hasil: hanya `.site` (200, live), `.online` (401 parking),
   `.website` (200, "Buy this domain"), `.ws` (timeout), sisanya DNS mati.
6. Wayback CDX: `huntersekai.info` 200 s/d 2025-03; `huntersekai.biz` 200 s/d 2025-07;
   `huntersekai.site` capture pertama **2026-11-07** (200) — domain .site memang baru.
7. crt.sh `?q=huntersekai&output=json` → **502 Bad Gateway** (gagal, tidak dipakai).
8. Telegram `t.me/s/hunter_no_sekai` (arsip pesan 2023) memakai `huntersekai.info` untuk semua
   link episode → mengonfirmasi `.info` adalah domain operasional 2023. Channel itu sudah
   "off telegram sementara" dan pindah ke WhatsApp channel (tidak bisa dibaca tanpa JS/login).

**Kesimpulan penemuan:** brand HnS pernah beroperasi di `huntersekai.me` → `.biz` → `.info`
(teks di halaman arsip: *"Ganti domain dari me ke biz"*), lalu hilang. `huntersekai.site` bukan
situs HnS: isinya landing page SEO yang menyuruh klik ke `pojokmanga.fun` (manga) dan
`hentaimu.cyou` (hentai). Sangat mungkin ini domain expiring/expired yang dipakai untuk
menyiram traffic lama, bukan operator asli — tidak ada bukti kuat ke arah mana pun.

---

## 2. Status domain (terukur, 2026-11-15)

| Domain | DNS | HTTP | Isi |
|---|---|---|---|
| huntersekai.site | CF 172.67.182.44 / 104.21.91.245 | 200 (cloudflare) | Landing statis "Hunter No Sekai", canonical .site, CTA ke pojokmanga.fun & hentaimu.cyou. Footer "© 2026". Ada script CF challenge-platform di akhir body. |
| huntersekai.biz | 103.224.182.242 | 302 → `ww17.huntersekai.biz` (199.191.50.190) | Parking: consentmanager, tidak ada konten; semua path (termasuk `/anime-list/`, `/?s=naruto`, URL episode) tetap parking. |
| huntersekai.bio | 103.224.182.219 | 302 → `ww38.huntersekai.bio` | Parking. |
| huntersekai.info | — | DNS fail | Domain mati (dulu domain utama 2023–2025). |
| huntersekai.me | — | DNS fail | Domain generasi pertama. |
| huntersekai.web.id | — | DNS fail | — |
| huntersekai.website | 104.247.81.99 | 200 (Caddy) | "Buy this domain" (parkir jual domain). |
| hunter-nosekai.com / .id / .net / .xyz / .site / .me / .biz | — | DNS fail | Sesuai hint: tidak merespons. Semua varian hyphen mati. |
| player.anitoki.biz.id | — | DNS fail | Player beta HnS (dari arsip) sudah hilang. |
| streamtape.net/e/laZD76XXdOC7741 | CF 104.21.29.151 | **404** | Embed player contoh dari arsip 2023 sudah dihapus. |

Catatan: `.site` hanya 6.6 KB, Tailwind via CDN, tanpa katalog, tanpa player, tanpa JS app —
**bukan platform streaming**, jadi tidak ada yang bisa di-scrape.

---

## 3. Platform & infrastruktur (dari arsip situs asli)

Sumber: snapshot Wayback `20230610111511` halaman episode
`/15-bishoujo-hyouryuuki-episode-02-subtitle-indonesia/` (47.786 byte, tersimpan sebagai fixture).

- **CMS:** WordPress **6.2.2** (`<meta name="generator" content="WordPress 6.2.2" />`).
- **Theme:** `wp-content/themes/animestream2` (style.css?ver=1.1.1) — theme anime Indo generik,
  markup `layar` / `mirror-content` / `server-button` / `eplist` / `titlelist` / `tublok`.
- **Plugin terlihat:** `favorites` v2.3.3 (`wp-content/plugins/favorites/...`), Google Site Kit
  (`google_gtagjs`, `G-2ESMJHLET6`), Disqus (komentar), oEmbed/`wp-json`.
- **CDN/keamanan:** Cloudflare (beacon `static.cloudflareinsights.com`, script challenge-platform
  di `.site`; arsip menyertakan `__CF$cv$params`).
- **Iklan/popunder:** `//nutletcorvees.com/rSyzeT6NBnNPIMb/61187` dan
  `//bebloommulvel.com/r72ihx4B02axvq/61187` (script `data-cfasync="false" async`).
- **Aset gambar:** `https://zonawibu.biz/wp-content/uploads/...` (situs saudara, bukan host sendiri).
- **Konten:** bukan anime umum — mayoritas **ecchi/hentai + JAV sub Indo** (kategori
  "Jav Sub Indo", "Anime Uncensored", judul seperti `15 Bishoujo Hyouryuuki`, `Kutsujoku 2`,
  `Boku ni Sexfriend...`). Ini penting untuk kebijakan agregator.

### Capability matrix (kondisi arsip 2023; live = N/A)

| Kapabilitas | Ada? | Bukti |
|---|---|---|
| Katalog / anime list | Ya | nav: Home / Ongoing / Complete / Anime List (`/anime-list/`), `/status/finished-airing/` |
| Halaman seri | Ya | `/anime/{slug}/` (breadcrumb + tombol ALL) |
| Halaman episode | Ya | `/{slug}-episode-{n}-subtitle-indonesia/` |
| Pencarian | Ya (WP search) | `/?s={query}` (JSON-LD SearchAction `?s={search_term_string}`) |
| Player | Ya, **iframe Streamtape** (bukan HLS sendiri) | `iframe.mirror-content[name=stream_iframe]` |
| Multi-server | Terbatas: BETA (proxy), STREAM, Download | 3× `button.server-button` |
| Download | Ya, via shortlink `ouo.io` → Streamtape `/v/{id}` | onclick `location.href='https://ouo.io/qs/...?s=...'` |
| Batch | Ya (klaim di meta description) | "Batch" hanya di teks SEO |
| Jadwal rilis | Tidak terlihat | — |
| Login/akun | Tidak (kecuali favorite lokal + Disqus) | `favorites` pakai localStorage/nonce |
| API publik | Hanya `wp-json` standar | `wp-json/wp/v2/posts/12901`, oembed |

---

## 4. URL pattern

```
/                                                    homepage (Ongoing + Jav + Uncensored + Completed)
/anime/{series-slug}/                                halaman seri
/{series-slug}-episode-{n}-subtitle-indonesia/       halaman episode   (pola dominan)
/{series-slug}-episode-{n}/                          varian tanpa suffix (mis. ...ahheahe-episode-1/)
/anime-list/                                         daftar semua anime
/status/finished-airing/                             daftar tamat
/actor/{nama-aktor}/                                 halaman aktor (dari CDX; konten JAV)
/?s={query}                                          pencarian WordPress
/wp-json/wp/v2/posts/{id}                            REST posting
/cara-download-di-hunter-no-sekai/                   halaman panduan download
```
Slug = lowercase, hyphen, romanisasi judul; nomor episode **tanpa zero-pad** di slug
(`episode-02`, `episode-1`), tapi label UI "Episode 2".
Domain di dalam HTML arsip masih menyebut `huntersekai.me` (sisa generasi sebelumnya), sedangkan
canonical/asset memakai `.biz` — konfirmasi rotasi domain.

---

## 5. DOM map (selector; hanya yang terverifikasi dari HTML arsip)

Halaman **episode** (terverifikasi 100% dari `wb_ep_raw.html`):

| Elemen | Selector | Catatan |
|---|---|---|
| Judul | `h1.title-post` (di dalam `.post-body`/header) | teks = "{Series} Episode {n} Subtitle Indonesia" |
| Breadcrumb | `ol.breadcrumb > li` | item ke-2 = seri, ke-3 = episode |
| Panel player | `.stream > .layar` | wrapper |
| **Iframe player** | `.stream iframe.mirror-content` (attr `name="stream_iframe"`) | `src` = embed Streamtape |
| Tombol server | `.button-container button.server-button` | 3 tombol: BETA / STREAM / Download |
| BETA (proxy) | `button.server-button` pertama, `onclick="openStream('//player.anitoki.biz.id/embed2/?id={id}&aid={id}&host=streamtape&ahost=streamtape')"` | host mati |
| STREAM | `button.server-button` kedua, `onclick="openStream('//streamtape.net/e/{id}')"` | |
| Download | `button.server-button` ketiga, `onclick="location.href='https://ouo.io/qs/{code}?s=https://streamtape.net/v/{id}'"` | shortlink |
| Prev / Next / ALL | `.prevnext .flir a[rel=prev]`, `a[rel=next]`, `a[rel=all]` (href `/anime/{slug}/`) | |
| Placeholder opsi episode | `#opteps` (kosong di HTML, diisi JS/theme) | isi tidak terverifikasi |
| Toggle sinema | `#switch .lightSwitcher` | teks "Cinema OFF" |
| Tombol Info | `#switch button[name=info]` + `.content` | berisi pesan "Ganti domain dari me ke biz…" |
| Sinopsis/INFO | `.post-body .jcontent.jayapanel` (h3 Keywords / INFO / Informasi) | teks SEO panjang + imbauan ganti DNS |
| Blok related | `.xrelated-box .xrelated` → `a[href^="/anime/"]`, `img`, `.eplist`, `.starlist`, `.titlelist.tublok` | |
| Komentar | Disqus `#disqus_thread` / `<noscript>` | |
| Lightbox iklan | `#lightsoff` | penutup iklan otomatis 5 detik |

Halaman **home** (verifikasi terbatas — hanya teks/struktur, bukan class):
- Section berurutan: `Jav Sub Indo >`, `New Episode`, `Anime Uncensored >`, `Anime Completed >`.
- Item: judul + `Episode {n}` + rating numerik (mis. `5.89`, `6.21`), thumbnail.
- Nav footer: Home / Ongoing / Complete / Anime List.

**Tidak terverifikasi** (Wayback 429/404, jangan dianggap fakta): selector kartu di homepage,
selector daftar episode di `/anime/{slug}/`, selector form search, dan apakah `#opteps` diisi AJAX.

---

## 6. AJAX / perilaku JS

- **Player tidak lewat AJAX.** `openStream(url, name, quality)` =
  `window.open(url, 'stream_iframe')`; target `iframe[name=stream_iframe]` sudah ada di DOM.
  Jadi stream URL **langsung ada di atribut `onclick`/`src`** — tidak perlu render JS.
- Satu-satunya endpoint AJAX: `wp-admin/admin-ajax.php` untuk plugin **favorites**
  (`var tes_data = {"ajaxurl":"https://huntersekai.biz/wp-admin/admin-ajax.php","nonce":"13d688970e", ...}`).
  Nonce per-halaman; tidak relevan untuk streaming.
- `wp-json` terbuka: `/wp-json/wp/v2/posts/{id}`, `/wp-json/oembed/1.0/embed?url=...`.
- `openTab()` untuk tab kualitas/server (`tabcontent`/`tablinks`) — ada di theme, tidak dipakai
  di halaman episode contoh.
- Cloudflare beacon + challenge-platform script di `.site`.

---

## 7. Stream extraction (arsip)

- **Host:** `streamtape.net` (embed `/e/{fileId}`, download `/v/{fileId}`), fileId 15 char
  (`laZD76XXdOC7741`).
- **Direct vs iframe:** **iframe** (bukan direct MP4/HLS). Direct butuh resolver Streamtape
  (`/e/` → JS `robotlink`), tidak ada di HTML HnS.
- **Header:** tidak ada header khusus dari sisi HnS; iframe di-load langsung oleh browser.
  Embed dibuka tanpa Referer requirement yang terlihat.
- **Proxy BETA:** `player.anitoki.biz.id/embed2/?id=&aid=&host=&ahost=` (domain mati, 2026).
- **Download:** `ouo.io` shortlink (masih 200, tapi meneruskan ke Streamtape `/v/` yang 404).
- **Status 2026:** embed contoh 404; tidak ada mirror alternatif di HTML (hanya 1 iframe + 3 tombol).
- **Thumbnail/OG:** `zonawibu.biz/wp-content/uploads/...`; JSON-LD `VideoObject.embedUrl`
  = `https://streamtape.net/e/{id}` dan `regionsAllowed: "ID"`.

---

## 8. Anti-bot

- Cloudflare di semua domain (`.site` aktif, `.biz` parking juga di belakang CF/103.224.x).
- Tidak ada login, tidak ada rate-limit yang teramati (tidak bisa diuji, situs mati).
- Tidak ada Cloudflare Turnstile di halaman arsip; `.site` menyertakan script
  `challenge-platform/scripts/jsd/main.js` (pasif).
- Popunder/redirect iklan (`nutletcorvees.com`, `bebloommulvel.com`) dan shortlink `ouo.io`
  adalah gangguan utama untuk otomasi, bukan proteksi.
- Wayback sendiri membalas **429** untuk request beruntun — bukan anti-bot situs, tapi membatasi
  pengambilan fixture.

---

## 9. Risiko

1. **Situs mati** — implementasi scraper tidak ada gunanya sampai domain baru muncul.
2. **Konten dewasa** (hentai/JAV) bercampur anime biasa → risiko kebijakan produk dan hukum,
   bukan sekadar copyright.
3. **Domain rotation** agresif (`.me` → `.biz` → `.info` → mati); setiap domain baru harus
   ditemukan ulang; tidak ada kanal resmi yang stabil.
4. **Player pihak ketiga** (Streamtape) + shortlink `ouo.io` → link cepat mati, iklan berlapis.
5. **Situs parkir / domain squatting**: `huntersekai.site` dan `.website` memperlihatkan pola
   monetisasi domain expiring; jangan percaya brand di halaman tanpa verifikasi katalog.
6. **Kualitas markup**: theme `animestream2` generik, `#opteps` kosong, tidak ada API katalog →
   scraping rapuh secara struktural.

---

## 10. Fixture

Semua di `research/raw/hunter-no-sekai/`:

| File | Isi |
|---|---|
| `home_huntersekai.site.html` | Landing live `.site` (6.620 B) — satu-satunya halaman HnS yang 200 |
| `head_website.txt` | Header `huntersekai.website` (parkir jual domain) |
| `home_huntersekai.website.html` | Halaman "Buy this domain" |
| `home_huntersekai.biz.html` | Hasil redirect parking `ww17.huntersekai.biz` |
| `home_www17.html` | Body parking (consentmanager) |
| `linktree.html` + `aboutme.html` | Sumber penemuan domain (berisi `.biz`, `.bio`, `.site`, `.web.id`) |
| `cdx_info.json`, `cdx_biz.json`, `cdx_site.json` | Riwayat snapshot Wayback per domain |
| `cdx_biz_urls.json` | 60 URL `.biz` (pola episode/anime/actor/?s) |
| `wb_ep_raw.html` | **Fixture utama**: HTML arsip halaman episode (47.786 B, WordPress 6.2.2 + theme animestream2 + iframe Streamtape) |
| `wb_biz_home_20221031.html`, `wb_info_20240903151842.html`, `wb_ep_15bishoujo_ep02.html`, `wb_ep2_alt.html`, `wb_series_15bishoujo.html`, `wb_biz_20240108.html` | Respons **429** Wayback (620 B) — bukti percobaan, bukan konten |
| `wb_info_20250122145627.html` | Halaman parkir `.info` 2025 (1.054 B, `window.park`) |
| `wb_biz_20250710.html` | Snapshot `.biz` 2025-07 (1.064 B) |
| `home_hentaimu.cyou.html`, `home_pojokmanga.fun.html` | Tujuan redirect dari `.site` (untuk konteks jaringan) |
| `crtsh.json` | Respons 502 crt.sh |

Catatan: banyak file `wb_*` berukuran 620 B karena Wayback membalas 429; jangan salah kira
sebagai HTML situs.

---

## 11. Unknowns (belum terbukti)

1. **Domain resmi HnS saat ini** — tidak ditemukan. Tidak ada bukti bahwa HnS masih beroperasi.
2. Apakah `huntersekai.site` dipegang operator asli (portal redirect) atau squatter/domain
   monetizer. Bukti condong ke squatter (JSON-LD menyebut `pojokmanga.fun`, CTA ke hentaimu,
   tidak ada katalog HnS).
3. Apakah ada domain baru di TLD yang tidak diuji (mis. `.biz.id` yang sempat disebut
   scamadviser, `.best`, `.cyou`, `.sbs`, `.rest`, `.vip`, `.my`, `.tv` — sebagian sudah diuji
   dan mati; `.biz.id` gagal resolve saat diuji tetapi mungkin pernah aktif 2024).
4. Selector halaman seri `/anime/{slug}/`, kartu homepage, dan form search — tidak terverifikasi
   (Wayback 404/429; halaman live tidak ada).
5. Apakah `#opteps` diisi via AJAX theme (tidak ada kode yang terlihat di halaman episode).
6. Apakah masih ada mirror resmi di WhatsApp channel
   (`whatsapp.com/channel/0029VaF7ouaLNSa7tnVvgn2m`) — butuh JS/login, tidak dibaca.
7. crt.sh tidak bisa dipakai (502) → enumerasi domain via certificate transparency belum selesai.
8. Apakah Streamtape file lain (selain contoh 2023) masih hidup — tidak ada katalog untuk diuji.

---

## 12. Rekomendasi untuk anicli-rs

- **Jangan implementasikan provider HnS sekarang**: tidak ada endpoint hidup untuk diuji, dan
  kontennya mayoritas dewasa (di luar cakupan agregator anime normal).
- Jika nanti domain baru muncul: sumber ekstraksi paling murah adalah
  `iframe.mirror-content[src]` (Streamtape `/e/{id}`) + tombol `onclick` (regex `openStream\('//([^']+)'`),
  tanpa perlu browser/JS. Katalog via `?s=` dan `/anime-list/` (WordPress HTML), bukan API.
- Simpan domain lama (`.me`, `.biz`, `.info`) sebagai indikator; cek periodik DNS + title
  untuk mendeteksi kebangkitan.
