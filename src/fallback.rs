//! Orkestrasi fallback lintas source (DESIGN §10).
//!
//! Masalah inti: URL anime berbeda di tiap source, jadi fallback tidak bisa memakai
//! URL yang sama. Yang portabel adalah **identitas**: judul anime + nomor episode.
//! Karena itu, saat source utama gagal, layanan ini:
//!
//! 1. mencari judul di source berikutnya,
//! 2. memilih hasil paling mirip (ambang 0,72 — lebih baik gagal daripada salah anime),
//! 3. mengambil detail + daftar episode,
//! 4. mencocokkan nomor episode,
//! 5. baru meminta stream.
//!
//! Setiap percobaan dibatasi timeout dan dicatat sebagai [`Attempt`] supaya UI bisa
//! menampilkan rantai kegagalan (`samehadaku ✗ timeout · otakudesu ✓`).

use std::collections::HashSet;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::config::Config;
use crate::error::SourceError;
use crate::model::{AnimeDetail, AnimeSearchResult, Episode, PlaybackTarget, StreamLink};
use crate::sources::util;
use crate::sources::{AnimeSource, SourceRegistry};

/// Hasil satu percobaan source.
#[derive(Debug, Clone, PartialEq)]
pub enum AttemptOutcome {
    Success,
    /// Stream kosong / tidak ada episode yang cocok.
    Empty,
    Unsupported,
    Failed(SourceError),
    Timeout,
    NoMatch(String),
}

impl AttemptOutcome {
    pub fn is_success(&self) -> bool {
        matches!(self, Self::Success)
    }

    pub fn label(&self) -> String {
        match self {
            Self::Success => "ok".to_string(),
            Self::Empty => "kosong".to_string(),
            Self::Unsupported => "tidak didukung".to_string(),
            Self::Failed(error) => match error {
                SourceError::Http { status, .. } => format!("HTTP {status}"),
                SourceError::Timeout { .. } => "timeout".to_string(),
                SourceError::Blocked { .. } => "diblokir".to_string(),
                SourceError::Parse { field, .. } => format!("parse ({field})"),
                other => other.to_string(),
            },
            Self::Timeout => "timeout".to_string(),
            Self::NoMatch(what) => format!("tidak cocok ({what})"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Attempt {
    pub source: String,
    pub outcome: AttemptOutcome,
    pub elapsed_ms: u64,
}

/// Hasil sukses: link stream + source yang berhasil + jejak percobaan.
#[derive(Debug, Clone)]
pub struct ResolvedStreams {
    pub links: Vec<StreamLink>,
    pub source: String,
    pub attempts: Vec<Attempt>,
}

/// Hasil sukses untuk daftar episode (dipakai fallback saat detail gagal).
#[derive(Debug, Clone)]
pub struct ResolvedEpisodes {
    pub episodes: Vec<Episode>,
    pub detail: AnimeDetail,
    pub source: String,
    pub attempts: Vec<Attempt>,
}

/// Event pencarian streaming (dikirim per source begitu selesai).
#[derive(Debug)]
pub enum StreamSearchEvent {
    /// Hasil satu source (bisa kosong) atau errornya.
    Source {
        index: usize,
        source: String,
        found: Vec<AnimeSearchResult>,
        error: Option<SourceError>,
    },
    /// Semua source sudah merespons.
    Done { sources_tried: usize },
}

/// Hasil pencarian paralel lintas source (DESIGN §10.6).
#[derive(Debug, Clone, Default)]
pub struct SearchOutcome {
    /// Hasil gabungan, sudah didedupe berdasarkan judul ternormalisasi.
    pub results: Vec<AnimeSearchResult>,
    /// Source yang gagal/timeout (ditampilkan non-blocking di UI).
    pub errors: Vec<(String, SourceError)>,
    /// Berapa source yang mencoba & berapa yang memberi hasil.
    pub sources_tried: usize,
    pub sources_with_results: usize,
}

/// Kegagalan total: rantai percobaan untuk ditampilkan di UI/log.
#[derive(Debug, Clone)]
pub struct FallbackError {
    pub attempts: Vec<Attempt>,
}

impl std::fmt::Display for FallbackError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "gagal di {} source: {}",
            self.attempts.len(),
            self.attempts
                .iter()
                .map(|attempt| format!("{} ✗ {}", attempt.source, attempt.outcome.label()))
                .collect::<Vec<_>>()
                .join(" · ")
        )
    }
}

impl std::error::Error for FallbackError {}

impl FallbackError {
    /// Pesan siap tampil di TUI.
    pub fn user_message(&self) -> String {
        let chain = self
            .attempts
            .iter()
            .map(|attempt| format!("{} ✗ {}", attempt.source, attempt.outcome.label()))
            .collect::<Vec<_>>()
            .join(" · ");
        if chain.is_empty() {
            "Tidak ada source yang bisa dipakai. Coba lagi atau ganti source (s).".to_string()
        } else {
            format!("Semua source gagal ({chain}). Coba lagi atau ganti source (s).")
        }
    }
}

/// Layanan fallback. Dibuat sekali di startup; aman dipakai paralel (`&self`).
#[derive(Clone)]
pub struct PlaybackService {
    registry: Arc<SourceRegistry>,
    attempt_timeout: Duration,
    total_budget: Duration,
}

impl PlaybackService {
    pub fn new(registry: Arc<SourceRegistry>, config: &Config) -> Self {
        Self {
            registry,
            // search+detail+episodes+streams bisa beberapa request; beri ruang.
            attempt_timeout: Duration::from_secs((config.http.timeout_secs * 2).clamp(15, 60)),
            total_budget: Duration::from_secs(120),
        }
    }

