//! Adapter Samehadaku: implementasi [`AnimeSource`] di atas client + parser.

pub mod client;
pub mod parse;

use std::collections::HashSet;
use std::sync::Arc;

use async_trait::async_trait;
use url::Url;

use crate::config::{Config, SourceConfig};
use crate::error::{SourceError, SourceResult};
use crate::model::{
    AnimeDetail, AnimeSearchResult, Episode, RequestHeaders, Resolution, ServerKind, StreamKind,
    StreamLink,
};
use crate::sources::{AnimeSource, Capabilities, SourceFactory};

use client::Samehadaku;
pub use client::NAME;

/// Factory yang didaftarkan di `sources::factories()`.
pub fn factory() -> SourceFactory {
    SourceFactory { name: NAME, build }
}

pub fn build(config: &Config, source_config: &SourceConfig) -> SourceResult<Arc<dyn AnimeSource>> {
    Ok(Arc::new(Samehadaku::new(config, source_config)?))
}

impl Samehadaku {
    /// Kumpulkan stream dari satu URL embed (resolver khusus per host).
    async fn collect_embed(
        &self,
        links: &mut Vec<StreamLink>,
        seen: &mut HashSet<String>,
        embed: &Url,
        fallback_resolution: Option<Resolution>,
        page_url: &Url,
    ) -> SourceResult<()> {
        match crate::embed::classify_server(embed) {
            ServerKind::Icefile => match self.icefile_sources(embed).await {
                Ok(sources) if !sources.is_empty() => {
                    for source in sources {
                        push_unique(links, seen, source);
                    }
                    return Ok(());
                }
                _ => {}
            },
            ServerKind::Blogspot => {
                if let Ok(Some(link)) = self.blogspot_media(embed).await {
                    push_unique(links, seen, link);
                    return Ok(());
                }
            }
            ServerKind::Pixeldrain => {
                if let Some(link) =
                    self.direct_from_share_url(embed, fallback_resolution, "Pixeldrain".to_string())
                {
                    push_unique(links, seen, link);
                    return Ok(());
                }
            }
            _ => {}
        }

        // Tidak bisa di-resolve statis → tawarkan sebagai Embed (butuh yt-dlp/browser),
        // dengan referer halaman episode supaya pemutar/yt-dlp punya konteks.
        let server = crate::embed::classify_server(embed);
        let label = match fallback_resolution {
            Some(resolution) => format!("{server} {resolution}"),
            None => server.to_string(),
        };
        push_unique(
            links,
            seen,
            StreamLink {
                source: self.source_id().clone(),
                resolution: fallback_resolution,
                label,
                server,
                container: None,
                url: embed.clone(),
                headers: RequestHeaders {
                    referer: Some(page_url.clone()),
                    user_agent: None,
                },
                stream_kind: StreamKind::Embed,
            },
        );
        Ok(())
    }
}

fn push_unique(links: &mut Vec<StreamLink>, seen: &mut HashSet<String>, link: StreamLink) {
    if seen.insert(link.url.to_string()) {
        links.push(link);
    }
}

#[async_trait]
impl AnimeSource for Samehadaku {
    fn name(&self) -> &'static str {
        NAME
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities::all()
    }

    async fn search(&self, query: &str) -> SourceResult<Vec<AnimeSearchResult>> {
        Samehadaku::search(self, query).await
    }

    async fn ongoing(&self) -> SourceResult<Vec<AnimeSearchResult>> {
        Samehadaku::ongoing(self).await
    }

    async fn completed(&self) -> SourceResult<Vec<AnimeSearchResult>> {
        Samehadaku::completed(self).await
    }

    async fn detail(&self, url: &str) -> SourceResult<AnimeDetail> {
        Samehadaku::detail(self, url).await
    }

    async fn episodes(&self, anime_url: &str) -> SourceResult<Vec<Episode>> {
        let detail = Samehadaku::detail(self, anime_url).await?;
        Ok(detail.episodes)
    }

    async fn streams(&self, episode_url: &str) -> SourceResult<Vec<StreamLink>> {
        let page_url = Url::parse(episode_url).map_err(|err| {
            SourceError::parse(NAME, "episode", "url", format!("URL tidak valid: {err}"))
        })?;
        let fetch = self.episode(episode_url).await?;

        let mut links: Vec<StreamLink> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();

        // 1) Player default yang sudah ada di HTML (biasanya Blogspot/iframe statis).
        if let Some(embed) = &fetch.page.default_embed {
            self.collect_embed(&mut links, &mut seen, embed, None, &page_url)
                .await?;
        }

        // 2) Tombol server. Server yang embed-nya shortcode tidak bisa di-resolve;
        //    AJAX `player_ajax` hanya dipakai kalau tombol benar-benar punya data-post/nume
        //    dan tidak ada embed statis (dibatasi agar tidak membanjiri situs).
        for button in &fetch.page.servers {
            let has_static = button
                .embed
                .as_deref()
                .is_some_and(|embed| embed.contains("<iframe"))
                || button
                    .data_url
                    .as_deref()
                    .is_some_and(|url| !url.trim().is_empty());
            let allow_ajax = !has_static;
            if !has_static {
                continue; // shortcode: tidak ada handler di theme (DESIGN §22.2)
            }
            if let Some(embed) = self.embed_url(&fetch.page, button, allow_ajax).await? {
                self.collect_embed(&mut links, &mut seen, &embed, button.resolution, &page_url)
                    .await?;
            }
        }

        // 3) Bagian unduhan: pixeldrain bisa diputar langsung, host lain DownloadOnly.
        for row in &fetch.downloads {
            for host in &row.links {
                let resolution = row.resolution;
                let label = format!(
                    "{} {}{}",
                    host.host,
                    row.label,
                    if row.format.is_empty() {
                        String::new()
                    } else {
                        format!(" ({})", row.format)
                    }
                );
                if let Some(link) = self.direct_from_share_url(&host.url, resolution, label.clone())
                {
                    push_unique(&mut links, &mut seen, link);
                    continue;
                }
                let server = crate::embed::classify_server(&host.url);
                push_unique(
                    &mut links,
                    &mut seen,
                    StreamLink {
                        source: self.source_id().clone(),
                        resolution,
                        label,
                        server,
                        container: row.container,
                        url: host.url.clone(),
                        headers: RequestHeaders::with_referer(page_url.clone()),
                        stream_kind: StreamKind::DownloadOnly,
                    },
                );
            }
        }

        if links.is_empty() {
            return Err(SourceError::parse(
                NAME,
                "episode",
                "streams",
                "tidak ada server/unduhan yang bisa dipakai di halaman ini",
            ));
        }
        Ok(links)
    }
}
