//! State machine TUI + event loop async (DESIGN §13.2–§13.3).
//!
//! Semua I/O berjalan di task `tokio`; event loop hanya menggambar, membaca keyboard,
//! dan menerapkan hasil lewat channel. Fallback lintas source ditangani
//! [`PlaybackService`](crate::fallback::PlaybackService), pemutaran oleh
//! [`crate::player`].

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use crossterm::event::{Event, EventStream, KeyCode, KeyEvent, KeyModifiers};
use futures::StreamExt;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use tokio::sync::mpsc::{self, UnboundedSender};

use crate::bookmarks::{BookmarkEntry, Bookmarks};
use crate::config::{Config, SearchScope};
use crate::error::SourceError;
use crate::fallback::{Attempt, PlaybackService};
use crate::history::{History, HistoryEntry};
use crate::logging::LogBuffer;
use crate::model::{
    AiringStatus, AnimeDetail, AnimeSearchResult, Episode, Language, PlaybackTarget, Resolution,
    StreamKind, StreamLink,
};
use crate::paths::Paths;
use crate::player::{self, PlaybackPlan};
use crate::resolve;
use crate::sources::SourceRegistry;
use crate::ui::selector::{SelectorAction, SourceEntry, SourceSelector};

/// Layar aktif.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Search,
    Browse,
    Detail,
    Episodes,
    Streams,
    History,
    Bookmarks,
    Logs,
    Settings,
    Help,
}

/// Tab layar Browse.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowseTab {
    Ongoing,
    Completed,
}

/// Hasil job async yang dikirim ke event loop.
///
/// Sengaja tidak di-box: event berumur pendek dan hanya lewat satu channel.
#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum AppEvent {
    /// Hasil inkremental dari satu source (streaming search).
    SearchSource {
        job: u64,
        index: usize,
        source: String,
        items: Vec<AnimeSearchResult>,
        error: Option<String>,
        sources_tried: usize,
    },
    /// Semua source selesai merespons.
    SearchDone {
        job: u64,
        sources_with_results: usize,
        sources_tried: usize,
        errors: String,
    },
    DetailOk {
        job: u64,
        detail: AnimeDetail,
    },
    EpisodesOk {
        job: u64,
        detail: AnimeDetail,
        episodes: Vec<Episode>,
        source: String,
        attempts: String,
    },
    StreamsOk {
        job: u64,
        links: Vec<StreamLink>,
        source: String,
        attempts: String,
    },
    BrowseOk {
        job: u64,
        tab: BrowseTab,
        items: Vec<AnimeSearchResult>,
    },
    PlayerStarted {
        job: u64,
        program: String,
    },
    PlayerExited {
        program: String,
        code: Option<i32>,
        stderr: String,
    },
    /// Ada versi baru dari GitHub Releases (dikirim sekali saat start).
    UpdateAvailable {
        version: String,
        tag: String,
        url: String,
        hint: String,
    },
    /// Di Android: intent berhasil dikirim; playback terjadi di aplikasi lain.
    PlayerLaunched {
        program: String,
    },
    DownloadStarted {
        job: u64,
        path: PathBuf,
        total: Option<u64>,
    },
    DownloadProgress {
        job: u64,
        downloaded: u64,
        total: Option<u64>,
    },
    DownloadDone {
        job: u64,
        path: PathBuf,
    },
    DownloadFailed {
        job: u64,
        message: String,
    },
    Failed {
        job: u64,
        error: SourceError,
    },
}

/// State aplikasi. Field publik di dalam crate supaya render bisa murni & dites.
/// Progres unduhan aktif di layar Stream.
#[derive(Debug, Clone, Default)]
pub struct DownloadState {
    pub path: PathBuf,
    pub downloaded: u64,
    pub total: Option<u64>,
    pub done: bool,
    pub failed: Option<String>,
}

impl DownloadState {
    pub fn percent(&self) -> u8 {
        match self.total {
            Some(total) if total > 0 => {
                ((self.downloaded.saturating_mul(100) / total).min(100)) as u8
            }
            _ => 0,
        }
    }

    pub fn bar(&self, width: usize) -> String {
        let filled = (usize::from(self.percent()) * width / 100).min(width);
        format!("{}{}", "█".repeat(filled), "░".repeat(width - filled))
    }
}

pub struct App {
    pub config: Config,
    pub registry: Arc<SourceRegistry>,
    pub service: PlaybackService,
    pub screen: Screen,
    pub previous_screen: Option<Screen>,
    pub source_name: String,
    pub search_scope: SearchScope,
    pub search_slots: Vec<Option<Vec<AnimeSearchResult>>>,
    pub search_cancel: Option<tokio::task::AbortHandle>,
    pub selector: Option<SourceSelector>,
    /// Filter bahasa aktif (`None` = semua).
    pub language_filter: Option<Language>,
    /// Indeks pilihan di popup bahasa.
    pub language_index: usize,
    /// Popup bahasa sedang terbuka.
    pub language_popup: bool,
    /// Toast update yang sedang tampil (versi lebih baru).
    pub update_toast: Option<String>,
    pub update_version: Option<String>,
    pub status: String,
    pub loading: bool,
    pub error: Option<String>,
    pub search_input: String,
    pub input_focused: bool,
    pub results: Vec<AnimeSearchResult>,
    pub result_index: usize,
    pub detail: Option<AnimeDetail>,
    pub target: Option<PlaybackTarget>,
    pub episodes: Vec<Episode>,
    pub episode_index: usize,
    pub streams: Vec<StreamLink>,
    pub stream_index: usize,
    pub resolution_override: Option<Resolution>,
    pub attempts: String,
    pub player_status: String,
    pub player_running: bool,
    pub download: Option<DownloadState>,
    pub history: History,
    pub bookmarks: Bookmarks,
    pub log_buffer: Option<LogBuffer>,
    pub browse_tab: BrowseTab,
    pub browse_items: Vec<AnimeSearchResult>,
    pub browse_index: usize,
    pub history_index: usize,
    pub bookmark_index: usize,
    pub log_index: usize,
    pub settings_index: usize,
    pub data_dir: PathBuf,
    pub config_file: PathBuf,
    pub job: u64,
    pub quit: bool,
    pub tick: u64,
    pub mock_mode: bool,
}

impl App {
    pub fn new(
        config: Config,
        registry: Arc<SourceRegistry>,
        source_name: String,
        paths: Paths,
        log_buffer: Option<LogBuffer>,
    ) -> Self {
        let service = PlaybackService::new(registry.clone(), &config);
        let search_scope = config.general.search_scope;
        let language_filter = Language::from_code(&config.general.language_filter);
        let config_file = paths.config_file.clone();
        let data_dir = if config.storage.data_dir.trim().is_empty() {
            paths.data_dir.clone()
        } else {
            PathBuf::from(config.storage.data_dir.trim())
        };
        let history_path = data_dir.join("history.json");
        let bookmarks_path = data_dir.join("bookmarks.json");
        let history = History::load(history_path, config.storage.max_history);
        let bookmarks = Bookmarks::load(bookmarks_path, config.storage.max_bookmarks);
        Self {
            config,
            registry,
            service,
            screen: Screen::Search,
            previous_screen: None,
            source_name: source_name.clone(),
            search_scope,
            search_slots: Vec::new(),
            search_cancel: None,
            selector: None,
            language_filter,
            language_index: 0,
            language_popup: false,
            update_toast: None,
            update_version: None,
            status: format!("source: {source_name}"),
            loading: false,
            error: None,
            search_input: String::new(),
            input_focused: true,
            results: Vec::new(),
            result_index: 0,
            detail: None,
            target: None,
            episodes: Vec::new(),
            episode_index: 0,
            streams: Vec::new(),
            stream_index: 0,
            resolution_override: None,
            attempts: String::new(),
            player_status: String::new(),
            player_running: false,
            download: None,
            history,
            bookmarks,
            log_buffer,
            browse_tab: BrowseTab::Ongoing,
            browse_items: Vec::new(),
            browse_index: 0,
            history_index: 0,
            bookmark_index: 0,
            log_index: 0,
            settings_index: 0,
            data_dir,
            config_file,
            job: 0,
            quit: false,
            tick: 0,
            mock_mode: false,
        }
    }