    /// Urutan source yang dicoba: origin dulu, lalu `order`, hanya yang enabled/ada.
    fn plan(&self, origin: &str, order: &[&str]) -> Vec<Arc<dyn AnimeSource>> {
        let mut names: Vec<&str> = vec![origin];
        names.extend(order.iter().copied());
        let mut planned: Vec<Arc<dyn AnimeSource>> = Vec::new();
        for name in names {
            if planned.iter().any(|source| source.name() == name) {
                continue;
            }
            if let Some(source) = self.registry.get(name) {
                planned.push(source);
            }
        }
        planned
    }

    /// Ambil stream dengan fallback lintas source.
    pub async fn streams(
        &self,
        target: &PlaybackTarget,
        order: &[&str],
    ) -> Result<ResolvedStreams, FallbackError> {
        let deadline = Instant::now() + self.total_budget;
        let mut attempts = Vec::new();

        for source in self.plan(target.origin.as_str(), order) {
            if Instant::now() >= deadline {
                break;
            }
            let started = Instant::now();
            let outcome = self.try_streams(&source, target).await;
            let elapsed_ms = started.elapsed().as_millis() as u64;
            attempts.push(Attempt {
                source: source.name().to_string(),
                outcome: match &outcome {
                    Ok(_) => AttemptOutcome::Success,
                    Err(outcome) => outcome.clone(),
                },
                elapsed_ms,
            });
            match outcome {
                Ok(links) => {
                    return Ok(ResolvedStreams {
                        links,
                        source: source.name().to_string(),
                        attempts,
                    })
                }
                Err(_) => continue,
            }
        }

        Err(FallbackError { attempts })
    }

    /// Ambil episode (dan detail) dengan fallback; berguna kalau source asal tidak
    /// menyediakan daftar episode (mis. Kusonime) atau detail-nya gagal.
    pub async fn episodes(
        &self,
        target: &PlaybackTarget,
        order: &[&str],
    ) -> Result<ResolvedEpisodes, FallbackError> {
        let deadline = Instant::now() + self.total_budget;
        let mut attempts = Vec::new();

        for source in self.plan(target.origin.as_str(), order) {
            if Instant::now() >= deadline {
                break;
            }
            let started = Instant::now();
            let outcome = self.try_episodes(&source, target).await;
            let elapsed_ms = started.elapsed().as_millis() as u64;
            attempts.push(Attempt {
                source: source.name().to_string(),
                outcome: match &outcome {
                    Ok(_) => AttemptOutcome::Success,
                    Err(outcome) => outcome.clone(),
                },
                elapsed_ms,
            });
            if let Ok((detail, episodes)) = outcome {
                return Ok(ResolvedEpisodes {
                    episodes,
                    detail,
                    source: source.name().to_string(),
                    attempts,
                });
            }
        }

        Err(FallbackError { attempts })
    }

