//! TUI ratatui (DESIGN §13).
//!
//! Struktur:
//! - [`app`] — state machine + penanganan input + spawning job async.
//! - [`render`] — fungsi render murni (`&App` → frame), dites dengan `TestBackend`.
//! - [`theme`] — style/warna terpusat.
//!
//! Prinsip: render tidak pernah menunggu I/O; semua request berjalan di task `tokio`
//! dan hasilnya masuk lewat channel sebagai [`app::AppEvent`].

pub mod app;
pub mod render;
pub mod theme;

pub use app::{run, App, AppEvent, Screen};
pub use render::render;
