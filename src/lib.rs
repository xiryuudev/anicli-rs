//! `anicli-rs` — TUI streaming anime subtitle Indonesia.
//!
//! Crate ini adalah **library** tempat semua logika berada, supaya bisa dites tanpa
//! menjalankan TUI. Binary tipisnya ada di `src/main.rs`, dan binary bantu
//! `sourcecheck` (Tahap 5) memakai library yang sama.
//!
//! Peta modul saat ini (Tahap 4):
//! - [`config`]  — skema TOML, load/validasi/migrasi/simpan atomik.
//! - [`paths`]   — lokasi config/data/log per platform.
//! - [`logging`] — tracing ke file harian + ring buffer untuk TUI.
//! - [`model`]   — tipe domain (search/detail/episode/stream/target).
//! - [`error`]   — `SourceError` + klasifikasi retryable/fatal + copy UX.
//! - [`http`]    — client bersama (rate limit/retry), `MirrorSet` probe domain.
//! - [`sources`] — trait `AnimeSource`, `Capabilities`, registry, util parsing.
//!
//! Modul yang menyusul: `fallback`, `resolve`, `player`, `history`, `bookmarks`
//! (lihat DESIGN §24).

#![forbid(unsafe_code)]

pub mod bookmarks;
pub mod config;
pub mod download;
pub mod embed;
pub mod error;
pub mod fallback;
pub mod history;
pub mod http;
pub mod logging;
pub mod model;
pub mod paths;
pub mod player;
pub mod resolve;
pub mod sources;
pub mod ui;

/// Versi aplikasi (dari `Cargo.toml`).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
