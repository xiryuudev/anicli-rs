//! Logging ke file harian + ring buffer in-app (DESIGN §16).
//!
//! Saat TUI aktif, stdout/stderr tidak boleh dipakai (merusak layar). Karena itu:
//! - Semua log selalu masuk ke file `anicli-rs.YYYY-MM-DD.log` di `<data>/logs/`.
//! - Ring buffer [`LogBuffer`] menyimpan N baris terakhir untuk layar `Logs` di TUI.
//! - stderr hanya aktif kalau diminta (`--no-tui` / `logging.to_stderr`).
//!
//! [`LogHandle`] **harus** dipegang sampai program selesai; kalau di-drop, worker
//! non-blocking tracing berhenti dan sisa log bisa hilang.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use anyhow::{Context, Result};
use tracing::field::{Field, Visit};
use tracing::{Event, Subscriber};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::fmt::time::ChronoLocal;
use tracing_subscriber::layer::{Context as LayerContext, Layer};
use tracing_subscriber::prelude::*;
use tracing_subscriber::EnvFilter;

/// Prefix file log; `tracing-appender` menambahkan tanggal: `anicli-rs.YYYY-MM-DD.log`.
pub const LOG_FILE_PREFIX: &str = "anicli-rs";

/// Satu baris log yang sudah diformat, siap ditampilkan di TUI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogLine {
    pub timestamp: String,
    pub level: String,
    pub target: String,
    pub message: String,
}

impl LogLine {
    /// Representasi satu baris untuk layar Logs / `--no-tui`.
    pub fn format(&self) -> String {
        format!(
            "{} {:<5} {}: {}",
            self.timestamp, self.level, self.target, self.message
        )
    }
}

/// Ring buffer thread-safe untuk N baris log terakhir.
#[derive(Clone)]
pub struct LogBuffer {
    inner: Arc<Mutex<VecDeque<LogLine>>>,
    capacity: usize,
}

impl LogBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(VecDeque::with_capacity(capacity))),
            capacity: capacity.max(1),
        }
    }

    pub fn push(&self, line: LogLine) {
        if let Ok(mut guard) = self.inner.lock() {
            if guard.len() == self.capacity {
                guard.pop_front();
            }
            guard.push_back(line);
        }
    }

    /// Salinan semua baris (urutan lama → baru).
    pub fn snapshot(&self) -> Vec<LogLine> {
        self.inner
            .lock()
            .map(|guard| guard.iter().cloned().collect())
            .unwrap_or_default()
    }

    pub fn len(&self) -> usize {
        self.inner.lock().map(|guard| guard.len()).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn clear(&self) {
        if let Ok(mut guard) = self.inner.lock() {
            guard.clear();
        }
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }
}

/// Opsi inisialisasi logging.
#[derive(Debug, Clone)]
pub struct Options {
    pub log_dir: PathBuf,
    /// Level atau direktif `EnvFilter` (mis. `"anicli_rs=debug,reqwest=warn"`).
    pub level: String,
    /// Tulis juga ke stderr (aman hanya saat TUI tidak aktif).
    pub to_stderr: bool,
    /// Kapasitas ring buffer untuk layar Logs.
    pub buffer_capacity: usize,
    /// Hapus file log lebih tua dari N hari (0 = jangan hapus).
    pub max_days: u32,
}

/// Pegangan logging. Harus hidup selama program berjalan.
pub struct LogHandle {
    buffer: LogBuffer,
    log_dir: PathBuf,
    _guard: WorkerGuard,
}

impl LogHandle {
    pub fn buffer(&self) -> &LogBuffer {
        &self.buffer
    }

    pub fn log_dir(&self) -> &Path {
        &self.log_dir
    }
}

/// Pasang subscriber global: filter → buffer → file → (opsional) stderr.
pub fn init(options: Options) -> Result<LogHandle> {
    std::fs::create_dir_all(&options.log_dir)
        .with_context(|| format!("gagal membuat direktori log {}", options.log_dir.display()))?;

    if options.max_days > 0 {
        prune_old_logs(&options.log_dir, options.max_days);
    }

    let filter = EnvFilter::try_new(&options.level)
        .with_context(|| format!("level log tidak valid: {:?}", options.level))?;

    let appender = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix(LOG_FILE_PREFIX)
        .filename_suffix("log")
        .build(&options.log_dir)
        .with_context(|| format!("gagal membuka file log di {}", options.log_dir.display()))?;
    let (writer, guard) = tracing_appender::non_blocking(appender);

    let buffer = LogBuffer::new(options.buffer_capacity);

    // Timestamp lokal (bukan UTC) supaya cocok dengan jam user dan baris di ring buffer.
    let timer = ChronoLocal::new("%Y-%m-%dT%H:%M:%S%.3f%:z".to_string());

    let file_layer = tracing_subscriber::fmt::layer()
        .with_writer(writer)
        .with_timer(timer.clone())
        .with_ansi(false)
        .with_target(true)
        .with_thread_ids(false)
        .with_thread_names(false);

    let stderr_layer = options.to_stderr.then(|| {
        tracing_subscriber::fmt::layer()
            .with_writer(std::io::stderr)
            .with_timer(timer.clone())
            .with_ansi(true)
            .with_target(true)
    });

    tracing_subscriber::registry()
        .with(filter)
        .with(BufferLayer {
            buffer: buffer.clone(),
        })
        .with(file_layer)
        .with(stderr_layer)
        .try_init()
        .context("subscriber tracing sudah terpasang di proses ini")?;

    Ok(LogHandle {
        buffer,
        log_dir: options.log_dir,
        _guard: guard,
    })
}