    fn next_job(&mut self) -> u64 {
        self.job += 1;
        self.job
    }

    pub fn active_source(&self) -> Option<Arc<dyn crate::sources::AnimeSource>> {
        self.registry.get(&self.source_name)
    }

    fn fallback_order(&self) -> Vec<String> {
        let mut order: Vec<String> = self
            .config
            .playback_order()
            .iter()
            .map(|name| (*name).to_string())
            .collect();
        // Source yang sedang dipilih diutamakan: pada pencarian paralel, dedupe
        // "first wins" jadi menghormati pilihan user, bukan urutan config.
        if let Some(position) = order.iter().position(|name| name == &self.source_name) {
            if position > 0 {
                let name = order.remove(position);
                order.insert(0, name);
            }
        } else if !self.source_name.is_empty() {
            order.insert(0, self.source_name.clone());
        }
        // Filter bahasa (tab Bahasa / config): hanya source yang cocok.
        order.retain(|name| self.source_matches_language(name));
        order
    }

    fn preferred_resolution(&self) -> Resolution {
        self.resolution_override
            .unwrap_or(self.config.playback.default_resolution)
    }

    /// Cek update di background (non-blocking) dan tampilkan toast bila ada versi baru.
    ///
    /// Memakai cache di data dir supaya tidak memanggil GitHub API setiap start.
    pub fn spawn_update_check(&self, tx: &UnboundedSender<AppEvent>) {
        if !self.config.update.check_on_start {
            return;
        }
        let interval = self.config.update.check_interval_hours;
        let cached = crate::update::CheckCache::load(&self.data_dir);
        let cached_latest = cached
            .as_ref()
            .filter(|cache| cache.is_fresh(interval))
            .map(|cache| cache.latest.clone());

        let tx = tx.clone();
        let data_dir = self.data_dir.clone();
        let http_config = self.config.http.clone();
        let platform = crate::update::Platform::current();

        tokio::spawn(async move {
            let latest = match cached_latest {
                Some(version) => version,
                None => {
                    let Ok(http) = crate::http::HttpClient::new(&http_config) else {
                        return;
                    };
                    let Ok(release) = crate::update::latest_release(&http).await else {
                        return;
                    };
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|elapsed| elapsed.as_secs())
                        .unwrap_or(0);
                    crate::update::CheckCache {
                        checked_at: now,
                        latest: release.version.clone(),
                    }
                    .save(&data_dir);
                    release.version
                }
            };

            if !crate::update::is_newer(crate::VERSION, &latest) {
                return;
            }
            let tag = format!("v{latest}");
            let _ = tx.send(AppEvent::UpdateAvailable {
                version: latest,
                tag,
                url: "https://github.com/xiryuudev/anicli-rs/releases/latest".to_string(),
                hint: "anicli update".to_string(),
            });
            let _ = platform;
        });
    }

    /// Buka popup filter bahasa (tombol `Y`).
    pub fn open_language_popup(&mut self) {
        let options = crate::ui::selector::language_options();
        self.language_index = options
            .iter()
            .position(|(language, _)| *language == self.language_filter)
            .unwrap_or(0);
        self.language_popup = true;
        self.status = "filter bahasa: ↑/↓ pilih · Enter terapkan · Esc batal".to_string();
    }

    fn handle_language_key(&mut self, key: KeyEvent) {
        let options = crate::ui::selector::language_options();
        match key.code {
            KeyCode::Esc => {
                self.language_popup = false;
                self.status = format!("filter bahasa: {}", self.language_label());
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.language_index = self.language_index.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.language_index + 1 < options.len() {
                    self.language_index += 1;
                }
            }
            KeyCode::Enter => {
                if let Some((language, _)) = options.get(self.language_index) {
                    self.language_filter = *language;
                    self.language_popup = false;
                    self.config.general.language_filter = language
                        .map(|language| language.code().to_string())
                        .unwrap_or_else(|| "all".to_string());
                    let _ = self.config.save(&self.config_file());
                    // Pastikan source aktif masih cocok dengan filter.
                    if !self.source_matches_language(&self.source_name) {
                        if let Some(first) = self.sources_for_language().first().cloned() {
                            self.select_source(&first);
                        }
                    }
                }
                self.status = format!("filter bahasa: {}", self.language_label());
            }
            _ => {}
        }
    }

    fn config_file(&self) -> std::path::PathBuf {
        self.config_file.clone()
    }

    /// Label filter bahasa untuk UI.
    pub fn language_label(&self) -> String {
        match self.language_filter {
            Some(language) => format!("{} {}", language.tag(), language.label()),
            None => "semua bahasa".to_string(),
        }
    }

    fn source_matches_language(&self, name: &str) -> bool {
        let Some(source) = self.registry.get(name) else {
            return false;
        };
        self.language_filter
            .map(|language| source.language() == language)
            .unwrap_or(true)
    }

    /// Nama source terpasang yang cocok dengan filter bahasa (urut registry).
    pub fn sources_for_language(&self) -> Vec<String> {
        self.registry
            .names()
            .into_iter()
            .filter(|name| self.source_matches_language(name))
            .map(ToString::to_string)
            .collect()
    }

    /// Buka popup pemilih source (tombol `s`).
    pub fn open_source_selector(&mut self) {
        let mut entries: Vec<SourceEntry> = Vec::new();
        let language_of = |name: &str| {
            self.registry
                .get(name)
                .map(|source| source.language())
                .unwrap_or(Language::Indonesian)
        };
        for (name, source_config) in &self.config.sources {
            if !self.source_matches_language(name) {
                continue;
            }
            entries.push(SourceEntry {
                name: name.clone(),
                language: language_of(name),
                available: self.registry.get(name).is_some(),
                enabled: source_config.enabled,
            });
        }
        for name in self.registry.names() {
            if entries.iter().any(|entry| entry.name == name) || !self.source_matches_language(name)
            {
                continue;
            }
            entries.push(SourceEntry {
                name: name.to_string(),
                language: language_of(name),
                available: true,
                enabled: true,
            });
        }
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        let count = entries.len();
        self.selector = Some(SourceSelector::new(entries, &self.source_name));
        self.status = format!("pilih source ({count} tersedia) · Enter pilih · Esc batal");
    }

    /// Terapkan source terpilih dari popup.
    pub fn select_source(&mut self, name: &str) {
        if self.registry.get(name).is_none() {
            self.error = Some(format!(
                "source {name:?} tidak tersedia (disabled/gagal dimuat)"
            ));
            return;
        }
        self.source_name = name.to_string();
        self.status = format!("source: {name}");
        self.error = None;
        // Hasil lama berasal dari source lain; bersihkan supaya tidak membingungkan.
        self.results.clear();
        self.search_slots.clear();
        self.result_index = 0;
    }

    /// Batalkan pencarian streaming yang sedang berjalan.
    pub fn cancel_search(&mut self) {
        if let Some(handle) = self.search_cancel.take() {
            handle.abort();
        }
        self.loading = false;
        self.status = format!("pencarian dibatalkan ({} hasil)", self.results.len());
    }

    /// Ganti resolusi pilihan (cycle di antara yang tersedia).
    pub fn cycle_resolution(&mut self) {
        let available = resolve::available_resolutions(&self.streams);
        if available.is_empty() {
            self.status = "tidak ada resolusi yang diketahui".to_string();
            return;
        }
        let current = self.preferred_resolution();
        let position = available
            .iter()
            .position(|resolution| *resolution == current)
            .map(|index| (index + 1) % available.len())
            .unwrap_or(0);
        self.resolution_override = Some(available[position]);
        self.highlight_best_stream();
        self.status = format!("resolusi: {}", available[position]);
    }

    pub fn on_tick(&mut self) {
        self.tick = self.tick.wrapping_add(1);
        // Toast update hilang sendiri setelah ~45 detik agar tidak mengganggu.
        if self.update_toast.is_some() && self.tick.is_multiple_of(375) {
            self.update_toast = None;
        }
    }

    /// Tangani satu event keyboard; spawn job bila perlu.
    pub fn handle_input(&mut self, event: Event, tx: &UnboundedSender<AppEvent>) {
        let Event::Key(key) = event else {
            return;
        };
        if key.kind != crossterm::event::KeyEventKind::Press {
            return;
        }

        // Popup bahasa menangkap input selama terbuka.
        if self.language_popup {
            self.handle_language_key(key);
            return;
        }

        // Popup source selector menangkap semua input selama terbuka.
        if self.selector.is_some() {
            let action = self
                .selector
                .as_mut()
                .map(|selector| selector.handle_key(key))
                .unwrap_or(SelectorAction::Ignored);
            match action {
                SelectorAction::Close => self.selector = None,
                SelectorAction::Choose(name) => {
                    self.select_source(&name);
                    self.selector = None;
                }
                SelectorAction::Ignored => {}
            }
            return;
        }

        match key.code {
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.quit = true;
                return;
            }
            KeyCode::Char('q') if !self.is_typing() => {
                self.quit = true;
                return;
            }
            KeyCode::Char('?') if !self.is_typing() => {
                self.toggle_help();
                return;
            }
            KeyCode::Char('Y') if !self.is_typing() => {
                self.open_language_popup();
                return;
            }
            KeyCode::Char('s') if !self.is_typing() && key.modifiers.is_empty() => {
                self.open_source_selector();
                return;
            }
            KeyCode::Char('l') if !self.is_typing() => {
                self.open_browse(tx);
                return;
            }
            KeyCode::Char('H') if !self.is_typing() => {
                self.open_history();
                return;
            }
            KeyCode::Char('B') if !self.is_typing() => {
                self.open_bookmarks();
                return;
            }
            KeyCode::Char('L') if !self.is_typing() => {
                self.open_logs();
                return;
            }
            KeyCode::Char('g') if !self.is_typing() => {
                self.open_settings();
                return;
            }
            KeyCode::Esc => {
                if self.loading {
                    self.cancel_search();
                    return;
                }
                if self.screen == Screen::Help {
                    self.screen = self.previous_screen.unwrap_or(Screen::Search);
                } else if self.screen == Screen::Search && self.is_typing() {
                    self.input_focused = false;
                } else {
                    self.go_back();
                }
                return;
            }
            _ => {}
        }

        if self.screen == Screen::Help {
            self.screen = self.previous_screen.unwrap_or(Screen::Search);
            return;
        }

        match self.screen {
            Screen::Search => self.handle_search(key, tx),
            Screen::Detail => match key.code {
                KeyCode::Enter | KeyCode::Char('e') => self.open_episodes(tx),
                KeyCode::Char('b') => self.toggle_bookmark(),
                _ => {}
            },
            Screen::Episodes => match key.code {
                KeyCode::Up | KeyCode::Char('k') => {
                    self.episode_index = self.episode_index.saturating_sub(1);
                }
                KeyCode::Down | KeyCode::Char('j')
                    if self.episode_index + 1 < self.episodes.len() =>
                {
                    self.episode_index += 1;
                }
                KeyCode::Enter => self.open_streams(tx),
                _ => {}
            },
            Screen::Streams => match key.code {
                KeyCode::Up | KeyCode::Char('k') => {
                    self.stream_index = self.stream_index.saturating_sub(1);
                }
                KeyCode::Down | KeyCode::Char('j')
                    if self.stream_index + 1 < self.streams.len() =>
                {
                    self.stream_index += 1;
                }
                KeyCode::Char('r') => self.cycle_resolution(),
                KeyCode::Enter | KeyCode::Char('p') => self.play_selected(tx),
                _ => {}
            },
            Screen::Browse => match key.code {
                KeyCode::Up | KeyCode::Char('k') => {
                    self.browse_index = self.browse_index.saturating_sub(1);
                }
                KeyCode::Down | KeyCode::Char('j')
                    if self.browse_index + 1 < self.browse_items.len() =>
                {
                    self.browse_index += 1;
                }
                KeyCode::Tab | KeyCode::Char('t') => self.switch_browse_tab(tx),
                KeyCode::Enter => self.open_browse_detail(tx),
                _ => {}
            },
            Screen::History => match key.code {
                KeyCode::Up | KeyCode::Char('k') => {
                    self.history_index = self.history_index.saturating_sub(1);
                }
                KeyCode::Down | KeyCode::Char('j')
                    if self.history_index + 1 < self.history.entries().len() =>
                {
                    self.history_index += 1;
                }
                KeyCode::Enter => self.open_history_entry(tx),
                KeyCode::Char('d') => self.remove_history_entry(),
                _ => {}
            },
            Screen::Bookmarks => match key.code {
                KeyCode::Up | KeyCode::Char('k') => {
                    self.bookmark_index = self.bookmark_index.saturating_sub(1);
                }
                KeyCode::Down | KeyCode::Char('j')
                    if self.bookmark_index + 1 < self.bookmarks.entries().len() =>
                {
                    self.bookmark_index += 1;
                }
                KeyCode::Enter => self.open_bookmark_entry(tx),
                KeyCode::Char('d') => self.remove_bookmark_entry(),
                _ => {}
            },
            Screen::Logs => match key.code {
                KeyCode::Up | KeyCode::Char('k') => {
                    self.log_index = self.log_index.saturating_sub(1);
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    let total = self.log_buffer.as_ref().map(|b| b.len()).unwrap_or(0);
                    if self.log_index + 1 < total {
                        self.log_index += 1;
                    }
                }
                _ => {}
            },
            Screen::Settings => match key.code {
                KeyCode::Up | KeyCode::Char('k') => {
                    self.settings_index = self.settings_index.saturating_sub(1);
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    self.settings_index = (self.settings_index + 1).min(20);
                }
                _ => {}
            },
            Screen::Help => {}
        }
    }

    fn is_typing(&self) -> bool {
        self.screen == Screen::Search && self.input_focused
    }

    fn toggle_help(&mut self) {
        if self.screen == Screen::Help {
            self.screen = self.previous_screen.unwrap_or(Screen::Search);
        } else {
            self.previous_screen = Some(self.screen);
            self.screen = Screen::Help;
        }
    }

    fn go_back(&mut self) {
        self.screen = match self.screen {
            Screen::Streams => Screen::Episodes,
            Screen::Episodes => Screen::Detail,
            Screen::Detail | Screen::Browse => Screen::Search,
            Screen::History | Screen::Bookmarks | Screen::Logs | Screen::Settings => Screen::Search,
            Screen::Search | Screen::Help => Screen::Search,
        };
        self.error = None;
    }

    fn handle_search(&mut self, key: KeyEvent, tx: &UnboundedSender<AppEvent>) {
        if self.input_focused {
            match key.code {
                KeyCode::Enter => self.start_search(tx),
                KeyCode::Tab | KeyCode::Down => {
                    if !self.results.is_empty() {
                        self.input_focused = false;
                    }
                }
                KeyCode::Backspace => {
                    self.search_input.pop();
                }
                KeyCode::Char(ch)
                    if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT =>
                {
                    self.search_input.push(ch);
                }
                _ => {}
            }
            return;
        }

        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.result_index = self.result_index.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') if self.result_index + 1 < self.results.len() => {
                self.result_index += 1;
            }
            KeyCode::Tab | KeyCode::Char('/') => {
                self.input_focused = true;
            }
            KeyCode::Enter => self.open_detail(tx),
            KeyCode::Char('S') => self.toggle_search_scope(),
            _ => {}
        }
    }

    fn toggle_search_scope(&mut self) {
        self.search_scope = match self.search_scope {
            SearchScope::All => SearchScope::Selected,
            SearchScope::Selected => SearchScope::All,
        };
        self.status = format!(
            "cakupan pencarian: {} (tekan S untuk ganti)",
            self.search_scope
        );
    }

    fn start_search(&mut self, tx: &UnboundedSender<AppEvent>) {
        let query = self.search_input.trim().to_string();
        if query.is_empty() {
            self.status = "ketik judul dulu".to_string();
            return;
        }
        let Some(source) = self.active_source() else {
            self.error = Some(format!("source {:?} tidak terpasang", self.source_name));
            return;
        };
        let job = self.next_job();
        self.loading = true;
        self.error = None;
        self.results.clear();
        self.result_index = 0;
        self.input_focused = false;

        let order = if self.search_scope == SearchScope::Selected {
            vec![self.source_name.clone()]
        } else {
            self.fallback_order()
        };
        self.search_slots = vec![None; order.len()];
        self.status = format!(
            "mencari {:?} di {} ({})…",
            query,
            source.name(),
            self.search_scope
        );

        let service = self.service.clone();
        let tx = tx.clone();
        let coordinator = tokio::spawn(async move {
            let (local_tx, mut local_rx) = mpsc::unbounded_channel();
            let query = query.clone();
            let order = order.clone();
            let search = tokio::spawn(async move {
                let order_refs: Vec<&str> = order.iter().map(String::as_str).collect();
                service
                    .search_streaming(&query, &order_refs, local_tx)
                    .await;
            });

            while let Some(event) = local_rx.recv().await {
                match event {
                    crate::fallback::StreamSearchEvent::Source {
                        index,
                        source,
                        found,
                        error,
                    } => {
                        let _ = tx.send(AppEvent::SearchSource {
                            job,
                            index,
                            source,
                            items: found,
                            error: error.map(|error| error.user_message()),
                            sources_tried: 0,
                        });
                    }
                    crate::fallback::StreamSearchEvent::Done { sources_tried } => {
                        let _ = tx.send(AppEvent::SearchDone {
                            job,
                            sources_with_results: 0,
                            sources_tried,
                            errors: String::new(),
                        });
                        break;
                    }
                }
            }
            let _ = search.await;
        });
        self.search_cancel = Some(coordinator.abort_handle());
    }

    /// Gabungkan hasil per slot source (urutan prioritas), dedupe judul.
    fn rebuild_search_results(&mut self) {
        let mut merged: Vec<AnimeSearchResult> = Vec::new();
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        for slot in &self.search_slots {
            let Some(items) = slot else {
                continue;
            };
            for item in items {
                let key = crate::sources::util::normalize_title(&item.title);
                if key.is_empty() || !seen.insert(key) {
                    continue;
                }
                merged.push(item.clone());
            }
        }
        self.results = merged;
        if self.result_index >= self.results.len() {
            self.result_index = self.results.len().saturating_sub(1);
        }
    }

    fn search_breakdown(&self) -> String {
        let mut counts: Vec<(String, usize)> = Vec::new();
        for item in &self.results {
            let name = item.source.as_str().to_string();
            match counts.iter_mut().find(|(existing, _)| *existing == name) {
                Some((_, total)) => *total += 1,
                None => counts.push((name, 1)),
            }
        }
        if counts.is_empty() {
            return "belum ada hasil".to_string();
        }
        counts
            .iter()
            .map(|(name, total)| format!("{name} {total}"))
            .collect::<Vec<_>>()
            .join(" · ")
    }

    fn open_detail(&mut self, tx: &UnboundedSender<AppEvent>) {
        let Some(result) = self.results.get(self.result_index).cloned() else {
            return;
        };
        let Some(source) = self
            .registry
            .get(result.source.as_str())
            .or_else(|| self.active_source())
        else {
            return;
        };
        let job = self.next_job();
        self.loading = true;
        self.error = None;
        self.status = format!("membuka {} (dari {})…", result.title, source.name());

        let tx = tx.clone();
        let url = result.url.to_string();
        let site = source.name();
        tokio::spawn(async move {
            let event =
                match tokio::time::timeout(Duration::from_secs(25), source.detail(&url)).await {
                    Ok(Ok(detail)) => AppEvent::DetailOk { job, detail },
                    Ok(Err(error)) => AppEvent::Failed { job, error },
                    Err(_) => AppEvent::Failed {
                        job,
                        error: SourceError::Timeout {
                            site,
                            step: "detail",
                        },
                    },
                };
            let _ = tx.send(event);
        });
    }

    /// Urutan source untuk pencarian/fallback (source aktif di depan).
    pub fn search_order(&self) -> Vec<String> {
        self.fallback_order()
    }

    /// Target playback sesi ini (dipakai juga oleh test integrasi).
    pub fn current_target(&self) -> Option<PlaybackTarget> {
        self.build_target()
    }

    fn build_target(&self) -> Option<PlaybackTarget> {
        let detail = self.detail.as_ref()?;
        let episode = self.episodes.get(self.episode_index);
        // Origin HARUS source asal detail, bukan source tab: kalau user memilih hasil
        // dari source lain, fast-path fallback tidak boleh menembak source tab.
        let origin = detail.source.clone();
        Some(PlaybackTarget {
            anime_title: detail.title.clone(),
            alt_titles: detail.alt_titles.clone(),
            episode_number: episode.and_then(|episode| episode.number),
            episode_label: episode
                .map(|episode| episode.label_for_ui())
                .unwrap_or_default(),
            origin,
            anime_url: detail.url.clone(),
            episode_url: episode
                .map(|episode| episode.url.clone())
                .unwrap_or_else(|| detail.url.clone()),
        })
    }

    fn open_episodes(&mut self, tx: &UnboundedSender<AppEvent>) {
        if let Some(detail) = &self.detail {
            if !detail.episodes.is_empty() {
                self.episodes = detail.episodes.clone();
                self.episode_index = 0;
                self.screen = Screen::Episodes;
                self.status = format!("{} episode", self.episodes.len());
                return;
            }
        }
        let Some(target) = self.build_target() else {
            return;
        };
        let job = self.next_job();
        self.loading = true;
        self.error = None;
        self.status = "mengambil daftar episode (fallback)…".to_string();

        let service = self.service.clone();
        let order = self.fallback_order();
        let tx = tx.clone();
        tokio::spawn(async move {
            let order_refs: Vec<&str> = order.iter().map(String::as_str).collect();
            let event = match service.episodes(&target, &order_refs).await {
                Ok(resolved) => AppEvent::EpisodesOk {
                    job,
                    detail: resolved.detail,
                    episodes: resolved.episodes,
                    source: resolved.source,
                    attempts: attempt_summary(&resolved.attempts),
                },
                Err(error) => AppEvent::Failed {
                    job,
                    error: SourceError::app(error.user_message()),
                },
            };
            let _ = tx.send(event);
        });
    }

    fn open_streams(&mut self, tx: &UnboundedSender<AppEvent>) {
        let Some(episode) = self.episodes.get(self.episode_index).cloned() else {
            return;
        };
        let Some(mut target) = self.build_target() else {
            return;
        };
        target.episode_number = episode.number;
        target.episode_label = episode.label_for_ui();
        target.episode_url = episode.url.clone();

        let job = self.next_job();
        self.loading = true;
        self.error = None;
        self.attempts.clear();
        self.status = format!("mengambil stream {}…", episode.label_for_ui());

        let service = self.service.clone();
        let order = self.fallback_order();
        let tx = tx.clone();
        tokio::spawn(async move {
            let order_refs: Vec<&str> = order.iter().map(String::as_str).collect();
            let event = match service.streams(&target, &order_refs).await {
                Ok(resolved) => AppEvent::StreamsOk {
                    job,
                    links: resolved.links,
                    source: resolved.source,
                    attempts: attempt_summary(&resolved.attempts),
                },
                Err(error) => AppEvent::Failed {
                    job,
                    error: SourceError::app(error.user_message()),
                },
            };
            let _ = tx.send(event);
        });
    }

    fn open_browse(&mut self, tx: &UnboundedSender<AppEvent>) {
        self.browse_tab = BrowseTab::Ongoing;
        self.browse_items.clear();
        self.browse_index = 0;
        self.screen = Screen::Browse;
        self.load_browse(tx);
    }

    fn switch_browse_tab(&mut self, tx: &UnboundedSender<AppEvent>) {
        self.browse_tab = match self.browse_tab {
            BrowseTab::Ongoing => BrowseTab::Completed,
            BrowseTab::Completed => BrowseTab::Ongoing,
        };
        self.browse_items.clear();
        self.browse_index = 0;
        self.load_browse(tx);
    }

    fn load_browse(&mut self, tx: &UnboundedSender<AppEvent>) {
        let Some(source) = self.active_source() else {
            self.error = Some(format!("source {:?} tidak terpasang", self.source_name));
            return;
        };
        let job = self.next_job();
        self.loading = true;
        self.error = None;
        let tab = self.browse_tab;
        self.status = format!(
            "memuat {} dari {}…",
            match tab {
                BrowseTab::Ongoing => "ongoing",
                BrowseTab::Completed => "completed",
            },
            source.name()
        );
        let site = source.name();
        let tx = tx.clone();
        tokio::spawn(async move {
            let future = match tab {
                BrowseTab::Ongoing => source.ongoing(),
                BrowseTab::Completed => source.completed(),
            };
            let event = match tokio::time::timeout(Duration::from_secs(25), future).await {
                Ok(Ok(items)) => AppEvent::BrowseOk { job, tab, items },
                Ok(Err(error)) => AppEvent::Failed { job, error },
                Err(_) => AppEvent::Failed {
                    job,
                    error: SourceError::Timeout {
                        site,
                        step: "browse",
                    },
                },
            };
            let _ = tx.send(event);
        });
    }

    fn open_browse_detail(&mut self, tx: &UnboundedSender<AppEvent>) {
        let Some(item) = self.browse_items.get(self.browse_index).cloned() else {
            return;
        };
        self.results = vec![item];
        self.result_index = 0;
        self.open_detail(tx);
    }

    fn open_history(&mut self) {
        self.history_index = 0;
        self.screen = Screen::History;
        self.status = format!("{} entri riwayat", self.history.entries().len());
    }

    fn open_bookmarks(&mut self) {
        self.bookmark_index = 0;
        self.screen = Screen::Bookmarks;
        self.status = format!("{} bookmark", self.bookmarks.entries().len());
    }

    fn open_logs(&mut self) {
        self.log_index = self
            .log_buffer
            .as_ref()
            .map(|buffer| buffer.len().saturating_sub(1))
            .unwrap_or(0);
        self.screen = Screen::Logs;
        self.status = format!(
            "{} baris log",
            self.log_buffer
                .as_ref()
                .map(|buffer| buffer.len())
                .unwrap_or(0)
        );
    }

    fn open_settings(&mut self) {
        self.settings_index = 0;
        self.screen = Screen::Settings;
        self.status = "pengaturan (baca-saja)".to_string();
    }

    fn toggle_bookmark(&mut self) {
        let Some(detail) = &self.detail else {
            return;
        };
        let entry = BookmarkEntry {
            title: detail.title.clone(),
            source: detail.source.to_string(),
            url: detail.url.to_string(),
            poster: detail.poster.as_ref().map(ToString::to_string),
            added_at: chrono::Local::now(),
        };
        match self.bookmarks.toggle(entry) {
            Ok(true) => self.status = "bookmark ditambahkan (B untuk melihat)".to_string(),
            Ok(false) => self.status = "bookmark dihapus".to_string(),
            Err(err) => {
                tracing::warn!(target: "anicli_rs::bookmarks", %err, "gagal menyimpan bookmark");
                self.error = Some(format!("gagal menyimpan bookmark: {err}"));
            }
        }
    }

    fn remove_history_entry(&mut self) {
        if let Err(err) = self.history.remove(self.history_index) {
            tracing::warn!(target: "anicli_rs::history", %err, "gagal menghapus riwayat");
            self.error = Some(format!("gagal menghapus riwayat: {err}"));
            return;
        }
        if self.history_index >= self.history.entries().len() {
            self.history_index = self.history.entries().len().saturating_sub(1);
        }
        self.status = format!("{} entri riwayat", self.history.entries().len());
    }

    fn remove_bookmark_entry(&mut self) {
        let Some(entry) = self.bookmarks.entries().get(self.bookmark_index).cloned() else {
            return;
        };
        match self.bookmarks.remove(&entry.url) {
            Ok(_) => {
                if self.bookmark_index >= self.bookmarks.entries().len() {
                    self.bookmark_index = self.bookmarks.entries().len().saturating_sub(1);
                }
                self.status = format!("{} bookmark", self.bookmarks.entries().len());
            }
            Err(err) => {
                tracing::warn!(target: "anicli_rs::bookmarks", %err, "gagal menghapus bookmark");
                self.error = Some(format!("gagal menghapus bookmark: {err}"));
            }
        }
    }

    fn open_bookmark_entry(&mut self, tx: &UnboundedSender<AppEvent>) {
        let Some(entry) = self.bookmarks.entries().get(self.bookmark_index).cloned() else {
            return;
        };
        let Ok(url) = url::Url::parse(&entry.url) else {
            self.error = Some(format!("URL bookmark tidak valid: {}", entry.url));
            return;
        };
        self.results = vec![AnimeSearchResult {
            source: crate::model::SourceId::new(entry.source.clone()),
            title: entry.title.clone(),
            url,
            thumbnail: None,
            kind: None,
            status: None,
            score: None,
        }];
        self.result_index = 0;
        self.open_detail(tx);
    }

    fn open_history_entry(&mut self, tx: &UnboundedSender<AppEvent>) {
        let Some(entry) = self.history.entries().get(self.history_index).cloned() else {
            return;
        };
        let (Ok(anime_url), Ok(episode_url)) = (
            url::Url::parse(&entry.anime_url),
            url::Url::parse(&entry.episode_url),
        ) else {
            self.error = Some("riwayat ini tidak punya URL episode yang valid".to_string());
            return;
        };
        let target = PlaybackTarget {
            anime_title: entry.anime_title.clone(),
            alt_titles: Vec::new(),
            episode_number: entry.episode_number,
            episode_label: entry.episode_label.clone(),
            origin: crate::model::SourceId::new(entry.source.clone()),
            anime_url,
            episode_url,
        };
        // Detail sintetis supaya judul player & status tetap benar tanpa request detail.
        self.detail = Some(AnimeDetail {
            source: crate::model::SourceId::new(entry.source.clone()),
            url: target.anime_url.clone(),
            title: entry.anime_title.clone(),
            alt_titles: Vec::new(),
            synopsis: String::new(),
            genres: Vec::new(),
            kind: None,
            status: AiringStatus::Unknown,
            year: None,
            score: None,
            total_episodes: None,
            poster: None,
            episodes: Vec::new(),
        });
        self.episodes = vec![Episode {
            number: entry.episode_number,
            label: entry.episode_label.clone(),
            title: None,
            url: target.episode_url.clone(),
            released: None,
        }];
        self.episode_index = 0;
        self.start_streams_for(target, tx);
    }

    fn record_history(&mut self, link: &StreamLink) {
        if !self.config.storage.history_enabled {
            return;
        }
        let Some(detail) = &self.detail else {
            return;
        };
        let episode = self.episodes.get(self.episode_index);
        let entry = HistoryEntry {
            anime_title: detail.title.clone(),
            source: detail.source.to_string(),
            episode_number: episode.and_then(|episode| episode.number),
            episode_label: episode
                .map(|episode| episode.label_for_ui())
                .unwrap_or_default(),
            anime_url: detail.url.to_string(),
            episode_url: episode
                .map(|episode| episode.url.to_string())
                .unwrap_or_default(),
            resolution: Some(link.resolution_label()),
            played_at: chrono::Local::now(),
            position_secs: None,
        };
        if let Err(err) = self.history.record(entry) {
            tracing::warn!(target: "anicli_rs::history", %err, "gagal menyimpan riwayat");
            self.status.push_str(" · gagal simpan riwayat");
        }
    }

    fn start_streams_for(&mut self, target: PlaybackTarget, tx: &UnboundedSender<AppEvent>) {
        let job = self.next_job();
        self.loading = true;
        self.error = None;
        self.status = format!("mengambil stream {}…", target.episode_label);
        let service = self.service.clone();
        let order = self.fallback_order();
        let tx = tx.clone();
        tokio::spawn(async move {
            let order_refs: Vec<&str> = order.iter().map(String::as_str).collect();
            let event = match service.streams(&target, &order_refs).await {
                Ok(resolved) => AppEvent::StreamsOk {
                    job,
                    links: resolved.links,
                    source: resolved.source,
                    attempts: attempt_summary(&resolved.attempts),
                },
                Err(error) => AppEvent::Failed {
                    job,
                    error: SourceError::app(error.user_message()),
                },
            };
            let _ = tx.send(event);
        });
    }

    /// Link yang sedang di-highlight di layar Stream.
    pub fn selected_stream(&self) -> Option<&StreamLink> {
        self.streams.get(self.stream_index)
    }

    /// Indeks link terbaik sesuai preferensi resolusi (default highlight).
    fn best_stream_index(&self) -> Option<usize> {
        let best = resolve::select_stream(
            &self.streams,
            self.preferred_resolution(),
            self.config.playback.resolution_policy,
            self.config.playback.prefer_mkv,
        )?;
        self.streams
            .iter()
            .position(|link| std::ptr::eq(link, best))
    }

    /// Pindahkan highlight ke link terbaik pada resolusi aktif (dipakai `r` dan saat
    /// stream selesai dimuat).
    fn highlight_best_stream(&mut self) {
        if let Some(index) = self.best_stream_index() {
            self.stream_index = index;
        }
    }

    /// Mulai unduhan: file langsung diunduh di app (progress bar), halaman
    /// perantara dibuka di browser.
    fn start_download(&mut self, link: &StreamLink, tx: &UnboundedSender<AppEvent>) {
        if let Some(active) = &self.download {
            if !active.done && active.failed.is_none() {
                self.status = "unduhan masih berjalan — tunggu sampai selesai".to_string();
                return;
            }
        }
        let label = link.label.clone();
        let direct_url = crate::download::direct_file_url(&link.url);
        if direct_url.is_none() && !crate::download::is_direct_file(&link.url) {
            match crate::player::open_browser(link.url.as_str()) {
                Ok(()) => {
                    self.status = format!("link unduhan dibuka di browser: {label}");
                    self.error = None;
                }
                Err(err) => self.error = Some(err.to_string()),
            }
            return;
        }

        let title = self
            .detail
            .as_ref()
            .map(|detail| detail.title.clone())
            .unwrap_or_else(|| "anicli-rs".to_string());
        let episode_label = self
            .episodes
            .get(self.episode_index)
            .map(|episode| episode.label_for_ui())
            .unwrap_or_default();
        let ext = link
            .container
            .map(|container| format!("{container:?}").to_ascii_lowercase())
            .unwrap_or_else(|| "bin".to_string());
        let raw_name = format!("{title} - {episode_label} - {label}");
        let file_name = crate::download::safe_file_name(&raw_name, &ext);
        let dest = self
            .data_dir
            .join("downloads")
            .join(&self.source_name)
            .join(file_name);
        let spec = crate::download::DownloadSpec {
            url: direct_url.unwrap_or_else(|| link.url.clone()),
            dest: dest.clone(),
            referer: link.headers.referer.clone(),
            user_agent: link
                .headers
                .user_agent
                .clone()
                .or_else(|| Some(self.config.http.user_agent.clone())),
        };
        self.download = Some(DownloadState {
            path: dest.clone(),
            ..DownloadState::default()
        });
        self.status = format!("mengunduh {}", crate::download::display_path(&dest));
        self.error = None;
        let job = self.job;
        let tx = tx.clone();
        tokio::spawn(crate::download::run(job, spec, tx));
    }

    /// Pilih link terbaik lalu putar (mpv/vlc) atau buka browser untuk embed.
    fn play_selected(&mut self, tx: &UnboundedSender<AppEvent>) {
        if self.player_running {
            self.status = "player masih berjalan — tutup jendela mpv dulu".to_string();
            return;
        }
        // Pakai BARIS yang di-highlight. Sebelumnya selalu memilih link "terbaik",
        // sehingga memilih baris download tetap memutar link Direct lain.
        let Some(link) = self.streams.get(self.stream_index).cloned() else {
            self.error = Some("Belum ada link. Ambil stream dulu.".to_string());
            return;
        };
        if link.stream_kind == StreamKind::DownloadOnly {
            self.start_download(&link, tx);
            return;
        }
        let title = self
            .detail
            .as_ref()
            .map(|detail| detail.title.clone())
            .unwrap_or_else(|| "anicli-rs".to_string());
        let episode_label = self
            .episodes
            .get(self.episode_index)
            .map(|episode| episode.label_for_ui())
            .unwrap_or_default();
        let media_title = if episode_label.is_empty() {
            title
        } else {
            format!("{title} — {episode_label}")
        };

        let path_env = std::env::var_os("PATH");
        let player_path = player::find_player(
            self.config.playback.player,
            match self.config.playback.player {
                crate::config::Player::Mpv => &self.config.playback.mpv_path,
                crate::config::Player::Vlc => &self.config.playback.vlc_path,
            },
            path_env.as_deref(),
        );
        let ytdlp = player::find_binary(player::YTDLP_BIN, "", path_env.as_deref());
        let plan =
            player::plan_playback(&self.config, &link, &media_title, None, player_path, ytdlp);

        match plan {
            PlaybackPlan::Android {
                url,
                mime,
                prefer,
                headers,
            } => {
                let job = self.next_job();
                self.error = None;
                self.player_status = format!("membuka player Android ({prefer})");
                self.status = format!(
                    "{} · buka player Android ({prefer}) {}",
                    media_title,
                    link.resolution_label()
                );
                self.record_history(&link);
                let tx = tx.clone();
                tokio::spawn(async move {
                    let result =
                        crate::player::android::launch(&url, &mime, prefer, &headers).await;
                    match result {
                        Ok(program) => {
                            let _ = tx.send(AppEvent::PlayerLaunched { program });
                        }
                        Err(message) => {
                            let _ = tx.send(AppEvent::Failed {
                                job,
                                error: crate::error::SourceError::app(message),
                            });
                        }
                    }
                });
            }
            PlaybackPlan::Spawn(command) => {
                let job = self.next_job();
                self.error = None;
                self.player_status = format!("memutar via {}", command.program.display());
                self.status = format!(
                    "{} · putar {} {}",
                    media_title,
                    link.resolution_label(),
                    link.server
                );
                self.record_history(&link);
                tracing::info!(target: "anicli_rs::player", command = %command.display(), "menjalankan player");
                let tx = tx.clone();
                let command_for_log = command.clone();
                let foreground = self.config.playback.foreground;
                let link_for_preflight = link.clone();
                tokio::spawn(async move {
                    // Deteksi takedown/host menolak sebelum mpv dijalankan, supaya
                    // pesan errornya jelas (bukan cuma "exit 2").
                    if let Some(message) = player::preflight_direct(&link_for_preflight).await {
                        let _ = tx.send(AppEvent::Failed {
                            job,
                            error: crate::error::SourceError::app(message),
                        });
                        return;
                    }
                    match player::spawn_player(&command_for_log, foreground).await {
                        Ok(child) => {
                            let _ = tx.send(AppEvent::PlayerStarted {
                                job,
                                program: command_for_log.program.display().to_string(),
                            });
                            let (code, stderr) = player::wait_with_stderr(child).await;
                            let _ = tx.send(AppEvent::PlayerExited {
                                program: command_for_log.program.display().to_string(),
                                code,
                                stderr,
                            });
                        }
                        Err(error) => {
                            let _ = tx.send(AppEvent::Failed { job, error });
                        }
                    }
                });
            }
            PlaybackPlan::Browser { url, reason } => match player::open_browser(&url) {
                Ok(()) => {
                    self.error = None;
                    self.status = reason;
                }
                Err(error) => self.error = Some(error.user_message()),
            },
            PlaybackPlan::Refuse(reason) => {
                self.error = Some(reason);
            }
        }
    }

    /// Terapkan hasil job (abaikan hasil job basi).
    pub fn handle_event(&mut self, event: AppEvent) {
        match event {
            AppEvent::SearchSource {
                job,
                index,
                source,
                items,
                error,
                sources_tried,
            } => {
                if job != self.job {
                    return;
                }
                let _ = sources_tried;
                if let Some(slot) = self.search_slots.get_mut(index) {
                    *slot = Some(items);
                }
                if let Some(message) = error {
                    if !self.attempts.contains(&source) {
                        let line = format!("{source} ✗ {message}");
                        self.attempts = if self.attempts.is_empty() {
                            line
                        } else {
                            format!("{} · {line}", self.attempts)
                        };
                    }
                }
                self.rebuild_search_results();
                let responded = self
                    .search_slots
                    .iter()
                    .filter(|slot| slot.is_some())
                    .count();
                self.status = format!(
                    "{}/{} hasil ({} dari {} source merespons)…",
                    self.results.len(),
                    self.results.len(),
                    responded,
                    self.search_slots.len()
                );
                self.screen = Screen::Search;
            }
            AppEvent::SearchDone {
                job,
                sources_tried,
                sources_with_results,
                errors,
            } => {
                if job != self.job {
                    return;
                }
                let _ = (sources_tried, sources_with_results);
                self.loading = false;
                self.search_cancel = None;
                if self.results.is_empty() {
                    self.input_focused = true;
                }
                if !errors.is_empty() {
                    self.attempts = errors;
                }
                let breakdown = self.search_breakdown();
                let count = self.results.len();
                let errored = self
                    .attempts
                    .split(" · ")
                    .filter(|part| part.contains('✗'))
                    .count();
                let scope_note = if self.search_scope == SearchScope::Selected {
                    format!("{}", self.search_scope)
                } else {
                    format!(
                        "{}/{} source",
                        self.search_slots.iter().filter(|s| s.is_some()).count(),
                        self.search_slots.len()
                    )
                };
                self.status = format!("{count} hasil ({scope_note}) · {breakdown}");
                if errored > 0 {
                    self.status.push_str(&format!(" · {errored} source error"));
                }
                if count == 0 {
                    self.error = Some(
                        "tidak ada hasil. Coba kata kunci lain, atau tekan S untuk cari di semua source."
                            .to_string(),
                    );
                }
            }
            AppEvent::DetailOk { job, detail } => {
                if job != self.job {
                    return;
                }
                self.loading = false;
                self.status = format!("{} — {} episode", detail.title, detail.episodes.len());
                self.detail = Some(detail);
                self.target = self.build_target();
                self.screen = Screen::Detail;
            }
            AppEvent::EpisodesOk {
                job,
                detail,
                episodes,
                source,
                attempts,
            } => {
                if job != self.job {
                    return;
                }
                self.loading = false;
                let count = episodes.len();
                self.status = if attempts.is_empty() {
                    format!("{count} episode")
                } else {
                    format!("{count} episode via {source} · {attempts}")
                };
                self.attempts = attempts;
                self.detail = Some(detail);
                self.episodes = episodes;
                self.episode_index = 0;
                self.target = self.build_target();
                self.screen = Screen::Episodes;
            }
            AppEvent::StreamsOk {
                job,
                links,
                source,
                attempts,
            } => {
                if job != self.job {
                    return;
                }
                self.loading = false;
                let count = links.len();
                let direct = links
                    .iter()
                    .filter(|link| link.stream_kind == StreamKind::Direct)
                    .count();
                self.status = if attempts.is_empty() {
                    format!("{count} link via {source} ({direct} siap mpv)")
                } else {
                    format!("{count} link via {source} · {attempts}")
                };
                self.attempts = attempts;
                self.streams = links;
                self.stream_index = 0;
                self.resolution_override = None;
                self.highlight_best_stream();
                self.screen = Screen::Streams;
            }
            AppEvent::BrowseOk { job, tab, items } => {
                if job != self.job {
                    return;
                }
                self.loading = false;
                let count = items.len();
                self.status = format!(
                    "{count} anime {} dari {}",
                    match tab {
                        BrowseTab::Ongoing => "ongoing",
                        BrowseTab::Completed => "completed",
                    },
                    self.source_name
                );
                self.browse_tab = tab;
                self.browse_items = items;
                self.browse_index = 0;
                self.screen = Screen::Browse;
            }
            AppEvent::DownloadStarted { path, total, .. } => {
                let state = self.download.get_or_insert_with(DownloadState::default);
                state.path = path.clone();
                state.total = total;
                self.status = format!(
                    "mengunduh {} ({})",
                    crate::download::display_path(&path),
                    total
                        .map(crate::download::format_bytes)
                        .unwrap_or_else(|| "ukuran tidak diketahui".to_string())
                );
            }
            AppEvent::DownloadProgress {
                downloaded, total, ..
            } => {
                if let Some(state) = self.download.as_mut() {
                    state.downloaded = downloaded;
                    state.total = total.or(state.total);
                    self.status = format!(
                        "mengunduh {}% ({})",
                        state.percent(),
                        total
                            .map(crate::download::format_bytes)
                            .unwrap_or_else(|| crate::download::format_bytes(downloaded))
                    );
                }
            }
            AppEvent::DownloadDone { path, .. } => {
                if let Some(state) = self.download.as_mut() {
                    state.done = true;
                    state.path = path.clone();
                }
                self.status = format!(
                    "✓ unduhan selesai: {}",
                    crate::download::display_path(&path)
                );
                self.error = None;
            }
            AppEvent::DownloadFailed { message, .. } => {
                if let Some(state) = self.download.as_mut() {
                    state.failed = Some(message.clone());
                }
                self.status = "unduhan gagal".to_string();
                self.error = Some(format!("unduhan gagal: {message}"));
            }
            AppEvent::UpdateAvailable {
                version,
                tag: _tag,
                url: _url,
                hint,
            } => {
                self.update_version = Some(version.clone());
                self.update_toast = Some(format!("versi {version} tersedia — jalankan `{hint}`"));
            }
            AppEvent::PlayerLaunched { program } => {
                self.player_running = false;
                self.error = None;
                self.player_status = format!("dibuka di {program}");
                self.status = format!(
                    "▶ dibuka di {program} — putar di aplikasi Android; kembali ke anicli kapan saja"
                );
            }
            AppEvent::PlayerStarted { job, program } => {
                if job != self.job {
                    return;
                }
                self.player_running = true;
                self.player_status = format!("diputar oleh {program}");
                self.status = format!("▶ {program} berjalan — tutup jendelanya untuk memutar lagi");
            }
            AppEvent::PlayerExited {
                program,
                code,
                stderr,
            } => {
                self.player_running = false;
                if code == Some(0) {
                    self.player_status = format!("{program} selesai");
                    self.status = format!("{program} selesai diputar");
                    self.error = None;
                } else {
                    let detail = player::summarize_stderr(&stderr, 260);
                    let exit = code
                        .map(|code| format!("exit {code}"))
                        .unwrap_or_else(|| "dihentikan".to_string());
                    self.player_status = format!("{program} gagal ({exit})");
                    self.status = format!("{program} gagal diputar ({exit})");
                    self.error = Some(format!(
                        "{program} gagal diputar ({exit}): {detail}. \
                         Jika ini link embed, coba `playback.embed_strategy = \"browser\"` \
                         atau pilih link direct."
                    ));
                }
            }
            AppEvent::Failed { job, error } => {
                if job != self.job {
                    return;
                }
                self.loading = false;
                self.status = "gagal".to_string();
                self.error = Some(error.user_message());
            }
        }
    }
}