    async fn try_streams(
        &self,
        source: &Arc<dyn AnimeSource>,
        target: &PlaybackTarget,
    ) -> Result<Vec<StreamLink>, AttemptOutcome> {
        if !source.capabilities().streams {
            return Err(AttemptOutcome::Unsupported);
        }

        // Jalur cepat: source yang sama dengan asal, pakai URL episode langsung.
        if source.name() == target.origin.as_str() {
            return match run_with_timeout(
                self.attempt_timeout,
                source.streams(target.episode_url.as_str()),
            )
            .await
            {
                Ok(Ok(links)) if !links.is_empty() => Ok(links),
                Ok(Ok(_)) => Err(AttemptOutcome::Empty),
                Ok(Err(error)) => Err(AttemptOutcome::Failed(error)),
                Err(()) => Err(AttemptOutcome::Timeout),
            };
        }

        let episode = self.find_episode(source, target).await?;
        match run_with_timeout(self.attempt_timeout, source.streams(episode.url.as_str())).await {
            Ok(Ok(links)) if !links.is_empty() => Ok(links),
            Ok(Ok(_)) => Err(AttemptOutcome::Empty),
            Ok(Err(error)) => Err(AttemptOutcome::Failed(error)),
            Err(()) => Err(AttemptOutcome::Timeout),
        }
    }

    async fn try_episodes(
        &self,
        source: &Arc<dyn AnimeSource>,
        target: &PlaybackTarget,
    ) -> Result<(AnimeDetail, Vec<Episode>), AttemptOutcome> {
        if !source.capabilities().episodes {
            return Err(AttemptOutcome::Unsupported);
        }

        // Fast path: detail anime dari source asal.
        if source.name() == target.origin.as_str() {
            if let Ok(Ok(detail)) = run_with_timeout(
                self.attempt_timeout,
                source.detail(target.anime_url.as_str()),
            )
            .await
            {
                let episodes = detail.episodes.clone();
                if !episodes.is_empty() {
                    return Ok((detail, episodes));
                }
                let url = detail.url.to_string();
                match run_with_timeout(self.attempt_timeout, source.episodes(&url)).await {
                    Ok(Ok(episodes)) if !episodes.is_empty() => return Ok((detail, episodes)),
                    Ok(Ok(_)) => {}
                    Ok(Err(error)) => return Err(AttemptOutcome::Failed(error)),
                    Err(()) => return Err(AttemptOutcome::Timeout),
                }
            }
        }

        let episode = self.find_episode(source, target).await?;
        let detail = run_with_timeout(
            self.attempt_timeout,
            source.detail(&episode_anime_url(&episode, target)),
        )
        .await
        .map_err(|()| AttemptOutcome::Timeout)?
        .map_err(AttemptOutcome::Failed)?;
        let mut episodes = detail.episodes.clone();
        if episodes.is_empty() {
            episodes = run_with_timeout(self.attempt_timeout, source.episodes(detail.url.as_str()))
                .await
                .map_err(|()| AttemptOutcome::Timeout)?
                .map_err(AttemptOutcome::Failed)?;
        }
        Ok((detail, episodes))
    }

    /// Pencarian paralel ke semua source aktif (bukan fallback: semua dicoba).
    ///
    /// Hasil didedupe berdasarkan judul ternormalisasi: sumber pertama dalam `order`
    /// menang supaya preferensi user dihormati. Error per source tetap dikumpulkan
    /// untuk ditampilkan sebagai peringatan.
    /// Daftar source untuk pencarian sesuai `order` (fallback ke semua yang enabled).
    fn search_sources(&self, order: &[&str]) -> Vec<Arc<dyn AnimeSource>> {
        let mut sources: Vec<Arc<dyn AnimeSource>> = Vec::new();
        for name in order {
            if let Some(source) = self.registry.get(name) {
                if source.capabilities().search
                    && !sources.iter().any(|s| s.name() == source.name())
                {
                    sources.push(source);
                }
            }
        }
        if sources.is_empty() {
            for source in self.registry.sources() {
                if source.capabilities().search {
                    sources.push(source.clone());
                }
            }
        }
        sources
    }

