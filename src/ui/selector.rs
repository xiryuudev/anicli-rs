//! Source selector popup: daftar semua source dengan filter teks.
//!
//! Muncul saat user menekan `s`. `S` (uppercase) tetap untuk toggle search scope.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::model::Language;

/// Satu baris di selector.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceEntry {
    pub name: String,
    /// Bahasa konten source (untuk label `[ID]`/`[EN]`).
    pub language: Language,
    /// Source punya factory & ikut dibangun registry.
    pub available: bool,
    /// Enabled di config saat ini.
    pub enabled: bool,
}

/// Aksi hasil input pada popup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectorAction {
    /// Bukan tombol untuk popup (biarkan handler lain).
    Ignored,
    /// Tutup tanpa memilih.
    Close,
    /// Pilih source dengan nama ini.
    Choose(String),
}

/// State popup.
#[derive(Debug, Clone, Default)]
pub struct SourceSelector {
    pub filter: String,
    pub index: usize,
    pub entries: Vec<SourceEntry>,
}

impl SourceSelector {
    pub fn new(entries: Vec<SourceEntry>, active: &str) -> Self {
        let mut selector = Self {
            filter: String::new(),
            index: 0,
            entries,
        };
        // Sorot source aktif bila ada.
        if let Some(position) = selector
            .filtered()
            .iter()
            .position(|entry| entry.name == active)
        {
            selector.index = position;
        }
        selector
    }

    /// Entri yang lolos filter (case-insensitive substring).
    pub fn filtered(&self) -> Vec<&SourceEntry> {
        let needle = self.filter.trim().to_ascii_lowercase();
        self.entries
            .iter()
            .filter(|entry| needle.is_empty() || entry.name.to_ascii_lowercase().contains(&needle))
            .collect()
    }

    pub fn selected(&self) -> Option<&SourceEntry> {
        self.filtered().get(self.index).copied()
    }

    fn move_by(&mut self, delta: isize) {
        let total = self.filtered().len();
        if total == 0 {
            self.index = 0;
            return;
        }
        let current = self.index.min(total - 1) as isize;
        let next = (current + delta).clamp(0, total as isize - 1);
        self.index = next as usize;
    }

    /// Tangani satu key event di popup.
    pub fn handle_key(&mut self, key: KeyEvent) -> SelectorAction {
        match key.code {
            KeyCode::Esc => SelectorAction::Close,
            KeyCode::Enter => match self.selected() {
                Some(entry) => SelectorAction::Choose(entry.name.clone()),
                None => SelectorAction::Close,
            },
            // j/k dipakai navigasi HANYA saat filter kosong; setelah mengetik,
            // j/k adalah huruf biasa (panah tetap navigasi).
            KeyCode::Up => {
                self.move_by(-1);
                SelectorAction::Ignored
            }
            KeyCode::Down => {
                self.move_by(1);
                SelectorAction::Ignored
            }
            KeyCode::Char('k') if self.filter.is_empty() => {
                self.move_by(-1);
                SelectorAction::Ignored
            }
            KeyCode::Char('j') if self.filter.is_empty() => {
                self.move_by(1);
                SelectorAction::Ignored
            }
            KeyCode::Backspace => {
                self.filter.pop();
                self.index = 0;
                SelectorAction::Ignored
            }
            KeyCode::Char(ch)
                if !ch.is_control()
                    && (key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT) =>
            {
                self.filter.push(ch);
                self.index = 0;
                SelectorAction::Ignored
            }
            _ => SelectorAction::Ignored,
        }
    }

    /// Pesan saat daftar kosong.
    pub fn empty_message(&self) -> &'static str {
        if self.entries.is_empty() {
            "Tidak ada source tersedia"
        } else if self.filtered().is_empty() {
            "Tidak ada source yang cocok"
        } else {
            ""
        }
    }
}

/// Pilihan bahasa untuk tab/filter Bahasa.
///
/// `None` berarti "semua bahasa".
pub fn language_options() -> Vec<(Option<Language>, String)> {
    let mut options = vec![(None, "Semua bahasa".to_string())];
    for language in Language::ALL {
        options.push((
            Some(*language),
            format!("{} {}", language.tag(), language.label()),
        ));
    }
    options
}

/// Source entry dengan label bahasa, mis. `Anichin [ID]`.
pub fn source_label(name: &str, language: Language) -> String {
    format!("{name} {}", language.tag())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn entries() -> Vec<SourceEntry> {
        vec![
            SourceEntry {
                name: "samehadaku".into(),
                language: Language::Indonesian,
                available: true,
                enabled: true,
            },
            SourceEntry {
                name: "otakudesu".into(),
                language: Language::Indonesian,
                available: true,
                enabled: true,
            },
            SourceEntry {
                name: "kusonime".into(),
                language: Language::Indonesian,
                available: true,
                enabled: false,
            },
        ]
    }

    #[test]
    fn filter_is_case_insensitive_substring() {
        let mut selector = SourceSelector::new(entries(), "samehadaku");
        selector.filter = "SAME".to_string();
        let names: Vec<&str> = selector
            .filtered()
            .iter()
            .map(|e| e.name.as_str())
            .collect();
        assert_eq!(names, vec!["samehadaku"]);
        assert_eq!(selector.selected().unwrap().name, "samehadaku");
    }

    #[test]
    fn typing_appends_to_filter_and_resets_index() {
        let mut selector = SourceSelector::new(entries(), "samehadaku");
        selector.index = 1;
        // 'k' saat filter kosong = navigasi; huruf lain masuk filter.
        selector.handle_key(key(KeyCode::Char('o')));
        assert_eq!(selector.filter, "o");
        assert_eq!(selector.index, 0);
        selector.handle_key(key(KeyCode::Backspace));
        assert_eq!(selector.filter, "");
    }

    #[test]
    fn navigation_clamps_and_enter_chooses() {
        let mut selector = SourceSelector::new(entries(), "samehadaku");
        assert_eq!(selector.index, 0);
        selector.handle_key(key(KeyCode::Char('j')));
        assert_eq!(selector.index, 1);
        selector.handle_key(key(KeyCode::Down));
        assert_eq!(selector.index, 2);
        selector.handle_key(key(KeyCode::Down));
        assert_eq!(selector.index, 2, "tidak boleh melewati batas");
        assert_eq!(
            selector.handle_key(key(KeyCode::Enter)),
            SelectorAction::Choose("kusonime".to_string())
        );
        assert_eq!(
            selector.handle_key(key(KeyCode::Esc)),
            SelectorAction::Close
        );
    }

    #[test]
    fn empty_and_no_match_messages() {
        let mut selector = SourceSelector::new(Vec::new(), "x");
        assert_eq!(selector.empty_message(), "Tidak ada source tersedia");
        assert_eq!(
            selector.handle_key(key(KeyCode::Enter)),
            SelectorAction::Close
        );

        let mut selector = SourceSelector::new(entries(), "samehadaku");
        selector.filter = "zzz".into();
        assert_eq!(selector.empty_message(), "Tidak ada source yang cocok");
        assert_eq!(
            selector.handle_key(key(KeyCode::Enter)),
            SelectorAction::Close
        );
    }
}
