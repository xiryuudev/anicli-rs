//! Riwayat tonton persisten (DESIGN §15).
//!
//! Disimpan sebagai JSON di `<data>/history.json`, ditulis atomik. Satu entri per
//! `(judul anime, nomor episode)`: memutar ulang episode yang sama hanya
//! memperbarui timestamp dan memindahkannya ke depan.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};

use crate::config::{write_atomic, ConfigError};

/// Versi format file.
pub const HISTORY_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub anime_title: String,
    pub source: String,
    pub episode_number: Option<u32>,
    pub episode_label: String,
    pub anime_url: String,
    pub episode_url: String,
    pub resolution: Option<String>,
    pub played_at: DateTime<Local>,
    /// Posisi terakhir (detik); diisi saat resume berbasis IPC tersedia.
    pub position_secs: Option<u64>,
}

impl HistoryEntry {
    /// Kunci dedupe: anime + episode (sumber boleh berbeda).
    fn key(&self) -> (String, Option<u32>, String) {
        (
            self.anime_title.to_lowercase(),
            self.episode_number,
            self.episode_label.to_lowercase(),
        )
    }

    pub fn display(&self) -> String {
        format!(
            "{} — {} ({})",
            self.anime_title, self.episode_label, self.source
        )
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct HistoryFile {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    entries: Vec<HistoryEntry>,
}

/// Penyimpanan riwayat tonton.
#[derive(Debug)]
pub struct History {
    path: PathBuf,
    max_entries: usize,
    entries: Vec<HistoryEntry>,
}

impl History {
    /// Muat dari disk; file rusak/hilang tidak fatal (riwayat kosong + warning log).
    pub fn load(path: PathBuf, max_entries: usize) -> Self {
        let max_entries = max_entries.max(1);
        let entries = match std::fs::read_to_string(&path) {
            Ok(raw) => match serde_json::from_str::<HistoryFile>(&raw) {
                Ok(file) => file.entries,
                Err(err) => {
                    tracing::warn!(
                        target: "anicli_rs::history",
                        path = %path.display(),
                        %err,
                        "file riwayat tidak bisa dibaca, dimulai kosong"
                    );
                    Vec::new()
                }
            },
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(err) => {
                tracing::warn!(
                    target: "anicli_rs::history",
                    path = %path.display(),
                    %err,
                    "gagal membaca riwayat"
                );
                Vec::new()
            }
        };

        let mut history = Self {
            path,
            max_entries,
            entries,
        };
        history.trim();
        history
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn entries(&self) -> &[HistoryEntry] {
        &self.entries
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Catat pemutaran (dedupe & simpan). Error simpan dikembalikan, bukan panic.
    pub fn record(&mut self, entry: HistoryEntry) -> Result<(), std::io::Error> {
        let key = entry.key();
        self.entries.retain(|existing| existing.key() != key);
        self.entries.insert(0, entry);
        self.trim();
        self.save()
    }

    pub fn remove(&mut self, index: usize) -> Result<(), std::io::Error> {
        if index < self.entries.len() {
            self.entries.remove(index);
            self.save()?;
        }
        Ok(())
    }

    pub fn clear(&mut self) -> Result<(), std::io::Error> {
        self.entries.clear();
        self.save()
    }

    /// Episode terakhir untuk satu judul anime (untuk fitur "lanjutkan").
    pub fn last_for(&self, anime_title: &str) -> Option<&HistoryEntry> {
        let needle = anime_title.to_lowercase();
        self.entries
            .iter()
            .find(|entry| entry.anime_title.to_lowercase() == needle)
    }

    fn trim(&mut self) {
        if self.entries.len() > self.max_entries {
            self.entries.truncate(self.max_entries);
        }
    }

    fn save(&self) -> Result<(), std::io::Error> {
        let file = HistoryFile {
            version: HISTORY_VERSION,
            entries: self.entries.clone(),
        };
        let text = serde_json::to_string_pretty(&file)
            .map_err(|err| std::io::Error::other(format!("serialisasi riwayat: {err}")))?;
        write_atomic(&self.path, &text).map_err(|err| match err {
            ConfigError::Write { source, .. } => source,
            other => std::io::Error::other(other.to_string()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn entry(title: &str, episode: Option<u32>) -> HistoryEntry {
        HistoryEntry {
            anime_title: title.to_string(),
            source: "samehadaku".to_string(),
            episode_number: episode,
            episode_label: format!("Episode {}", episode.unwrap_or(0)),
            anime_url: format!("https://s.test/anime/{title}"),
            episode_url: format!("https://s.test/{title}/ep{}", episode.unwrap_or(0)),
            resolution: Some("720p".to_string()),
            played_at: Local.with_ymd_and_hms(2026, 11, 15, 4, 0, 0).unwrap(),
            position_secs: None,
        }
    }

    #[test]
    fn missing_file_starts_empty() {
        let tmp = tempfile::TempDir::new().unwrap();
        let history = History::load(tmp.path().join("history.json"), 10);
        assert!(history.is_empty());
    }

    #[test]
    fn record_persists_and_reloads() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("history.json");
        let mut history = History::load(path.clone(), 10);
        history.record(entry("Shuiro no Kamen", Some(6))).unwrap();

        let reloaded = History::load(path, 10);
        assert_eq!(reloaded.entries().len(), 1);
        assert_eq!(reloaded.entries()[0].episode_number, Some(6));
        assert_eq!(reloaded.entries()[0].resolution.as_deref(), Some("720p"));
    }

    #[test]
    fn duplicate_episode_moves_to_front_and_is_deduped() {
        let tmp = tempfile::TempDir::new().unwrap();
        let mut history = History::load(tmp.path().join("history.json"), 10);
        history.record(entry("A", Some(1))).unwrap();
        history.record(entry("B", Some(1))).unwrap();
        history.record(entry("A", Some(1))).unwrap();

        assert_eq!(history.entries().len(), 2);
        assert_eq!(history.entries()[0].anime_title, "A");
    }

    #[test]
    fn max_entries_is_enforced() {
        let tmp = tempfile::TempDir::new().unwrap();
        let mut history = History::load(tmp.path().join("history.json"), 2);
        for episode in 1..=4 {
            history.record(entry("A", Some(episode))).unwrap();
        }
        assert_eq!(history.entries().len(), 2);
        assert_eq!(history.entries()[0].episode_number, Some(4));
    }

    #[test]
    fn last_for_returns_most_recent_episode_of_title() {
        let tmp = tempfile::TempDir::new().unwrap();
        let mut history = History::load(tmp.path().join("history.json"), 10);
        history.record(entry("A", Some(1))).unwrap();
        history.record(entry("B", Some(1))).unwrap();
        history.record(entry("A", Some(2))).unwrap();
        assert_eq!(
            history.last_for("a").map(|entry| entry.episode_number),
            Some(Some(2))
        );
    }

    #[test]
    fn corrupt_file_is_tolerated() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("history.json");
        std::fs::write(&path, "bukan json").unwrap();
        let history = History::load(path, 10);
        assert!(history.is_empty());
    }

    #[test]
    fn remove_and_clear_persist() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("history.json");
        let mut history = History::load(path.clone(), 10);
        history.record(entry("A", Some(1))).unwrap();
        history.record(entry("B", Some(1))).unwrap();
        history.remove(0).unwrap();
        assert_eq!(History::load(path.clone(), 10).entries().len(), 1);
        history.clear().unwrap();
        assert!(History::load(path, 10).is_empty());
    }

    #[test]
    fn display_is_readable() {
        let text = entry("Shuiro no Kamen", Some(6)).display();
        assert!(text.contains("Shuiro no Kamen"));
        assert!(text.contains("Episode 6"));
        assert!(text.contains("samehadaku"));
    }
}