    /// Pencarian paralel **streaming**: tiap source mengirim hasilnya sendiri
    /// begitu selesai. Task ini memiliki `JoinSet`; abort task (mis. `Esc`) ikut
    /// membatalkan seluruh pencarian source.
    pub async fn search_streaming(
        &self,
        query: &str,
        order: &[&str],
        tx: tokio::sync::mpsc::UnboundedSender<StreamSearchEvent>,
    ) {
        let sources = self.search_sources(order);
        let sources_tried = sources.len();

        let mut set: tokio::task::JoinSet<(
            usize,
            String,
            Result<Vec<AnimeSearchResult>, SourceError>,
        )> = tokio::task::JoinSet::new();
        for (index, source) in sources.into_iter().enumerate() {
            let query = query.to_string();
            let timeout = self.attempt_timeout;
            set.spawn(async move {
                tokio::time::sleep(Duration::from_millis(120 * index as u64)).await;
                let site = source.name();
                let outcome = match tokio::time::timeout(timeout, source.search(&query)).await {
                    Ok(Ok(results)) => Ok(results),
                    Ok(Err(error)) => Err(error),
                    Err(_) => Err(SourceError::Timeout {
                        site,
                        step: "search",
                    }),
                };
                (index, site.to_string(), outcome)
            });
        }

        while let Some(joined) = set.join_next().await {
            let Ok((index, source, outcome)) = joined else {
                continue;
            };
            let (found, error) = match outcome {
                Ok(found) => (found, None),
                Err(error) => (Vec::new(), Some(error)),
            };
            let _ = tx.send(StreamSearchEvent::Source {
                index,
                source,
                found,
                error,
            });
        }
        let _ = tx.send(StreamSearchEvent::Done { sources_tried });
    }

    pub async fn search_all(&self, query: &str, order: &[&str]) -> SearchOutcome {
        let sources = self.search_sources(order);
        let sources_tried = sources.len();
        let mut dedupe: HashSet<String> = HashSet::new();
        let mut results: Vec<AnimeSearchResult> = Vec::new();
        let mut errors: Vec<(String, SourceError)> = Vec::new();
        let mut sources_with_results = 0_usize;

        // Hasil dikumpulkan per indeks source supaya urutan akhir mengikuti
        // `order` (prioritas user), bukan siapa yang selesai lebih dulu.
        type SearchSlot = (String, Result<Vec<AnimeSearchResult>, SourceError>);
        let mut set: tokio::task::JoinSet<(usize, SearchSlot)> = tokio::task::JoinSet::new();
        for (index, source) in sources.into_iter().enumerate() {
            let query = query.to_string();
            let timeout = self.attempt_timeout;
            set.spawn(async move {
                // Stagger ringan supaya tidak menembak semua domain serentak.
                tokio::time::sleep(Duration::from_millis(120 * index as u64)).await;
                let site = source.name();
                let outcome = match tokio::time::timeout(timeout, source.search(&query)).await {
                    Ok(Ok(results)) => Ok(results),
                    Ok(Err(error)) => Err(error),
                    Err(_) => Err(SourceError::Timeout {
                        site,
                        step: "search",
                    }),
                };
                (index, (site.to_string(), outcome))
            });
        }

        let mut per_source: Vec<Option<SearchSlot>> = (0..sources_tried).map(|_| None).collect();
        while let Some(joined) = set.join_next().await {
            let Ok((index, slot)) = joined else {
                continue;
            };
            if let Some(slot_ref) = per_source.get_mut(index) {
                *slot_ref = Some(slot);
            }
        }

        for slot in per_source.into_iter().flatten() {
            let (site, outcome) = slot;
            match outcome {
                Ok(found) => {
                    if found.is_empty() {
                        continue;
                    }
                    sources_with_results += 1;
                    for result in found {
                        let key = util::normalize_title(&result.title);
                        if key.is_empty() || !dedupe.insert(key) {
                            continue;
                        }
                        results.push(result);
                    }
                }
                Err(error) => errors.push((site, error)),
            }
        }

        SearchOutcome {
            results,
            errors,
            sources_tried,
            sources_with_results,
        }
    }

