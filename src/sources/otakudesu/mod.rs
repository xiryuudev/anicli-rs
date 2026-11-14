//! Adapter Otakudesu: implementasi [`AnimeSource`] di atas client + parser.

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

use client::Otakudesu;
pub use client::NAME;

pub fn factory() -> SourceFactory {
    SourceFactory { name: NAME, build }
}

pub fn build(config: &Config, source_config: &SourceConfig) -> SourceResult<Arc<dyn AnimeSource>> {
    Ok(Arc::new(Otakudesu::new(config, source_config)?))
}

fn push_unique(links: &mut Vec<StreamLink>, seen: &mut HashSet<String>, link: StreamLink) {
    if seen.insert(link.url.to_string()) {
        links.push(link);
    }
}

impl Otakudesu {
    /// Mirror yang dicoba di-resolve via AJAX; dibatasi agar tidak membanjiri situs.
    const MAX_AJAX_MIRRORS: usize = 3;
}

#[async_trait]
impl AnimeSource for Otakudesu {
    fn name(&self) -> &'static str {
        NAME
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities::all()
    }

    async fn search(&self, query: &str) -> SourceResult<Vec<AnimeSearchResult>> {
        Otakudesu::search(self, query).await
    }

    async fn ongoing(&self) -> SourceResult<Vec<AnimeSearchResult>> {
        Otakudesu::ongoing(self).await
    }

    async fn completed(&self) -> SourceResult<Vec<AnimeSearchResult>> {
        Otakudesu::completed(self).await
    }

    async fn detail(&self, url: &str) -> SourceResult<AnimeDetail> {
        Otakudesu::detail(self, url).await
    }

    async fn episodes(&self, anime_url: &str) -> SourceResult<Vec<Episode>> {
        let detail = Otakudesu::detail(self, anime_url).await?;
        Ok(detail.episodes)
    }

    async fn streams(&self, episode_url: &str) -> SourceResult<Vec<StreamLink>> {
        let page_url = Url::parse(episode_url).map_err(|err| {
            SourceError::parse(NAME, "episode", "url", format!("URL tidak valid: {err}"))
        })?;
        let fetch = self.episode(episode_url).await?;

        let mut links: Vec<StreamLink> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        let mut has_direct = false;

        // 1) Player default `#pembed` — statis di HTML, biasanya desustream.
        if let Some(embed) = &fetch.page.default_embed {
            if crate::embed::classify_server(embed) == ServerKind::Desustream {
                match self.desustream_media(embed).await {
                    Ok(Some(link)) => {
                        has_direct = true;
                        push_unique(&mut links, &mut seen, link);
                    }
                    Ok(None) => {
                        push_unique(
                            &mut links,
                            &mut seen,
                            self.embed_link(embed, None, "Default"),
                        );
                    }
                    Err(err) => {
                        tracing::debug!(target: "anicli_rs::otakudesu", %err, "gagal resolve embed default");
                        push_unique(
                            &mut links,
                            &mut seen,
                            self.embed_link(embed, None, "Default"),
                        );
                    }
                }
            } else {
                push_unique(
                    &mut links,
                    &mut seen,
                    self.embed_link(embed, None, "Default"),
                );
            }
        }

        // 2) Mirror alternatif hanya di-resolve kalau belum ada URL langsung; resolusi
        //    yang lebih tinggi dicoba lebih dulu.
        if !has_direct {
            let mut mirrors = fetch.page.mirrors.clone();
            mirrors.sort_by_key(|mirror| std::cmp::Reverse(mirror.resolution));
            mirrors.truncate(Self::MAX_AJAX_MIRRORS);
            for mirror in &mirrors {
                let resolution = mirror.resolution;
                match self.resolve_mirror(mirror).await {
                    Ok(Some(embed)) => {
                        if crate::embed::classify_server(&embed) == ServerKind::Desustream {
                            match self.desustream_media(&embed).await {
                                Ok(Some(link)) => push_unique(&mut links, &mut seen, link),
                                _ => push_unique(
                                    &mut links,
                                    &mut seen,
                                    self.embed_link(
                                        &embed,
                                        resolution,
                                        format!(
                                            "{} {}",
                                            mirror.label,
                                            mirror
                                                .resolution
                                                .map(|r| r.to_string())
                                                .unwrap_or_default()
                                        ),
                                    ),
                                ),
                            }
                        } else {
                            push_unique(
                                &mut links,
                                &mut seen,
                                self.embed_link(
                                    &embed,
                                    resolution,
                                    format!(
                                        "{} {}",
                                        mirror.label,
                                        mirror
                                            .resolution
                                            .map(|r| r.to_string())
                                            .unwrap_or_default()
                                    ),
                                ),
                            );
                        }
                    }
                    Ok(None) => continue,
                    Err(err) => {
                        tracing::debug!(target: "anicli_rs::otakudesu", mirror = %mirror.label, %err, "mirror gagal di-resolve");
                    }
                }
            }
        }

        // 3) Unduhan selalu tersedia (halaman `/download/`) — v1 hanya ditandai.
        for row in &fetch.downloads {
            for host in &row.links {
                let resolution: Option<Resolution> = row.resolution;
                push_unique(
                    &mut links,
                    &mut seen,
                    StreamLink {
                        source: self.source_id().clone(),
                        resolution,
                        label: format!("{} {}", host.host, row.label),
                        server: crate::embed::classify_server(&host.url),
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
                "tidak ada embed/mirror di halaman episode",
            ));
        }
        Ok(links)
    }
}

/// Konversi `ServerKind::Desustream` → label untuk UI.
pub fn desustream_label() -> &'static str {
    "Desustream"
}

/// Resolusi default yang wajar untuk episode Otakudesu (360/480/720).
pub fn supported_resolutions() -> &'static [Resolution] {
    &[Resolution::P360, Resolution::P480, Resolution::P720]
}
