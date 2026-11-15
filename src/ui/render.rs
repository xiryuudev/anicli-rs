//! Fungsi render murni (`&App` → frame). Dites dengan `TestBackend` di `tests/tui_render.rs`.

use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap};
use ratatui::Frame;

use crate::model::StreamKind;
use crate::ui::app::{App, BrowseTab, Screen};
use crate::ui::theme;

const SPINNER: [&str; 4] = ["|", "/", "-", "\\"];

pub fn render(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let footer_height = if app.error.is_some() { 3 } else { 2 };
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(footer_height),
        ])
        .split(area);

    render_header(frame, app, chunks[0]);

    match app.screen {
        Screen::Search => render_search(frame, app, chunks[1]),
        Screen::Detail => render_detail(frame, app, chunks[1]),
        Screen::Episodes => render_episodes(frame, app, chunks[1]),
        Screen::Streams => render_streams(frame, app, chunks[1]),
        Screen::Browse => render_browse(frame, app, chunks[1]),
        Screen::History => render_history(frame, app, chunks[1]),
        Screen::Bookmarks => render_bookmarks(frame, app, chunks[1]),
        Screen::Logs => render_logs(frame, app, chunks[1]),
        Screen::Settings => render_settings(frame, app, chunks[1]),
        Screen::Help => render_help(frame, area),
    }

    render_footer(frame, app, chunks[2]);
}

fn render_header(frame: &mut Frame, app: &App, area: Rect) {
    let loading = if app.loading {
        format!(" {} ", SPINNER[(app.tick as usize) % SPINNER.len()])
    } else {
        String::new()
    };
    let title = Line::from(vec![
        Span::styled(" anicli-rs ", theme::title()),
        Span::styled(format!("v{}", crate::VERSION), theme::dim()),
        Span::raw("  ·  "),
        Span::styled(app.source_name.clone(), theme::accent()),
        Span::styled(loading, theme::warning()),
    ]);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::dim())
        .title(title);
    let tabs = Line::from(vec![
        tab("cari", app.screen == Screen::Search),
        Span::raw(" "),
        tab("detail", app.screen == Screen::Detail),
        Span::raw(" "),
        tab("episode", app.screen == Screen::Episodes),
        Span::raw(" "),
        tab("stream", app.screen == Screen::Streams),
    ]);
    frame.render_widget(Paragraph::new(tabs).block(block), area);
}

fn tab(label: &str, active: bool) -> Span<'static> {
    let text = format!(" {label} ");
    if active {
        Span::styled(text, theme::highlight())
    } else {
        Span::styled(text, theme::dim())
    }
}

fn render_search(frame: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(3)])
        .split(area);

    let cursor = if app.input_focused { "█" } else { "" };
    let input_line = Line::from(vec![
        Span::styled("> ", theme::accent()),
        Span::raw(app.search_input.clone()),
        Span::styled(cursor, theme::accent()),
    ]);
    let input_style = if app.input_focused {
        theme::base()
    } else {
        theme::dim()
    };
    let input_title = if app.input_focused {
        " Cari anime — Enter: cari · Tab: ke hasil · Esc: lepas fokus "
    } else {
        " Cari anime — / atau Tab: kembali ke input "
    };
    frame.render_widget(
        Paragraph::new(input_line)
            .style(input_style)
            .block(Block::default().borders(Borders::ALL).title(input_title)),
        chunks[0],
    );

    if app.results.is_empty() {
        let message = if app.loading {
            "mencari…"
        } else if app.search_input.trim().is_empty() {
            "Ketik judul lalu tekan Enter. Contoh: one piece, kaguya, frieren."
        } else {
            "Belum ada hasil. Tekan Enter untuk mencari."
        };
        frame.render_widget(
            Paragraph::new(message)
                .style(theme::dim())
                .block(Block::default().borders(Borders::ALL).title(" Hasil "))
                .wrap(Wrap { trim: true }),
            chunks[1],
        );
        return;
    }

    let items: Vec<ListItem<'_>> = app
        .results
        .iter()
        .map(|result| {
            let mut spans = vec![Span::raw(result.title.clone())];
            if let Some(status) = result.status {
                spans.push(Span::styled(format!("  [{status:?}]"), theme::dim()));
            }
            if let Some(score) = result.score {
                spans.push(Span::styled(format!("  ★ {score:.2}"), theme::warning()));
            }
            ListItem::new(Line::from(spans))
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(" Hasil ({}) ", app.results.len())),
        )
        .highlight_style(theme::highlight())
        .highlight_symbol("▶ ");
    frame.render_stateful_widget(
        list,
        chunks[1],
        &mut ratatui::widgets::ListState::default().with_selected(Some(app.result_index)),
    );
}

