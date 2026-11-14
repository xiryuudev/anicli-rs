//! Test render TUI dengan `TestBackend` (tanpa terminal nyata) + logika navigasi.

use std::sync::Arc;
use std::time::Duration;

use anicli_rs::bookmarks::BookmarkEntry;
use anicli_rs::config::Config;
use anicli_rs::history::HistoryEntry;
use anicli_rs::logging::{LogBuffer, LogLine};
use anicli_rs::model::{
    AiringStatus, AnimeDetail, AnimeSearchResult, Container, Episode, MediaKind, RequestHeaders,
    Resolution, ServerKind, SourceId, StreamKind, StreamLink,
};
use anicli_rs::paths::Paths;
use anicli_rs::sources::mock::MockSource;
use anicli_rs::sources::SourceRegistry;
use anicli_rs::ui::{self, App};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::backend::TestBackend;
use ratatui::Terminal;

fn url(raw: &str) -> url::Url {
    url::Url::parse(raw).unwrap()
}

fn registry() -> Arc<SourceRegistry> {
    Arc::new(SourceRegistry::new(vec![
        Arc::new(MockSource::new("samehadaku")),
        Arc::new(MockSource::new("otakudesu")),
    ]))
}

fn sample_result(title: &str, status: AiringStatus) -> AnimeSearchResult {
    AnimeSearchResult {
        source: SourceId::new("samehadaku"),
        title: title.to_string(),
        url: url("https://v2.samehadaku.how/anime/x/"),
        thumbnail: None,
        kind: Some(MediaKind::Tv),
        status: Some(status),
        score: Some(8.7),
    }
}

fn sample_detail() -> AnimeDetail {
    AnimeDetail {
        source: SourceId::new("samehadaku"),
        url: url("https://v2.samehadaku.how/anime/x/"),
        title: "Shuiro no Kamen".into(),
        alt_titles: vec![],
        synopsis: "Topeng yang membawa malapetaka.".into(),
        genres: vec!["Action".into(), "Fantasy".into()],
        kind: Some(MediaKind::Tv),
        status: AiringStatus::Ongoing,
        year: Some(2026),
        score: Some(7.09),
        total_episodes: Some(6),
        poster: None,
        episodes: sample_episodes(),
    }
}

fn sample_episodes() -> Vec<Episode> {
    (1..=2)
        .map(|number| Episode {
            number: Some(number),
            label: format!("Shuiro no Kamen Episode {number}"),
            title: None,
            url: url(&format!(
                "https://v2.samehadaku.how/shuiro-no-kamen-episode-{number}/"
            )),
            released: None,
        })
        .collect()
}

fn sample_streams() -> Vec<StreamLink> {
    vec![
        StreamLink {
            source: SourceId::new("samehadaku"),
            resolution: Some(Resolution::P720),
            label: "Stream (720p)".into(),
            server: ServerKind::Icefile,
            container: None,
            url: url("https://beta.icefile.my.id/x.m3u8"),
            headers: RequestHeaders::default(),
            stream_kind: StreamKind::Direct,
        },
        StreamLink {
            source: SourceId::new("samehadaku"),
            resolution: Some(Resolution::P480),
            label: "Mega 480p".into(),
            server: ServerKind::Mega,
            container: Some(Container::Mp4),
            url: url("https://mega.nz/embed/x"),
            headers: RequestHeaders::default(),
            stream_kind: StreamKind::Embed,
        },
        StreamLink {
            source: SourceId::new("samehadaku"),
            resolution: Some(Resolution::P1080),
            label: "Acefile 1080p".into(),
            server: ServerKind::Acefile,
            container: Some(Container::Mkv),
            url: url("https://acefile.co/f/x"),
            headers: RequestHeaders::default(),
            stream_kind: StreamKind::DownloadOnly,
        },
    ]
}

fn paths() -> Paths {
    let tmp = tempfile::TempDir::new().unwrap();
    let paths = Paths::resolve(
        Some(tmp.path().join("config").join("config.toml")),
        Some(tmp.path().join("data")),
    )
    .unwrap();
    paths.ensure().unwrap();
    // Biarkan direktori sementara hidup selama proses test (dibersihkan OS).
    std::mem::forget(tmp);
    paths
}

fn app() -> App {
    App::new(
        Config::default(),
        registry(),
        "samehadaku".to_string(),
        paths(),
        None,
    )
}

fn app_with_logs(lines: Vec<LogLine>) -> App {
    let buffer = LogBuffer::new(64);
    for line in lines {
        buffer.push(line);
    }
    let mut app = App::new(
        Config::default(),
        registry(),
        "samehadaku".to_string(),
        paths(),
        Some(buffer),
    );
    app.log_index = app
        .log_buffer
        .as_ref()
        .map(|b| b.len().saturating_sub(1))
        .unwrap_or(0);
    app.screen = ui::Screen::Logs;
    app
}

