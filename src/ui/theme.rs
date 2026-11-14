//! Warna & style terpusat.

use ratatui::style::{Color, Modifier, Style};

pub fn base() -> Style {
    Style::default()
}

pub fn title() -> Style {
    Style::default().add_modifier(Modifier::BOLD)
}

pub fn dim() -> Style {
    Style::default().add_modifier(Modifier::DIM)
}

pub fn highlight() -> Style {
    Style::default().add_modifier(Modifier::REVERSED | Modifier::BOLD)
}

pub fn accent() -> Style {
    Style::default().fg(Color::Cyan)
}

pub fn success() -> Style {
    Style::default().fg(Color::Green)
}

pub fn warning() -> Style {
    Style::default().fg(Color::Yellow)
}

pub fn error() -> Style {
    Style::default().fg(Color::Red)
}