fn render_detail(frame: &mut Frame, app: &App, area: Rect) {
    let Some(detail) = &app.detail else {
        frame.render_widget(
            Paragraph::new("Belum ada detail. Tekan Esc untuk kembali.")
                .block(Block::default().borders(Borders::ALL).title(" Detail ")),
            area,
        );
        return;
    };

    let mut lines: Vec<Line<'_>> = Vec::new();
    lines.push(Line::from(Span::styled(
        detail.title.clone(),
        theme::title(),
    )));
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("Status: ", theme::dim()),
        Span::raw(format!("{:?}", detail.status)),
        Span::raw("   "),
        Span::styled("Tahun: ", theme::dim()),
        Span::raw(match detail.year {
            Some(year) => year.to_string(),
            None => "?".to_string(),
        }),
        Span::raw("   "),
        Span::styled("Skor: ", theme::dim()),
        Span::raw(match detail.score {
            Some(score) => format!("{score:.2}"),
            None => "?".to_string(),
        }),
        Span::raw("   "),
        Span::styled("Episode: ", theme::dim()),
        Span::raw(detail.episodes.len().to_string()),
    ]));
    lines.push(Line::from(vec![
        Span::styled("Genre: ", theme::dim()),
        Span::raw(if detail.genres.is_empty() {
            "-".to_string()
        } else {
            detail.genres.join(", ")
        }),
    ]));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled("Sinopsis", theme::accent())));
    lines.push(Line::from(detail.synopsis.clone()));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Enter: lihat daftar episode · Esc: kembali",
        theme::success(),
    )));

    frame.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Detail Anime "),
            )
            .wrap(Wrap { trim: true }),
        area,
    );
}

fn render_episodes(frame: &mut Frame, app: &App, area: Rect) {
    if app.episodes.is_empty() {
        frame.render_widget(
            Paragraph::new("Tidak ada episode (source ini mungkin download-only).")
                .block(Block::default().borders(Borders::ALL).title(" Episode ")),
            area,
        );
        return;
    }

    let items: Vec<ListItem<'_>> = app
        .episodes
        .iter()
        .map(|episode| {
            ListItem::new(Line::from(vec![
                Span::raw(episode.label_for_ui()),
                Span::styled(format!("  {}", episode.url.path()), theme::dim()),
            ]))
        })
        .collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(format!(
            " Episode ({}) — Enter: stream ",
            app.episodes.len()
        )))
        .highlight_style(theme::highlight())
        .highlight_symbol("▶ ");
    frame.render_stateful_widget(
        list,
        area,
        &mut ratatui::widgets::ListState::default().with_selected(Some(app.episode_index)),
    );
}

fn render_streams(frame: &mut Frame, app: &App, area: Rect) {
    if app.streams.is_empty() {
        frame.render_widget(
            Paragraph::new("Belum ada stream. Tekan Esc untuk kembali.")
                .block(Block::default().borders(Borders::ALL).title(" Stream ")),
            area,
        );
        return;
    }

    let items: Vec<ListItem<'_>> = app
        .streams
        .iter()
        .map(|link| {
            let kind_style = match link.stream_kind {
                StreamKind::Direct => theme::success(),
                StreamKind::Embed => theme::warning(),
                StreamKind::DownloadOnly => theme::dim(),
            };
            ListItem::new(Line::from(vec![
                Span::styled(format!("[{}]", link.resolution_label()), theme::accent()),
                Span::raw(" "),
                Span::styled(format!("{:8}", link.stream_kind_label()), kind_style),
                Span::raw(" "),
                Span::raw(format!("{:12}", link.server.to_string())),
                Span::raw(" "),
                Span::raw(link.label.clone()),
            ]))
        })
        .collect();

    let direct = app
        .streams
        .iter()
        .filter(|link| link.stream_kind == StreamKind::Direct)
        .count();
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(format!(
            " Stream ({}) — {direct} siap mpv · pemutar di Tahap 8 ",
            app.streams.len()
        )))
        .highlight_style(theme::highlight())
        .highlight_symbol("▶ ");
    frame.render_stateful_widget(
        list,
        area,
        &mut ratatui::widgets::ListState::default().with_selected(Some(app.stream_index)),
    );
}