fn sample_history() -> HistoryEntry {
    HistoryEntry {
        anime_title: "Shuiro no Kamen".into(),
        source: "samehadaku".into(),
        episode_number: Some(6),
        episode_label: "Episode 6".into(),
        anime_url: "https://v2.samehadaku.how/anime/shuiro-no-kamen/".into(),
        episode_url: "https://v2.samehadaku.how/shuiro-no-kamen-episode-6/".into(),
        resolution: Some("720p".into()),
        played_at: chrono::Local::now(),
        position_secs: None,
    }
}

fn sample_bookmark() -> BookmarkEntry {
    BookmarkEntry {
        title: "Shuiro no Kamen".into(),
        source: "samehadaku".into(),
        url: "https://v2.samehadaku.how/anime/shuiro-no-kamen/".into(),
        poster: None,
        added_at: chrono::Local::now(),
    }
}

fn render_text(app: &App, width: u16, height: u16) -> String {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|frame| ui::render(frame, app)).unwrap();
    let buffer = terminal.backend().buffer();
    let area = buffer.area;
    let mut text = String::new();
    for y in 0..area.height {
        for x in 0..area.width {
            let index = (y * area.width + x) as usize;
            text.push_str(buffer.content[index].symbol());
        }
        text.push('\n');
    }
    text
}

fn key(code: KeyCode) -> Event {
    Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
}

#[test]
fn search_screen_shows_input_results_and_help_hint() {
    let mut app = app();
    app.search_input = "shuiro".into();
    app.results = vec![
        sample_result("Shuiro no Kamen", AiringStatus::Ongoing),
        sample_result("Shuiro no Kamen Season 2", AiringStatus::Completed),
    ];
    app.input_focused = false;
    app.result_index = 1;
    app.status = "2 hasil".into();

    let text = render_text(&app, 100, 26);
    assert!(text.contains("anicli-rs"), "{text}");
    assert!(text.contains("samehadaku"), "{text}");
    assert!(text.contains("shuiro"), "{text}");
    assert!(text.contains("Hasil (2)"), "{text}");
    assert!(text.contains("Shuiro no Kamen"), "{text}");
    assert!(text.contains("2 hasil"), "{text}");
    assert!(text.contains("bantuan"), "{text}");
    // item kedua terpilih → penanda seleksi
    assert!(text.contains("▶"), "{text}");
}

#[test]
fn detail_screen_shows_metadata_and_synopsis() {
    let mut app = app();
    app.detail = Some(sample_detail());
    app.screen = ui::Screen::Detail;

    let text = render_text(&app, 100, 26);
    assert!(text.contains("Shuiro no Kamen"), "{text}");
    assert!(text.contains("Ongoing"), "{text}");
    assert!(text.contains("Tahun: 2026"), "{text}");
    assert!(text.contains("Action, Fantasy"), "{text}");
    assert!(text.contains("Sinopsis"), "{text}");
    assert!(text.contains("Topeng yang membawa malapetaka"), "{text}");
    assert!(text.contains("daftar episode"), "{text}");
}

#[test]
fn episodes_screen_lists_episodes() {
    let mut app = app();
    app.detail = Some(sample_detail());
    app.episodes = sample_episodes();
    app.episode_index = 1;
    app.screen = ui::Screen::Episodes;

    let text = render_text(&app, 100, 26);
    assert!(text.contains("Episode (2)"), "{text}");
    assert!(text.contains("Shuiro no Kamen Episode 1"), "{text}");
    assert!(text.contains("episode-2"), "{text}");
}

#[test]
fn streams_screen_marks_direct_links() {
    let mut app = app();
    app.streams = sample_streams();
    app.screen = ui::Screen::Streams;
    app.status = "3 link (1 siap mpv)".into();

    let text = render_text(&app, 110, 26);
    assert!(text.contains("[720p]"), "{text}");
    assert!(text.contains("direct"), "{text}");
    assert!(text.contains("embed"), "{text}");
    assert!(text.contains("download"), "{text}");
    assert!(text.contains("1 siap mpv"), "{text}");
    assert!(text.contains("IceFile"), "{text}");
}

#[test]
fn error_banner_is_visible() {
    let mut app = app();
    app.error = Some("samehadaku: HTTP 523. Mencoba source lain.".into());

    let text = render_text(&app, 110, 26);
    assert!(text.contains("HTTP 523"), "{text}");
    assert!(text.contains("coba lagi"), "{text}");
}

#[test]
fn help_overlay_documents_keymap() {
    let mut app = app();
    app.previous_screen = Some(ui::Screen::Search);
    app.screen = ui::Screen::Help;

    let text = render_text(&app, 100, 26);
    assert!(text.contains("bantuan"), "{text}");
    assert!(text.contains("ganti source"), "{text}");
    assert!(text.contains("navigasi daftar"), "{text}");
}

