//! Tipe error domain (DESIGN §17).
//!
//! Prinsip: setiap kegagalan punya **sumber** dan **aksi yang bisa dilakukan user**.
//! Tidak ada error generik "something went wrong": UI menampilkan `user_message()`,
//! log menyimpan detail teknis, dan orchestrator fallback memakai `is_retryable()` /
//! `is_source_fatal()` untuk memutuskan lanjut ke source berikutnya.
//!
//! Catatan desain: field nama source sengaja dinamai `site`, bukan `source`, karena
//! `thiserror` memperlakukan field bernama `source` sebagai error-source (harus
//! implement `std::error::Error`) — `&'static str` tidak.

use std::fmt;

/// Error dari satu source (adapter). Field `site` memakai nama stabil
/// (`"samehadaku"`) supaya muncul konsisten di log dan jejak fallback.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SourceError {
    #[error("{site}: jaringan gagal ({url}): {detail}")]
    Network {
        site: &'static str,
        url: String,
        detail: String,
    },

    #[error("{site}: timeout saat {step}")]
    Timeout {
        site: &'static str,
        step: &'static str,
    },

    #[error("{site}: HTTP {status} ({url})")]
    Http {
        site: &'static str,
        url: String,
        status: u16,
    },

    /// Cloudflare/anti-bot challenge nyata (bukan sekadar 403 karena UA).
    #[error("{site}: permintaan diblokir sistem anti-bot ({url})")]
    Blocked { site: &'static str, url: String },

    /// Halaman berubah sehingga selector kunci tidak menemukan apa pun.
    #[error("{site}: gagal mem-parse halaman {page} (field `{field}`): {hint}")]
    Parse {
        site: &'static str,
        page: &'static str,
        field: &'static str,
        hint: String,
    },

    #[error("{site}: tidak ditemukan: {what}")]
    NotFound { site: &'static str, what: String },

    /// Source tidak menyediakan fitur tersebut (bukan kegagalan, jangan fallback).
    #[error("{site}: fitur `{feature}` tidak didukung source ini")]
    Unsupported {
        site: &'static str,
        feature: &'static str,
    },

    /// Semua domain/mirror kandidat gagal di-probe.
    #[error("{site}: tidak ada domain yang bisa dipakai (dicoba {tried} kandidat)")]
    NoMirror { site: &'static str, tried: usize },

    /// Kesalahan di sisi aplikasi (config/player), bukan source.
    #[error("{0}")]
    App(String),
}

impl SourceError {
    pub fn network(site: &'static str, url: impl fmt::Display, detail: impl fmt::Display) -> Self {
        Self::Network {
            site,
            url: url.to_string(),
            detail: detail.to_string(),
        }
    }

    pub fn parse(
        site: &'static str,
        page: &'static str,
        field: &'static str,
        hint: impl fmt::Display,
    ) -> Self {
        Self::Parse {
            site,
            page,
            field,
            hint: hint.to_string(),
        }
    }

    pub fn not_found(site: &'static str, what: impl fmt::Display) -> Self {
        Self::NotFound {
            site,
            what: what.to_string(),
        }
    }

    pub fn unsupported(site: &'static str, feature: &'static str) -> Self {
        Self::Unsupported { site, feature }
    }

    pub fn app(detail: impl fmt::Display) -> Self {
        Self::App(detail.to_string())
    }

    /// Nama source yang gagal (kalau ada).
    pub fn source_name(&self) -> Option<&'static str> {
        match self {
            Self::Network { site, .. }
            | Self::Timeout { site, .. }
            | Self::Http { site, .. }
            | Self::Blocked { site, .. }
            | Self::Parse { site, .. }
            | Self::NotFound { site, .. }
            | Self::Unsupported { site, .. }
            | Self::NoMirror { site, .. } => Some(site),
            Self::App(_) => None,
        }
    }

    /// Layak dicoba ulang (dengan jeda) ke source yang sama?
    pub fn is_retryable(&self) -> bool {
        match self {
            Self::Network { .. } | Self::Timeout { .. } => true,
            Self::Http { status, .. } => *status == 429 || (500..=599).contains(status),
            _ => false,
        }
    }

    /// Kalau true, fallback ke source lain hanya membuang waktu (mis. config salah).
    pub fn is_source_fatal(&self) -> bool {
        matches!(self, Self::Unsupported { .. } | Self::App(_))
    }

    /// Pesan siap tampil di TUI — bahasa Indonesia, menyebut aksi berikutnya.
    pub fn user_message(&self) -> String {
        match self {
            Self::Network { site, .. } => format!(
                "{site} tidak bisa dihubungi (jaringan). Mencoba source berikutnya…"
            ),
            Self::Timeout { site, step } => {
                format!("{site} lambat merespons saat {step}. Mencoba source berikutnya…")
            }
            Self::Http { site, status, .. } => match status {
                429 => format!(
                    "{site} membatasi permintaan (HTTP 429). Tunggu sebentar lalu coba lagi."
                ),
                401 | 403 => format!("{site} menolak permintaan (HTTP {status}). Coba source lain."),
                404 => format!("Halaman {site} tidak ditemukan (HTTP 404)."),
                500..=599 => {
                    format!("{site} sedang bermasalah (HTTP {status}). Mencoba source berikutnya…")
                }
                _ => format!("{site} membalas HTTP {status}."),
            },
            Self::Blocked { site, .. } => format!(
                "{site} memblokir permintaan otomatis (anti-bot). Pakai source lain; \
                 opsi cookie manual ada di config [http]."
            ),
            Self::Parse {
                site,
                page,
                field,
                ..
            } => format!(
                "Struktur halaman {site} ({page}) berubah — parser tidak menemukan `{field}`. \
                 Pakai source lain sementara dan laporkan ini."
            ),
            Self::NotFound { site, what } => format!("{site}: {what} tidak ditemukan."),
            Self::Unsupported { site, feature } => {
                format!("{site} tidak menyediakan `{feature}`.")
            }
            Self::NoMirror { site, .. } => format!(
                "{site}: semua domain kandidat mati/di-parkir. Perbarui `base_url`/`mirrors` di config."
            ),
            Self::App(detail) => detail.clone(),
        }
    }
}

impl From<reqwest::Error> for SourceError {
    fn from(err: reqwest::Error) -> Self {
        if err.is_timeout() {
            return Self::Timeout {
                site: "http",
                step: "request",
            };
        }
        Self::Network {
            site: "http",
            url: err.url().map(ToString::to_string).unwrap_or_default(),
            detail: err.to_string(),
        }
    }
}

/// Hasil operasi source: `Result<T, SourceError>`.
pub type SourceResult<T> = Result<T, SourceError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retryable_classification() {
        assert!(SourceError::network("s", "https://x", "reset").is_retryable());
        assert!(SourceError::Timeout {
            site: "s",
            step: "search"
        }
        .is_retryable());
        assert!(SourceError::Http {
            site: "s",
            url: "https://x".into(),
            status: 503
        }
        .is_retryable());
        assert!(SourceError::Http {
            site: "s",
            url: "https://x".into(),
            status: 429
        }
        .is_retryable());
        assert!(!SourceError::Http {
            site: "s",
            url: "https://x".into(),
            status: 404
        }
        .is_retryable());
        assert!(!SourceError::parse("s", "detail", "title", "kosong").is_retryable());
    }

    #[test]
    fn fatal_errors_are_not_worth_falling_back() {
        assert!(SourceError::unsupported("kusonime", "streams").is_source_fatal());
        assert!(SourceError::app("config rusak").is_source_fatal());
        assert!(!SourceError::not_found("s", "episode 12").is_source_fatal());
    }

    #[test]
    fn user_messages_are_actionable() {
        let blocked = SourceError::Blocked {
            site: "samehadaku",
            url: "https://x".into(),
        };
        let text = blocked.user_message();
        assert!(text.contains("samehadaku"));
        assert!(text.contains("source lain"));

        let unsupported = SourceError::unsupported("kusonime", "streams");
        assert!(unsupported.user_message().contains("tidak menyediakan"));

        let not_found = SourceError::not_found("otakudesu", "episode 99");
        assert!(not_found.user_message().contains("episode 99"));
    }

    #[test]
    fn source_name_is_present_for_adapter_errors() {
        let err = SourceError::not_found("otakudesu", "x");
        assert_eq!(err.source_name(), Some("otakudesu"));
        assert_eq!(SourceError::app("boom").source_name(), None);
    }

    #[test]
    fn display_includes_source_and_detail() {
        let err = SourceError::Http {
            site: "samehadaku",
            url: "https://v2.samehadaku.how/x".into(),
            status: 523,
        };
        let text = err.to_string();
        assert!(text.contains("samehadaku"));
        assert!(text.contains("523"));
    }
}