fn render_browse(frame: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(3)])
        .split(area);

    let tabs = Line::from(vec![
        Span::styled(
            " ongoing ",
            if app.browse_tab == BrowseTab::Ongoing {
                theme::highlight()
            } else {
                theme::dim()
            },
        ),
        Span::raw(" "),
        Span::styled(
            " completed ",
            if app.browse_tab == BrowseTab::Completed {
                theme::highlight()
            } else {
                theme::dim()
            },
        ),
        Span::styled("  (Tab: ganti tab)", theme::dim()),
    ]);
    frame.render_widget(Paragraph::new(tabs), chunks[0]);

    if app.browse_items.is_empty() {
        frame.render_widget(
            Paragraph::new(if app.loading {
                "memuat daftar…"
            } else {
                "Tidak ada item. Tekan Tab untuk ganti tab atau Esc untuk kembali."
            })
            .style(theme::dim())
            .block(Block::default().borders(Borders::ALL).title(" Browse ")),
            chunks[1],
        );
        return;
    }

    let items: Vec<ListItem<'_>> = app
        .browse_items
        .iter()
        .map(|item| {
            let mut spans = vec![Span::raw(item.title.clone())];
            if let Some(status) = item.status {
                spans.push(Span::styled(format!("  [{status:?}]"), theme::dim()));
            }
            ListItem::new(Line::from(spans))
        })
        .collect();
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(" Browse — {} item ", app.browse_items.len())),
        )
        .highlight_style(theme::highlight())
        .highlight_symbol("▶ ");
    frame.render_stateful_widget(
        list,
        chunks[1],
        &mut ratatui::widgets::ListState::default().with_selected(Some(app.browse_index)),
    );
}

fn render_history(frame: &mut Frame, app: &App, area: Rect) {
    if app.history.is_empty() {
        frame.render_widget(
            Paragraph::new("Belum ada riwayat. Putar satu episode dulu.")
                .block(Block::default().borders(Borders::ALL).title(" Riwayat ")),
            area,
        );
        return;
    }
    let items: Vec<ListItem<'_>> = app
        .history
        .entries()
        .iter()
        .map(|entry| {
            ListItem::new(Line::from(vec![
                Span::raw(entry.display()),
                Span::styled(
                    format!("  {}", entry.played_at.format("%H:%M %d/%m")),
                    theme::dim(),
                ),
                Span::styled(
                    match &entry.resolution {
                        Some(resolution) => format!("  [{resolution}]"),
                        None => String::new(),
                    },
                    theme::accent(),
                ),
            ]))
        })
        .collect();
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(format!(
            " Riwayat ({}) — Enter: putar ulang · d: hapus ",
            app.history.entries().len()
        )))
        .highlight_style(theme::highlight())
        .highlight_symbol("▶ ");
    frame.render_stateful_widget(
        list,
        area,
        &mut ratatui::widgets::ListState::default().with_selected(Some(app.history_index)),
    );
}

fn render_bookmarks(frame: &mut Frame, app: &App, area: Rect) {
    if app.bookmarks.is_empty() {
        frame.render_widget(
            Paragraph::new("Belum ada bookmark. Tekan b di layar detail untuk menambah.")
                .block(Block::default().borders(Borders::ALL).title(" Bookmark ")),
            area,
        );
        return;
    }
    let items: Vec<ListItem<'_>> = app
        .bookmarks
        .entries()
        .iter()
        .map(|entry| {
            ListItem::new(Line::from(vec![
                Span::raw(entry.title.clone()),
                Span::styled(format!("  ({})", entry.source), theme::dim()),
            ]))
        })
        .collect();
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(format!(
            " Bookmark ({}) — Enter: buka · d: hapus ",
            app.bookmarks.entries().len()
        )))
        .highlight_style(theme::highlight())
        .highlight_symbol("▶ ");
    frame.render_stateful_widget(
        list,
        area,
        &mut ratatui::widgets::ListState::default().with_selected(Some(app.bookmark_index)),
    );
}

fn render_logs(frame: &mut Frame, app: &App, area: Rect) {
    let lines: Vec<Line<'_>> = match &app.log_buffer {
        Some(buffer) => buffer
            .snapshot()
            .into_iter()
            .map(|line| Line::from(line.format()))
            .collect(),
        None => Vec::new(),
    };
    if lines.is_empty() {
        frame.render_widget(
            Paragraph::new("Belum ada log.")
                .block(Block::default().borders(Borders::ALL).title(" Log ")),
            area,
        );
        return;
    }

    let visible = area.height.saturating_sub(2) as usize;
    let len = lines.len();
    let end = (app.log_index + 1).min(len);
    let start = end.saturating_sub(visible);
    let shown: Vec<Line<'_>> = lines[start..end].to_vec();

    frame.render_widget(
        Paragraph::new(shown).block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(" Log ({len} baris) — j/k scroll ")),
        ),
        area,
    );
}

