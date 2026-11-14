//! Pemilihan resolusi & stream (DESIGN §11).
//!
//! Aturan: URL `Direct` lebih dipilih daripada `Embed`; resolusi persis paling
//! dipilih, lalu terdekat ke atas/lower sesuai kebijakan; resolusi tak diketahui
//! paling akhir. `DownloadOnly` tidak pernah dipilih otomatis oleh pemutar.

use crate::config::ResolutionPolicy;
use crate::model::{Container, Resolution, StreamKind, StreamLink};

/// Apakah link ini bisa langsung diputar (mpv) atau setidaknya dicoba (embed)?
pub fn is_playable(link: &StreamLink) -> bool {
    matches!(link.stream_kind, StreamKind::Direct | StreamKind::Embed)
}

fn resolution_key(
    resolution: Option<Resolution>,
    preferred: Resolution,
    policy: ResolutionPolicy,
) -> Option<(u8, u16)> {
    match resolution {
        None => Some((3, 0)),
        Some(resolution) if resolution == preferred => Some((0, 0)),
        Some(_) if policy == ResolutionPolicy::ExactOnly => None,
        Some(resolution) if resolution > preferred => Some(match policy {
            ResolutionPolicy::NearestLower => (2, resolution.height() - preferred.height()),
            _ => (1, resolution.height() - preferred.height()),
        }),
        Some(resolution) => Some(match policy {
            ResolutionPolicy::NearestLower => (1, preferred.height() - resolution.height()),
            _ => (2, preferred.height() - resolution.height()),
        }),
    }
}

fn container_key(link: &StreamLink, prefer_mkv: bool) -> u8 {
    if !prefer_mkv {
        return 0;
    }
    match link.container {
        Some(Container::Mkv) => 0,
        Some(Container::Mp4) => 1,
        _ => 2,
    }
}

/// Kunci peringkat; kecil = lebih dipilih.
fn rank_key(
    link: &StreamLink,
    preferred: Resolution,
    policy: ResolutionPolicy,
    prefer_mkv: bool,
) -> Option<(u8, u8, u8, u16)> {
    if !is_playable(link) {
        return None;
    }
    let kind_rank = match link.stream_kind {
        StreamKind::Direct => 0,
        StreamKind::Embed => 1,
        StreamKind::DownloadOnly => return None,
    };
    let (resolution_class, delta) = resolution_key(link.resolution, preferred, policy)?;
    Some((
        kind_rank,
        resolution_class,
        container_key(link, prefer_mkv),
        delta,
    ))
}

/// Pilih satu link terbaik. `None` kalau tidak ada kandidat yang layak
/// (mis. policy `exact_only` dan resolusi persis tidak tersedia).
pub fn select_stream(
    links: &[StreamLink],
    preferred: Resolution,
    policy: ResolutionPolicy,
    prefer_mkv: bool,
) -> Option<&StreamLink> {
    links
        .iter()
        .filter_map(|link| rank_key(link, preferred, policy, prefer_mkv).map(|key| (key, link)))
        .min_by_key(|(key, _)| *key)
        .map(|(_, link)| link)
}

/// Semua kandidat playable, terurut sesuai preferensi (untuk picker UI).
pub fn ranked_streams(
    links: &[StreamLink],
    preferred: Resolution,
    policy: ResolutionPolicy,
    prefer_mkv: bool,
) -> Vec<&StreamLink> {
    type Ranked<'a> = ((u8, u8, u8, u16), &'a StreamLink);
    let mut ranked: Vec<Ranked<'_>> = links
        .iter()
        .filter_map(|link| rank_key(link, preferred, policy, prefer_mkv).map(|key| (key, link)))
        .collect();
    ranked.sort_by_key(|(key, _)| *key);
    ranked.into_iter().map(|(_, link)| link).collect()
}