    /// Cari anime (judul cocok) lalu episode yang cocok di satu source.
    async fn find_episode(
        &self,
        source: &Arc<dyn AnimeSource>,
        target: &PlaybackTarget,
    ) -> Result<Episode, AttemptOutcome> {
        let titles = target.candidate_titles();
        let mut last_error: Option<SourceError> = None;

        for title in titles {
            let results = match run_with_timeout(self.attempt_timeout, source.search(title)).await {
                Ok(Ok(results)) => results,
                Ok(Err(error)) => {
                    last_error = Some(error);
                    continue;
                }
                Err(()) => return Err(AttemptOutcome::Timeout),
            };
            let Some(matched) = best_title_match(&results, target) else {
                continue;
            };
            let detail =
                match run_with_timeout(self.attempt_timeout, source.detail(matched.url.as_str()))
                    .await
                {
                    Ok(Ok(detail)) => detail,
                    Ok(Err(error)) => {
                        last_error = Some(error);
                        continue;
                    }
                    Err(()) => return Err(AttemptOutcome::Timeout),
                };
            let episodes = if detail.episodes.is_empty() {
                match run_with_timeout(self.attempt_timeout, source.episodes(detail.url.as_str()))
                    .await
                {
                    Ok(Ok(episodes)) => episodes,
                    Ok(Err(error)) => {
                        last_error = Some(error);
                        continue;
                    }
                    Err(()) => return Err(AttemptOutcome::Timeout),
                }
            } else {
                detail.episodes.clone()
            };
            match match_episode(&episodes, target) {
                Some(episode) => return Ok(episode.clone()),
                None => {
                    return Err(AttemptOutcome::NoMatch(format!(
                        "episode {}",
                        target.episode_label
                    )))
                }
            }
        }

        Err(match last_error {
            Some(error) => AttemptOutcome::Failed(error),
            None => AttemptOutcome::NoMatch(format!("judul {:?}", target.anime_title)),
        })
    }
}

async fn run_with_timeout<F, T>(timeout: Duration, future: F) -> Result<T, ()>
where
    F: std::future::Future<Output = T>,
{
    tokio::time::timeout(timeout, future).await.map_err(|_| ())
}

fn episode_anime_url(_episode: &Episode, target: &PlaybackTarget) -> String {
    // Detail anime di source tujuan tidak selalu ada di Episode; pakai URL anime dari
    // target sebagai referensi (detail akan ditimpa oleh parser sesuai URL-nya).
    target.anime_url.to_string()
}

/// Pilih hasil pencarian yang paling mirip dengan target (ambang 0,72).
pub fn best_title_match<'a>(
    results: &'a [AnimeSearchResult],
    target: &PlaybackTarget,
) -> Option<&'a AnimeSearchResult> {
    results
        .iter()
        .map(|result| {
            let score = target
                .candidate_titles()
                .iter()
                .map(|title| title_similarity(title, &result.title))
                .fold(0.0_f32, f32::max);
            (score, result)
        })
        .filter(|(score, _)| *score >= 0.72)
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, result)| result)
}

/// Skor kemiripan judul: token-set Jaccard; 1.0 kalau token set identik.
pub fn title_similarity(a: &str, b: &str) -> f32 {
    let left = tokens(a);
    let right = tokens(b);
    if left.is_empty() || right.is_empty() {
        return 0.0;
    }
    if left == right {
        return 1.0;
    }
    let intersection = left.intersection(&right).count();
    let union = left.union(&right).count();
    intersection as f32 / union as f32
}

fn tokens(title: &str) -> HashSet<String> {
    util::normalize_title(title)
        .split_whitespace()
        .map(ToString::to_string)
        .collect()
}