/// Layer yang menyalin event ke ring buffer (untuk TUI).
struct BufferLayer {
    buffer: LogBuffer,
}

impl<S: Subscriber> Layer<S> for BufferLayer {
    fn on_event(&self, event: &Event<'_>, _ctx: LayerContext<'_, S>) {
        let metadata = event.metadata();
        let mut visitor = FieldVisitor::default();
        event.record(&mut visitor);

        let mut message = visitor.message.unwrap_or_default();
        for (name, value) in visitor.extras {
            if !message.is_empty() {
                message.push(' ');
            }
            message.push_str(&name);
            message.push('=');
            message.push_str(&value);
        }

        self.buffer.push(LogLine {
            timestamp: chrono::Local::now()
                .format("%Y-%m-%dT%H:%M:%S%.3f%:z")
                .to_string(),
            level: metadata.level().to_string(),
            target: metadata.target().to_string(),
            message,
        });
    }
}

#[derive(Default)]
struct FieldVisitor {
    message: Option<String>,
    extras: Vec<(String, String)>,
}

impl FieldVisitor {
    fn record(&mut self, field: &Field, text: String) {
        if field.name() == "message" {
            self.message = Some(text);
        } else {
            let name = field.name().to_string();
            let value = redact(field.name(), &text);
            self.extras.push((name, value));
        }
    }
}

impl Visit for FieldVisitor {
    fn record_str(&mut self, field: &Field, value: &str) {
        self.record(field, value.to_string());
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.record(field, format!("{value:?}"));
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.record(field, value.to_string());
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.record(field, value.to_string());
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.record(field, value.to_string());
    }

    fn record_f64(&mut self, field: &Field, value: f64) {
        self.record(field, value.to_string());
    }

    fn record_error(&mut self, field: &Field, value: &(dyn std::error::Error + 'static)) {
        self.record(field, value.to_string());
    }
}

/// Nilai sensitif tidak boleh masuk log/TUI (DESIGN §16).
fn redact(field: &str, value: &str) -> String {
    const SENSITIVE: &[&str] = &[
        "cookie",
        "authorization",
        "token",
        "password",
        "secret",
        "api_key",
    ];
    let lowered = field.to_ascii_lowercase();
    if SENSITIVE.iter().any(|needle| lowered.contains(needle)) {
        "[REDACTED]".to_string()
    } else {
        value.to_string()
    }
}

/// Hapus file `anicli-rs.*.log` yang lebih tua dari `max_days`.
/// Best-effort: kegagalan diabaikan (logging tidak boleh menggagalkan aplikasi).
fn prune_old_logs(dir: &Path, max_days: u32) {
    let max_age = Duration::from_secs(u64::from(max_days) * 24 * 60 * 60);
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let now = SystemTime::now();
    for entry in entries.flatten() {
        let path = entry.path();
        let is_our_log = path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with(LOG_FILE_PREFIX) && name.ends_with(".log"));
        if !is_our_log {
            continue;
        }
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        let Ok(modified) = metadata.modified() else {
            continue;
        };
        let too_old = now
            .duration_since(modified)
            .map(|age| age > max_age)
            .unwrap_or(false);
        if too_old {
            let _ = std::fs::remove_file(&path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buffer_is_bounded_and_ordered() {
        let buffer = LogBuffer::new(2);
        for index in 0..3 {
            buffer.push(LogLine {
                timestamp: "t".into(),
                level: "INFO".into(),
                target: "test".into(),
                message: index.to_string(),
            });
        }
        let snapshot = buffer.snapshot();
        assert_eq!(snapshot.len(), 2);
        assert_eq!(snapshot[0].message, "1");
        assert_eq!(snapshot[1].message, "2");
        assert_eq!(buffer.capacity(), 2);
        buffer.clear();
        assert!(buffer.is_empty());
    }

    #[test]
    fn sensitive_fields_are_redacted() {
        assert_eq!(redact("cookie", "abc=1"), "[REDACTED]");
        assert_eq!(redact("Authorization", "Bearer x"), "[REDACTED]");
        assert_eq!(redact("request_token", "abc"), "[REDACTED]");
        assert_eq!(redact("url", "https://example.com"), "https://example.com");
    }

    #[test]
    fn old_logs_are_pruned_and_fresh_ones_kept() {
        let tmp = tempfile::TempDir::new().unwrap();
        let old = tmp.path().join("anicli-rs.2000-01-01.log");
        let fresh = tmp.path().join("anicli-rs.2099-01-01.log");
        let unrelated = tmp.path().join("other.txt");
        std::fs::write(&old, "x").unwrap();
        std::fs::write(&fresh, "x").unwrap();
        std::fs::write(&unrelated, "x").unwrap();

        // Mundurkan mtime file "lama" 30 hari (tanpa dependency tambahan).
        let backdated = SystemTime::now() - Duration::from_secs(30 * 24 * 60 * 60);
        std::fs::File::options()
            .write(true)
            .open(&old)
            .unwrap()
            .set_times(std::fs::FileTimes::new().set_modified(backdated))
            .unwrap();

        prune_old_logs(tmp.path(), 7);

        assert!(!old.exists(), "file lama harus dihapus");
        assert!(fresh.exists(), "file baru harus dipertahankan");
        assert!(unrelated.exists(), "file lain tidak boleh disentuh");
    }

    #[test]
    fn log_line_format_contains_level_and_target() {
        let line = LogLine {
            timestamp: "2026-11-14T21:00:00+07:00".into(),
            level: "WARN".into(),
            target: "anicli_rs::http".into(),
            message: "retry".into(),
        };
        let text = line.format();
        assert!(text.contains("WARN"));
        assert!(text.contains("anicli_rs::http"));
        assert!(text.contains("retry"));
    }
}