/// Ringkas jejak fallback untuk status bar.
fn attempt_summary(attempts: &[Attempt]) -> String {
    // "tidak didukung" = source katalog (tanpa player); jangan memenuhi pesan
    // error kalau ada percobaan lain yang lebih informatif.
    let informative: Vec<&Attempt> = attempts
        .iter()
        .filter(|attempt| {
            !matches!(
                attempt.outcome,
                crate::fallback::AttemptOutcome::Unsupported
            )
        })
        .collect();
    let chosen: Vec<&Attempt> = if informative.is_empty() {
        attempts.iter().collect()
    } else {
        informative
    };
    if chosen.len() <= 1 && attempts.len() <= 1 {
        return String::new();
    }
    chosen
        .iter()
        .map(|attempt| format!("{} {}", attempt.source, attempt.outcome.label()))
        .collect::<Vec<_>>()
        .join(" · ")
}

/// Jalankan TUI sampai user keluar.
pub async fn run(app: App) -> Result<()> {
    let terminal = ratatui::init();
    let result = event_loop(terminal, app).await;
    ratatui::restore();
    result
}

async fn event_loop(
    mut terminal: Terminal<CrosstermBackend<std::io::Stdout>>,
    mut app: App,
) -> Result<()> {
    let mut events = EventStream::new();
    let (tx, mut rx) = mpsc::unbounded_channel::<AppEvent>();
    let mut ticker = tokio::time::interval(Duration::from_millis(120));
    // Cek update sekali di latar; hasilnya muncul sebagai toast di footer.
    app.spawn_update_check(&tx);

    loop {
        terminal
            .draw(|frame| crate::ui::render::render(frame, &app))
            .context("menggambar frame TUI")?;

        tokio::select! {
            maybe_event = events.next() => {
                if let Some(Ok(event)) = maybe_event {
                    app.handle_input(event, &tx);
                }
            }
            Some(message) = rx.recv() => {
                app.handle_event(message);
            }
            _ = ticker.tick() => {
                app.on_tick();
            }
        }

        if app.quit {
            return Ok(());
        }
    }
}