/// Cocokkan episode berdasarkan nomor (utama) atau label (fallback).
pub fn match_episode<'a>(episodes: &'a [Episode], target: &PlaybackTarget) -> Option<&'a Episode> {
    if let Some(number) = target.episode_number {
        if let Some(episode) = episodes
            .iter()
            .find(|episode| episode.number == Some(number))
        {
            return Some(episode);
        }
    }
    let label_number = util::parse_episode_number(&target.episode_label);
    if let Some(number) = label_number {
        if let Some(episode) = episodes
            .iter()
            .find(|episode| episode.number == Some(number))
        {
            return Some(episode);
        }
    }
    episodes.iter().find(|episode| {
        !episode.label.is_empty() && episode.label.eq_ignore_ascii_case(&target.episode_label)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::{SourceError, SourceResult};
    use crate::model::{
        AiringStatus, AnimeDetail, Episode, MediaKind, RequestHeaders, Resolution, ServerKind,
        SourceId, StreamKind, StreamLink,
    };
    use crate::sources::mock::{MockData, MockSource};
    use crate::sources::{AnimeSource, Capabilities};
    use url::Url;

    fn url(raw: &str) -> Url {
        Url::parse(raw).unwrap()
    }

    fn target() -> PlaybackTarget {
        PlaybackTarget {
            anime_title: "Shuiro no Kamen".into(),
            alt_titles: vec![],
            episode_number: Some(2),
            episode_label: "Episode 2".into(),
            origin: SourceId::new("samehadaku"),
            anime_url: url("https://samehadaku.test/anime/shuiro/"),
            episode_url: url("https://samehadaku.test/shuiro-episode-2/"),
        }
    }

    fn detail() -> AnimeDetail {
        AnimeDetail {
            source: SourceId::new("otakudesu"),
            url: url("https://otakudesu.test/anime/shuiro/"),
            title: "Shuiro no Kamen".into(),
            alt_titles: vec![],
            synopsis: String::new(),
            genres: vec![],
            kind: Some(MediaKind::Tv),
            status: AiringStatus::Ongoing,
            year: Some(2026),
            score: None,
            total_episodes: Some(2),
            poster: None,
            episodes: vec![
                Episode {
                    number: Some(1),
                    label: "Episode 1".into(),
                    title: None,
                    url: url("https://otakudesu.test/episode/1"),
                    released: None,
                },
                Episode {
                    number: Some(2),
                    label: "Episode 2".into(),
                    title: None,
                    url: url("https://otakudesu.test/episode/2"),
                    released: None,
                },
            ],
        }
    }

    fn stream() -> StreamLink {
        StreamLink {
            source: SourceId::new("otakudesu"),
            resolution: Some(Resolution::P720),
            label: "720p".into(),
            server: ServerKind::Desustream,
            container: None,
            url: url("https://cdn.test/720.mp4"),
            headers: RequestHeaders::default(),
            stream_kind: StreamKind::Direct,
        }
    }

    fn origin_empty() -> Arc<dyn AnimeSource> {
        Arc::new(MockSource::new("samehadaku").with_data(MockData {
            streams: vec![],
            ..Default::default()
        }))
    }

    fn good_source() -> Arc<dyn AnimeSource> {
        Arc::new(MockSource::new("otakudesu").with_data(MockData {
            search: vec![AnimeSearchResult {
                source: SourceId::new("otakudesu"),
                title: "Shuiro no Kamen".into(),
                url: url("https://otakudesu.test/anime/shuiro/"),
                thumbnail: None,
                kind: Some(MediaKind::Tv),
                status: None,
                score: None,
            }],
            detail: Some(detail()),
            episodes: detail().episodes,
            streams: vec![stream()],
            ..Default::default()
        }))
    }

    fn service(sources: Vec<Arc<dyn AnimeSource>>) -> PlaybackService {
        let registry = Arc::new(SourceRegistry::new(sources));
        PlaybackService::new(registry, &Config::default())
    }

    #[tokio::test]
    async fn falls_back_to_next_source_when_origin_returns_empty() {
        let service = service(vec![origin_empty(), good_source()]);
        let resolved = service.streams(&target(), &["otakudesu"]).await.unwrap();
        assert_eq!(resolved.source, "otakudesu");
        assert_eq!(resolved.links.len(), 1);
        assert_eq!(resolved.attempts.len(), 2);
        assert!(matches!(
            resolved.attempts[0].outcome,
            AttemptOutcome::Empty
        ));
        assert!(resolved.attempts[1].outcome.is_success());
    }

    #[tokio::test]
    async fn skips_metadata_only_sources() {
        let metadata_only =
            Arc::new(MockSource::new("kusonime").with_capabilities(Capabilities::metadata_only()));
        let service = service(vec![origin_empty(), metadata_only]);
        let error = service.streams(&target(), &["kusonime"]).await.unwrap_err();
        assert!(matches!(
            error.attempts[1].outcome,
            AttemptOutcome::Unsupported
        ));
        assert!(error.user_message().contains("gagal"));
    }

    #[tokio::test]
    async fn reports_failed_sources_and_continues() {
        let failing = Arc::new(MockSource::new("otakudesu").failing(SourceError::Http {
            site: "otakudesu",
            url: "https://otakudesu.test/".into(),
            status: 523,
        }));
        let service = service(vec![origin_empty(), failing]);
        let error = service
            .streams(&target(), &["otakudesu"])
            .await
            .unwrap_err();
        assert_eq!(error.attempts.len(), 2);
        let labels: Vec<String> = error.attempts.iter().map(|a| a.outcome.label()).collect();
        assert!(labels.contains(&"HTTP 523".to_string()), "{labels:?}");
    }

    #[tokio::test]
    async fn episode_fallback_resolves_episode_on_other_source() {
        let service = service(vec![origin_empty(), good_source()]);
        let resolved = service.episodes(&target(), &["otakudesu"]).await.unwrap();
        assert_eq!(resolved.source, "otakudesu");
        assert_eq!(resolved.episodes.len(), 2);
        // Fast path origin: streams kosong bukan masalah untuk episodes; detail origin
        // gagal (mock tanpa detail) → fallback lewat pencarian.
    }

    #[tokio::test]
    async fn title_matching_prefers_exact_title() {
        let target = target();
        let results = vec![
            AnimeSearchResult {
                source: SourceId::new("x"),
                title: "Shuiro no Kamen Season 2".into(),
                url: url("https://x.test/s2"),
                thumbnail: None,
                kind: None,
                status: None,
                score: None,
            },
            AnimeSearchResult {
                source: SourceId::new("x"),
                title: "Shuiro no Kamen".into(),
                url: url("https://x.test/s1"),
                thumbnail: None,
                kind: None,
                status: None,
                score: None,
            },
        ];
        let matched = best_title_match(&results, &target).unwrap();
        assert_eq!(matched.title, "Shuiro no Kamen");
    }

    #[tokio::test]
    async fn title_matching_rejects_unrelated_results() {
        let target = target();
        let results = vec![AnimeSearchResult {
            source: SourceId::new("x"),
            title: "One Piece".into(),
            url: url("https://x.test/op"),
            thumbnail: None,
            kind: None,
            status: None,
            score: None,
        }];
        assert!(best_title_match(&results, &target).is_none());
    }

    #[test]
    fn episode_matching_uses_number_then_label() {
        let target = target();
        let episodes = detail().episodes;
        let matched = match_episode(&episodes, &target).unwrap();
        assert_eq!(matched.number, Some(2));

        let mut label_target = target.clone();
        label_target.episode_number = None;
        label_target.episode_label = "Episode 1".into();
        let matched = match_episode(&episodes, &label_target).unwrap();
        assert_eq!(matched.number, Some(1));
    }

    #[test]
    fn title_similarity_normalizes_suffixes() {
        assert_eq!(
            title_similarity("Shuiro no Kamen Sub Indo", "Shuiro no Kamen"),
            1.0
        );
        assert!(title_similarity("Shuiro no Kamen", "One Piece") < 0.2);
    }

    #[test]
    fn attempt_labels_are_ui_friendly() {
        assert_eq!(AttemptOutcome::Empty.label(), "kosong");
        assert_eq!(
            AttemptOutcome::Failed(SourceError::Timeout {
                site: "x",
                step: "search"
            })
            .label(),
            "timeout"
        );
        assert!(AttemptOutcome::NoMatch("episode 9".into())
            .label()
            .contains("episode 9"));
    }

    #[tokio::test]
    async fn parallel_search_merges_and_dedupes_results() {
        let first = Arc::new(MockSource::new("samehadaku").with_data(MockData {
            search: vec![
                search_result("One Piece", "https://a.test/op"),
                search_result("Naruto", "https://a.test/n"),
            ],
            ..Default::default()
        }));
        let second = Arc::new(MockSource::new("otakudesu").with_data(MockData {
            search: vec![
                search_result("One Piece Sub Indo", "https://b.test/op"),
                search_result("Bleach", "https://b.test/b"),
            ],
            ..Default::default()
        }));
        let service = service(vec![first, second]);
        let outcome = service
            .search_all("one piece", &["samehadaku", "otakudesu"])
            .await;
        assert_eq!(outcome.sources_tried, 2);
        assert_eq!(outcome.sources_with_results, 2);
        let titles: Vec<&str> = outcome.results.iter().map(|r| r.title.as_str()).collect();
        assert_eq!(
            titles.len(),
            3,
            "satu judul duplikat harus dibuang: {titles:?}"
        );
        assert!(
            outcome
                .results
                .iter()
                .any(|r| r.url.as_str() == "https://a.test/op"),
            "source pertama menang"
        );
        assert!(outcome.errors.is_empty());
    }

    #[tokio::test]
    async fn parallel_search_records_failed_sources() {
        let good = Arc::new(MockSource::new("samehadaku").with_data(MockData {
            search: vec![search_result("One Piece", "https://a.test/op")],
            ..Default::default()
        }));
        let bad = Arc::new(MockSource::new("otakudesu").failing(SourceError::Http {
            site: "otakudesu",
            url: "https://b.test/".into(),
            status: 523,
        }));
        let service = service(vec![good, bad]);
        let outcome = service
            .search_all("one piece", &["samehadaku", "otakudesu"])
            .await;
        assert_eq!(outcome.results.len(), 1);
        assert_eq!(outcome.errors.len(), 1);
        assert_eq!(outcome.errors[0].0, "otakudesu");
    }

    /// Source uji dengan delay buatan (untuk membuktikan urutan prioritas).
    struct SlowSource {
        name: &'static str,
        delay_ms: u64,
        results: Vec<AnimeSearchResult>,
    }

    #[async_trait::async_trait]
    impl AnimeSource for SlowSource {
        fn name(&self) -> &'static str {
            self.name
        }
        fn capabilities(&self) -> Capabilities {
            Capabilities::all()
        }
        async fn search(&self, _query: &str) -> SourceResult<Vec<AnimeSearchResult>> {
            tokio::time::sleep(Duration::from_millis(self.delay_ms)).await;
            Ok(self.results.clone())
        }
        async fn ongoing(&self) -> SourceResult<Vec<AnimeSearchResult>> {
            Ok(Vec::new())
        }
        async fn completed(&self) -> SourceResult<Vec<AnimeSearchResult>> {
            Ok(Vec::new())
        }
        async fn detail(&self, _url: &str) -> SourceResult<AnimeDetail> {
            Err(SourceError::unsupported(self.name, "detail"))
        }
        async fn episodes(&self, _anime_url: &str) -> SourceResult<Vec<Episode>> {
            Err(SourceError::unsupported(self.name, "episodes"))
        }
        async fn streams(&self, _episode_url: &str) -> SourceResult<Vec<StreamLink>> {
            Err(SourceError::unsupported(self.name, "streams"))
        }
    }

    #[tokio::test]
    async fn search_order_follows_priority_not_completion() {
        // Source prioritas lebih lambat; hasil akhir tetap harus dari source itu.
        let slow = Arc::new(SlowSource {
            name: "samehadaku",
            delay_ms: 80,
            results: vec![search_result("One Piece", "https://slow.test/op")],
        });
        let fast = Arc::new(SlowSource {
            name: "otakudesu",
            delay_ms: 0,
            results: vec![search_result("One Piece", "https://fast.test/op")],
        });
        let service = service(vec![slow, fast]);
        let outcome = service
            .search_all("one piece", &["samehadaku", "otakudesu"])
            .await;
        assert_eq!(outcome.results.len(), 1);
        assert_eq!(
            outcome.results[0].url.as_str(),
            "https://slow.test/op",
            "dedupe harus memilih source prioritas, bukan yang tercepat"
        );
        // (`search_result` helper memakai SourceId dummy; yang penting URL dari
        // source prioritas.)
        assert_eq!(outcome.sources_with_results, 2);
    }

    fn search_result(title: &str, url_raw: &str) -> AnimeSearchResult {
        AnimeSearchResult {
            source: SourceId::new("x"),
            title: title.into(),
            url: url(url_raw),
            thumbnail: None,
            kind: None,
            status: None,
            score: None,
        }
    }
}
