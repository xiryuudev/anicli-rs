//! Client mkissa.to — GraphQL `api.mkissa.net` (tanpa challenge JS).

use std::sync::Arc;

use serde_json::json;
use url::Url;

use crate::config::{Config, SourceConfig};
use crate::error::{SourceError, SourceResult};
use crate::http::{HttpClient, MirrorSet};
use crate::model::{AnimeDetail, AnimeSearchResult, Episode, SourceId};

pub const NAME: &str = "mkissa";
pub const DEFAULT_BASE: &str = "https://mkissa.to";
pub const API_URL: &str = "https://api.mkissa.net/api";
pub const MARKER: &str = "mkissa";

#[derive(Debug)]
pub struct Mkissa {
    source: SourceId,
    mirrors: MirrorSet,
    http: Arc<HttpClient>,
}

impl Mkissa {
    pub fn new(config: &Config, source_config: &SourceConfig) -> SourceResult<Self> {
        let http = Arc::new(HttpClient::new(&config.http)?);
        let base = if source_config.base_url.trim().is_empty() {
            DEFAULT_BASE
        } else {
            source_config.base_url.trim()
        };
        let mirrors = MirrorSet::new(
            base,
            &source_config.mirrors,
            MARKER,
            config.http.mirror_cache_ttl_secs,
        )?;
        Ok(Self {
            source: SourceId::new(NAME),
            mirrors,
            http,
        })
    }

    pub async fn base_url(&self) -> SourceResult<Url> {
        self.mirrors.resolve(&self.http, NAME).await
    }

    async fn graphql(&self, query: &str) -> SourceResult<String> {
        let base = self.base_url().await?;
        let api = Url::parse(API_URL).map_err(|err| {
            SourceError::parse(NAME, "api", "url", format!("URL API tidak valid: {err}"))
        })?;
        let body = json!({ "query": query }).to_string();
        self.http
            .post_json_text(NAME, &api, &body, Some(&base))
            .await
    }

    pub async fn search(&self, query: &str) -> SourceResult<Vec<AnimeSearchResult>> {
        let base = self.base_url().await?;
        let escaped = escape_graphql(query);
        let gql = format!(
            "{{ shows(search:{{query:\"{escaped}\"}}, limit:24){{ edges{{ _id name englishName thumbnail }} }} }}"
        );
        let body = self.graphql(&gql).await?;
        super::parse::parse_search(&body, &base, &self.source)
    }

    pub async fn detail(&self, anime_url: &str) -> SourceResult<AnimeDetail> {
        let base = self.base_url().await?;
        let page_url = Url::parse(anime_url).map_err(|err| {
            SourceError::parse(NAME, "detail", "url", format!("URL tidak valid: {err}"))
        })?;
        let id = show_id_from_url(&page_url)
            .ok_or_else(|| SourceError::parse(NAME, "detail", "id", "tidak ada id show di URL"))?;
        let gql = format!(
            "{{ show(_id:\"{}\"){{ _id name englishName nativeName description thumbnail score status type episodeCount availableEpisodesDetail }} }}",
            escape_graphql(&id)
        );
        let body = self.graphql(&gql).await?;
        super::parse::parse_detail(&body, &page_url, &base, &self.source)
    }

    pub async fn episodes(&self, anime_url: &str) -> SourceResult<Vec<Episode>> {
        self.detail(anime_url).await.map(|detail| detail.episodes)
    }

    pub fn source_id(&self) -> &SourceId {
        &self.source
    }

    pub fn http(&self) -> &HttpClient {
        &self.http
    }
}

/// `/anime/<id>` → `<id>`.
pub fn show_id_from_url(url: &Url) -> Option<String> {
    let segments: Vec<&str> = url.path_segments()?.collect();
    let index = segments.iter().position(|segment| *segment == "anime")?;
    segments
        .get(index + 1)
        .map(|segment| (*segment).to_string())
}

fn escape_graphql(raw: &str) -> String {
    raw.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn show_id_is_extracted_from_url() {
        let url = Url::parse("https://mkissa.to/anime/u85xuZjGw7eb6u3um").unwrap();
        assert_eq!(show_id_from_url(&url).as_deref(), Some("u85xuZjGw7eb6u3um"));
    }

    #[test]
    fn graphql_strings_are_escaped() {
        assert_eq!(escape_graphql("a\"b\\c"), "a\\\"b\\\\c");
    }
}