fn render_settings(frame: &mut Frame, app: &App, area: Rect) {
    let config = &app.config;
    let lines = vec![
        Line::from(Span::styled("Pengaturan (baca-saja)", theme::title())),
        Line::from(""),
        Line::from(format!(
            "config    : {}",
            app.data_dir.join("config.toml").display()
        )),
        Line::from(format!("data      : {}", app.data_dir.display())),
        Line::from(""),
        Line::from(format!("source    : {}", config.general.default_source)),
        Line::from(format!(
            "fallback  : {}",
            config.playback_order().join(" -> ")
        )),
        Line::from(format!(
            "resolusi  : {} ({})",
            config.playback.default_resolution, config.playback.resolution_policy
        )),
        Line::from(format!("player    : {}", config.playback.player)),
        Line::from(format!("embed     : {}", config.playback.embed_strategy)),
        Line::from(format!(
            "http      : delay {} ms, timeout {} s, max {} paralel, retry {}",
            config.http.request_delay_ms,
            config.http.timeout_secs,
            config.http.max_concurrent,
            config.http.retries
        )),
        Line::from(""),
        Line::from(Span::styled("sources:", theme::accent())),
    ]
    .into_iter()
    .chain(config.sources.iter().map(|(name, source)| {
        Line::from(format!(
            "  [{:8}] {name:12} {}",
            if source.enabled { "aktif" } else { "nonaktif" },
            source.base_url
        ))
    }))
    .collect::<Vec<_>>();

    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::default().borders(Borders::ALL).title(" Setting "))
            .wrap(Wrap { trim: true }),
        area,
    );
}

fn render_help(frame: &mut Frame, area: Rect) {
    let width = area.width.min(64);
    let height = area.height.min(16);
    let popup = Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    };
    frame.render_widget(Clear, popup);

    let lines = vec![
        Line::from(Span::styled("anicli-rs — bantuan", theme::title())),
        Line::from(""),
        Line::from("  /  atau Tab    fokus input pencarian"),
        Line::from("  Enter          cari / buka detail / pilih"),
        Line::from("  j k / ↑ ↓      navigasi daftar"),
        Line::from("  s              ganti source"),
        Line::from("  Esc            kembali / lepas fokus"),
        Line::from("  q              keluar (saat tidak mengetik)"),
        Line::from("  Ctrl+C         keluar kapan saja"),
        Line::from("  ?              tutup bantuan"),
        Line::from(""),
        Line::from(Span::styled(
            "  Sumber konten milik masing-masing penyedia — untuk penggunaan pribadi.",
            theme::dim(),
        )),
    ];
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(theme::accent())
                .title(" Bantuan "),
        ),
        popup,
    );
}

fn render_footer(frame: &mut Frame, app: &App, area: Rect) {
    let mut lines = vec![Line::from(vec![
        Span::styled(format!(" {} ", app.status), theme::base()),
        Span::raw("  "),
        Span::styled(
            "p putar · r resolusi · l browse · H riwayat · B bookmark · L log · g setting · ? bantuan",
            theme::dim(),
        ),
    ])];
    if let Some(download) = &app.download {
        let (text, style) = if let Some(message) = &download.failed {
            (format!("! gagal mengunduh: {message}"), theme::error())
        } else if download.done {
            (
                format!(
                    "✓ selesai: {} ({})",
                    crate::download::display_path(&download.path),
                    crate::download::format_bytes(download.downloaded)
                ),
                theme::accent(),
            )
        } else {
            let total = download
                .total
                .map(crate::download::format_bytes)
                .unwrap_or_else(|| "?".to_string());
            (
                format!(
                    "[{}] {}%  {} / {}  → {}",
                    download.bar(24),
                    download.percent(),
                    crate::download::format_bytes(download.downloaded),
                    total,
                    crate::download::display_path(&download.path)
                ),
                theme::base(),
            )
        };
        lines.push(Line::from(Span::styled(text, style)));
    }
    if let Some(error) = &app.error {
        lines.push(Line::from(Span::styled(
            format!("! {error}"),
            theme::error(),
        )));
        lines.push(Line::from(Span::styled(
            "  Enter/Esc untuk kembali dan coba lagi.",
            theme::dim(),
        )));
    }
    frame.render_widget(Paragraph::new(lines).alignment(Alignment::Left), area);
}