#[test]
fn keyboard_navigation_moves_selection_and_pops_screens() {
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let mut app = app();
    app.results = vec![
        sample_result("A", AiringStatus::Ongoing),
        sample_result("B", AiringStatus::Ongoing),
        sample_result("C", AiringStatus::Ongoing),
    ];
    app.input_focused = false;

    app.handle_input(key(KeyCode::Char('j')), &tx);
    assert_eq!(app.result_index, 1);
    app.handle_input(key(KeyCode::Down), &tx);
    assert_eq!(app.result_index, 2);
    app.handle_input(key(KeyCode::Char('j')), &tx);
    assert_eq!(app.result_index, 2, "tidak boleh melewati batas");
    app.handle_input(key(KeyCode::Char('k')), &tx);
    assert_eq!(app.result_index, 1);

    // Esc saat fokus hasil akan pop layar (Search tetap Search).
    app.screen = ui::Screen::Streams;
    app.handle_input(key(KeyCode::Esc), &tx);
    assert_eq!(app.screen, ui::Screen::Episodes);
    app.handle_input(key(KeyCode::Esc), &tx);
    assert_eq!(app.screen, ui::Screen::Detail);
    app.handle_input(key(KeyCode::Esc), &tx);
    assert_eq!(app.screen, ui::Screen::Search);

    // q keluar
    app.handle_input(key(KeyCode::Char('q')), &tx);
    assert!(app.quit);
}

#[test]
fn source_cycling_changes_active_source() {
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let mut app = app();
    // `s` hanya mengganti source saat tidak sedang mengetik (di input, huruf s = teks).
    app.input_focused = false;
    assert_eq!(app.source_name, "samehadaku");
    app.handle_input(key(KeyCode::Char('s')), &tx);
    assert_eq!(app.source_name, "otakudesu");
    // Setelah berganti source fokus kembali ke input; lepas dulu untuk menekan `s` lagi.
    app.input_focused = false;
    app.handle_input(key(KeyCode::Char('s')), &tx);
    assert_eq!(app.source_name, "samehadaku");
}

#[test]
fn ctrl_c_quits_even_while_typing() {
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let mut app = app();
    app.input_focused = true;
    app.handle_input(
        Event::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)),
        &tx,
    );
    assert!(app.quit);
}

#[test]
fn typing_updates_search_input_only_when_focused() {
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let mut app = app();
    app.search_input.clear();
    app.input_focused = true;

    for ch in "one piece".chars() {
        app.handle_input(key(KeyCode::Char(ch)), &tx);
    }
    assert_eq!(app.search_input, "one piece");

    // Tab ke hasil → karakter tidak lagi masuk input.
    app.results = vec![sample_result("A", AiringStatus::Ongoing)];
    app.handle_input(key(KeyCode::Tab), &tx);
    assert!(!app.input_focused);
    app.handle_input(key(KeyCode::Char('x')), &tx);
    assert_eq!(app.search_input, "one piece");
}

#[test]
fn stale_job_results_are_ignored() {
    let mut app = app();
    app.job = 7;
    app.handle_event(ui::AppEvent::SearchOk {
        job: 6,
        results: vec![sample_result("basi", AiringStatus::Ongoing)],
        errors: String::new(),
        sources: "1/1".into(),
    });
    assert!(app.results.is_empty(), "hasil job lama tidak boleh dipakai");

    app.handle_event(ui::AppEvent::SearchOk {
        job: 7,
        results: vec![sample_result("segar", AiringStatus::Ongoing)],
        errors: String::new(),
        sources: "1/1".into(),
    });
    assert_eq!(app.results.len(), 1);
    assert!(!app.loading);
}

#[test]
fn browse_screen_renders_items_and_tabs() {
    let mut app = app();
    app.screen = ui::Screen::Browse;
    app.browse_tab = ui::app::BrowseTab::Ongoing;
    app.browse_items = vec![
        sample_result("Shuiro no Kamen", AiringStatus::Ongoing),
        sample_result("Kusuriya no Hitorigoto", AiringStatus::Ongoing),
    ];
    app.browse_index = 1;
    app.status = "2 anime ongoing".into();

    let text = render_text(&app, 100, 26);
    assert!(text.contains("Browse"), "{text}");
    assert!(text.contains("ongoing"), "{text}");
    assert!(text.contains("completed"), "{text}");
    assert!(text.contains("Shuiro no Kamen"), "{text}");
    assert!(text.contains("Kusuriya"), "{text}");
    assert!(text.contains("2 item"), "{text}");
}

