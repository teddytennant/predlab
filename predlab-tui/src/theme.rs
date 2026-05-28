//! Color palette and reusable styles for the PredLab TUI.
//!
//! Goal: terminal-native (uses the user's background, no painted body) with a
//! cohesive muted-green primary and high-contrast cues for positive/negative
//! P&L. The palette is taken from the website (`predlab.teddytennant.com`) so
//! the TUI feels like the same product, just in your shell.

use ratatui::style::{Color, Modifier, Style};

/// Foreground text — bright enough on any dark terminal.
pub const FG: Color = Color::Rgb(232, 232, 232);
/// De-emphasized labels, help hints, subdued chrome.
pub const DIM: Color = Color::Rgb(120, 120, 120);
/// Borders, separators — barely-there structure.
pub const BORDER: Color = Color::Rgb(60, 70, 60);
/// PredLab green — primary accent.
pub const PRIMARY: Color = Color::Rgb(120, 200, 140);
/// Amber for the #1 leader / highlights.
pub const ACCENT: Color = Color::Rgb(240, 200, 100);
/// P&L green.
pub const POSITIVE: Color = Color::Rgb(100, 200, 130);
/// P&L red.
pub const NEGATIVE: Color = Color::Rgb(220, 110, 110);
/// Status-line vim-mode tag background.
pub const MODE_NORMAL: Color = Color::Rgb(60, 130, 90);
pub const MODE_COMMAND: Color = Color::Rgb(180, 140, 60);
pub const MODE_SEARCH: Color = Color::Rgb(80, 130, 180);

pub fn fg() -> Style {
    Style::default().fg(FG)
}
pub fn dim() -> Style {
    Style::default().fg(DIM)
}
pub fn border() -> Style {
    Style::default().fg(BORDER)
}
pub fn border_active() -> Style {
    Style::default().fg(PRIMARY)
}
pub fn primary() -> Style {
    Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD)
}
pub fn accent() -> Style {
    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
}
pub fn positive() -> Style {
    Style::default().fg(POSITIVE)
}
pub fn negative() -> Style {
    Style::default().fg(NEGATIVE)
}

/// Highlight bar on the currently-selected list row.
pub fn row_highlight() -> Style {
    Style::default()
        .bg(Color::Rgb(28, 38, 30))
        .add_modifier(Modifier::BOLD)
}

/// Tinted block for the active vim mode pill at the left of the status line.
pub fn mode_pill(bg: Color) -> Style {
    Style::default()
        .fg(Color::Black)
        .bg(bg)
        .add_modifier(Modifier::BOLD)
}