/// Ringkasan resolusi yang tersedia (untuk UI "tersedia: 360p, 480p, 720p").
pub fn available_resolutions(links: &[StreamLink]) -> Vec<Resolution> {
    let mut resolutions: Vec<Resolution> = links
        .iter()
        .filter(|link| is_playable(link))
        .filter_map(|link| link.resolution)
        .collect();
    resolutions.sort_unstable();
    resolutions.dedup();
    resolutions
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{RequestHeaders, ServerKind, SourceId};
    use url::Url;

    fn link(
        resolution: Option<Resolution>,
        kind: StreamKind,
        container: Option<Container>,
    ) -> StreamLink {
        let id = resolution.map(|r| r.height()).unwrap_or(0);
        StreamLink {
            source: SourceId::new("test"),
            resolution,
            label: format!("{id}"),
            server: ServerKind::Mirror,
            container,
            url: Url::parse(&format!("https://example.test/{id}")).unwrap(),
            headers: RequestHeaders::default(),
            stream_kind: kind,
        }
    }

    #[test]
    fn exact_resolution_wins() {
        let links = vec![
            link(Some(Resolution::P360), StreamKind::Direct, None),
            link(Some(Resolution::P720), StreamKind::Direct, None),
            link(Some(Resolution::P1080), StreamKind::Direct, None),
        ];
        let selected = select_stream(
            &links,
            Resolution::P720,
            ResolutionPolicy::NearestHigher,
            false,
        )
        .unwrap();
        assert_eq!(selected.resolution, Some(Resolution::P720));
    }

    #[test]
    fn nearest_higher_prefers_higher_then_lower() {
        let links = vec![
            link(Some(Resolution::P480), StreamKind::Direct, None),
            link(Some(Resolution::P1080), StreamKind::Direct, None),
        ];
        let selected = select_stream(
            &links,
            Resolution::P720,
            ResolutionPolicy::NearestHigher,
            false,
        )
        .unwrap();
        assert_eq!(selected.resolution, Some(Resolution::P1080), "naik dulu");

        let selected = select_stream(
            &links,
            Resolution::P720,
            ResolutionPolicy::NearestLower,
            false,
        )
        .unwrap();
        assert_eq!(selected.resolution, Some(Resolution::P480), "turun dulu");
    }

    #[test]
    fn exact_only_returns_none_when_missing() {
        let links = vec![link(Some(Resolution::P480), StreamKind::Direct, None)];
        assert!(
            select_stream(&links, Resolution::P720, ResolutionPolicy::ExactOnly, false).is_none()
        );
    }

    #[test]
    fn direct_beats_embed_at_same_resolution() {
        let links = vec![
            link(Some(Resolution::P720), StreamKind::Embed, None),
            link(Some(Resolution::P720), StreamKind::Direct, None),
        ];
        let selected = select_stream(
            &links,
            Resolution::P720,
            ResolutionPolicy::NearestHigher,
            false,
        )
        .unwrap();
        assert_eq!(selected.stream_kind, StreamKind::Direct);
    }

    #[test]
    fn download_only_is_never_selected() {
        let links = vec![link(Some(Resolution::P720), StreamKind::DownloadOnly, None)];
        assert!(select_stream(
            &links,
            Resolution::P720,
            ResolutionPolicy::NearestHigher,
            false
        )
        .is_none());
    }

    #[test]
    fn unknown_resolution_is_last_resort() {
        let links = vec![
            link(None, StreamKind::Direct, None),
            link(Some(Resolution::P360), StreamKind::Direct, None),
        ];
        let selected = select_stream(
            &links,
            Resolution::P720,
            ResolutionPolicy::NearestHigher,
            false,
        )
        .unwrap();
        assert_eq!(selected.resolution, Some(Resolution::P360));
    }

    #[test]
    fn prefer_mkv_breaks_ties() {
        let links = vec![
            link(
                Some(Resolution::P720),
                StreamKind::Direct,
                Some(Container::Mp4),
            ),
            link(
                Some(Resolution::P720),
                StreamKind::Direct,
                Some(Container::Mkv),
            ),
        ];
        let selected = select_stream(
            &links,
            Resolution::P720,
            ResolutionPolicy::NearestHigher,
            true,
        )
        .unwrap();
        assert_eq!(selected.container, Some(Container::Mkv));
    }

    #[test]
    fn ranked_list_and_available_resolutions() {
        let links = vec![
            link(Some(Resolution::P360), StreamKind::Direct, None),
            link(Some(Resolution::P720), StreamKind::Direct, None),
            link(Some(Resolution::P720), StreamKind::Embed, None),
            link(Some(Resolution::P1080), StreamKind::DownloadOnly, None),
        ];
        let ranked = ranked_streams(
            &links,
            Resolution::P720,
            ResolutionPolicy::NearestHigher,
            false,
        );
        assert_eq!(ranked.len(), 3);
        assert_eq!(ranked[0].resolution, Some(Resolution::P720));
        assert_eq!(ranked[0].stream_kind, StreamKind::Direct);

        assert_eq!(
            available_resolutions(&links),
            vec![Resolution::P360, Resolution::P720]
        );
    }
}
