//! Bookmark/favorit persisten (DESIGN §15).
//!
//! Disimpan di `<data>/bookmarks.json`, ditulis atomik. Kunci unik: URL anime
//! (satu anime bisa muncul dari source berbeda dengan judul sama).

use std::path::{Path, PathBuf};

use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};

use crate::config::{write_atomic, ConfigError};

pub const BOOKMARKS_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BookmarkEntry {
    pub title: String,
    pub source: String,
    pub url: String,
    pub poster: Option<String>,
    pub added_at: DateTime<Local>,
}

impl BookmarkEntry {
    fn key(&self) -> String {
        self.url.clone()
    }

    pub fn display(&self) -> String {
        format!("{} ({})", self.title, self.source)
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct BookmarksFile {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    entries: Vec<BookmarkEntry>,
}

/// Penyimpanan bookmark.
#[derive(Debug)]
pub struct Bookmarks {
    path: PathBuf,
    max_entries: usize,
    entries: Vec<BookmarkEntry>,
}

impl Bookmarks {
    pub fn load(path: PathBuf, max_entries: usize) -> Self {
        let max_entries = max_entries.max(1);
        let entries = match std::fs::read_to_string(&path) {
            Ok(raw) => match serde_json::from_str::<BookmarksFile>(&raw) {
                Ok(file) => file.entries,
                Err(err) => {
                    tracing::warn!(
                        target: "anicli_rs::bookmarks",
                        path = %path.display(),
                        %err,
                        "file bookmark tidak bisa dibaca, dimulai kosong"
                    );
                    Vec::new()
                }
            },
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(err) => {
                tracing::warn!(
                    target: "anicli_rs::bookmarks",
                    path = %path.display(),
                    %err,
                    "gagal membaca bookmark"
                );
                Vec::new()
            }
        };

        Self {
            path,
            max_entries,
            entries,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn entries(&self) -> &[BookmarkEntry] {
        &self.entries
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn contains(&self, anime_url: &str) -> bool {
        self.entries.iter().any(|entry| entry.key() == anime_url)
    }

    /// Tambah kalau belum ada; kembalikan `true` kalau bookmark baru dibuat.
    pub fn add(&mut self, entry: BookmarkEntry) -> Result<bool, std::io::Error> {
        if self.contains(&entry.url) {
            return Ok(false);
        }
        self.entries.insert(0, entry);
        if self.entries.len() > self.max_entries {
            self.entries.truncate(self.max_entries);
        }
        self.save()?;
        Ok(true)
    }

    /// Hapus berdasarkan URL anime; `true` kalau ada yang dihapus.
    pub fn remove(&mut self, anime_url: &str) -> Result<bool, std::io::Error> {
        let before = self.entries.len();
        self.entries.retain(|entry| entry.key() != anime_url);
        let removed = self.entries.len() != before;
        if removed {
            self.save()?;
        }
        Ok(removed)
    }

    /// Toggle; mengembalikan status akhir (`true` = sekarang dibookmark).
    pub fn toggle(&mut self, entry: BookmarkEntry) -> Result<bool, std::io::Error> {
        if self.remove(&entry.url)? {
            Ok(false)
        } else {
            self.add(entry)?;
            Ok(true)
        }
    }

    pub fn clear(&mut self) -> Result<(), std::io::Error> {
        self.entries.clear();
        self.save()
    }

    fn save(&self) -> Result<(), std::io::Error> {
        let file = BookmarksFile {
            version: BOOKMARKS_VERSION,
            entries: self.entries.clone(),
        };
        let text = serde_json::to_string_pretty(&file)
            .map_err(|err| std::io::Error::other(format!("serialisasi bookmark: {err}")))?;
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

    fn entry(title: &str, url: &str) -> BookmarkEntry {
        BookmarkEntry {
            title: title.to_string(),
            source: "samehadaku".to_string(),
            url: url.to_string(),
            poster: None,
            added_at: Local.with_ymd_and_hms(2026, 11, 15, 4, 0, 0).unwrap(),
        }
    }

    #[test]
    fn add_persists_and_dedupes() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("bookmarks.json");
        let mut bookmarks = Bookmarks::load(path.clone(), 10);
        assert!(bookmarks
            .add(entry("A", "https://s.test/anime/a/"))
            .unwrap());
        assert!(!bookmarks
            .add(entry("A lagi", "https://s.test/anime/a/"))
            .unwrap());
        assert_eq!(bookmarks.entries().len(), 1);

        let reloaded = Bookmarks::load(path, 10);
        assert!(reloaded.contains("https://s.test/anime/a/"));
    }

    #[test]
    fn toggle_removes_when_present() {
        let tmp = tempfile::TempDir::new().unwrap();
        let mut bookmarks = Bookmarks::load(tmp.path().join("bookmarks.json"), 10);
        assert!(bookmarks
            .toggle(entry("A", "https://s.test/anime/a/"))
            .unwrap());
        assert!(bookmarks.contains("https://s.test/anime/a/"));
        assert!(!bookmarks
            .toggle(entry("A", "https://s.test/anime/a/"))
            .unwrap());
        assert!(!bookmarks.contains("https://s.test/anime/a/"));
    }

    #[test]
    fn remove_reports_whether_removed() {
        let tmp = tempfile::TempDir::new().unwrap();
        let mut bookmarks = Bookmarks::load(tmp.path().join("bookmarks.json"), 10);
        bookmarks.add(entry("A", "https://s.test/a")).unwrap();
        assert!(bookmarks.remove("https://s.test/a").unwrap());
        assert!(!bookmarks.remove("https://s.test/a").unwrap());
    }

    #[test]
    fn max_entries_and_clear() {
        let tmp = tempfile::TempDir::new().unwrap();
        let mut bookmarks = Bookmarks::load(tmp.path().join("bookmarks.json"), 2);
        for index in 0..4 {
            bookmarks
                .add(entry(
                    &format!("A{index}"),
                    &format!("https://s.test/{index}"),
                ))
                .unwrap();
        }
        assert_eq!(bookmarks.entries().len(), 2);
        bookmarks.clear().unwrap();
        assert!(bookmarks.is_empty());
    }

    #[test]
    fn corrupt_file_is_tolerated() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("bookmarks.json");
        std::fs::write(&path, "{{{").unwrap();
        assert!(Bookmarks::load(path, 10).is_empty());
    }
}