#[test]
fn history_screen_renders_entries() {
    let mut app = app();
    app.history.record(sample_history()).unwrap();
    app.screen = ui::Screen::History;

    let text = render_text(&app, 110, 26);
    assert!(text.contains("Riwayat"), "{text}");
    assert!(text.contains("Shuiro no Kamen"), "{text}");
    assert!(text.contains("Episode 6"), "{text}");
    assert!(text.contains("720p"), "{text}");
    assert!(text.contains("d: hapus"), "{text}");
}

#[test]
fn bookmarks_screen_renders_entries() {
    let mut app = app();
    app.bookmarks.add(sample_bookmark()).unwrap();
    app.screen = ui::Screen::Bookmarks;

    let text = render_text(&app, 110, 26);
    assert!(text.contains("Bookmark"), "{text}");
    assert!(text.contains("Shuiro no Kamen"), "{text}");
    assert!(text.contains("samehadaku"), "{text}");
}

#[test]
fn bookmark_toggle_from_detail_persists_state() {
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let mut app = app();
    app.detail = Some(sample_detail());
    app.screen = ui::Screen::Detail;

    app.handle_input(key(KeyCode::Char('b')), &tx);
    assert!(app.bookmarks.contains("https://v2.samehadaku.how/anime/x/"));
    assert!(app.status.contains("ditambahkan"), "{}", app.status);

    app.handle_input(key(KeyCode::Char('b')), &tx);
    assert!(!app.bookmarks.contains("https://v2.samehadaku.how/anime/x/"));
    assert!(app.status.contains("dihapus"), "{}", app.status);
}

#[test]
fn logs_screen_shows_buffer() {
    let mut app = app_with_logs(vec![
        LogLine {
            timestamp: "2026-11-15T04:00:00+07:00".into(),
            level: "INFO".into(),
            target: "anicli_rs::http".into(),
            message: "domain aktif ditemukan".into(),
        },
        LogLine {
            timestamp: "2026-11-15T04:00:01+07:00".into(),
            level: "WARN".into(),
            target: "anicli_rs::sources".into(),
            message: "mirror gagal".into(),
        },
    ]);
    app.status = "2 baris log".into();

    let text = render_text(&app, 120, 26);
    assert!(text.contains("Log"), "{text}");
    assert!(text.contains("domain aktif ditemukan"), "{text}");
    assert!(text.contains("mirror gagal"), "{text}");
}

#[test]
fn settings_screen_shows_effective_config() {
    let mut app = app();
    app.screen = ui::Screen::Settings;

    let text = render_text(&app, 120, 30);
    assert!(text.contains("Pengaturan"), "{text}");
    assert!(text.contains("mpv"), "{text}");
    assert!(text.contains("720p"), "{text}");
    assert!(text.contains("samehadaku"), "{text}");
    assert!(text.contains("fallback"), "{text}");
}

// Fake player memakai `/bin/true` (Unix). Di Windows tidak ada padanan yang aman
// untuk di-spawn dengan argumen mpv, jadi test ini hanya dijalankan di Unix;
// pencatatan riwayat sendiri diuji lintas platform di `src/history.rs`.
#[cfg(unix)]
#[tokio::test]
async fn playing_records_history_with_a_fake_player() {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let mut app = app();
    // Player palsu: /bin/true selalu ada di Linux CI/dev.
    app.config.playback.mpv_path = "/bin/true".into();
    app.detail = Some(sample_detail());
    app.episodes = sample_episodes();
    app.episode_index = 0;
    app.streams = sample_streams();
    app.stream_index = 0;
    app.screen = ui::Screen::Streams;

    app.handle_input(key(KeyCode::Char('p')), &tx);

    // Tunggu proses palsu selesai (PlayerExited) atau kegagalan spawn.
    for _ in 0..50 {
        if let Ok(Some(event)) = tokio::time::timeout(Duration::from_millis(100), rx.recv()).await {
            match event {
                ui::AppEvent::PlayerExited { .. } => break,
                ui::AppEvent::Failed { error, .. } => panic!("gagal spawn player: {error}"),
                _ => {}
            }
        }
    }

    assert!(
        !app.history.is_empty(),
        "riwayat harus tercatat saat memutar"
    );
    let entry = &app.history.entries()[0];
    assert_eq!(entry.anime_title, "Shuiro no Kamen");
    assert_eq!(entry.episode_number, Some(1));
    assert_eq!(entry.resolution.as_deref(), Some("720p"));
}

#[tokio::test]
async fn download_only_link_is_refused_without_spawning() {
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let mut app = app();
    app.detail = Some(sample_detail());
    app.episodes = sample_episodes();
    app.episode_index = 0;
    app.streams = vec![sample_streams()[2].clone()]; // DownloadOnly
    app.screen = ui::Screen::Streams;

    app.handle_input(key(KeyCode::Char('p')), &tx);
    assert!(app.error.as_deref().unwrap_or_default().contains("unduhan"));
    assert!(app.history.is_empty());
}
