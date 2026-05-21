//! PredLab admin library.
//!
//! Pure, testable building blocks shared by the `ratatui` admin TUI:
//! - [`registry`]: the club student roster (dual paper keys) persisted in
//!   `~/.predlab/students.db` — the same SQLite schema the previous Python TUI
//!   used, so existing student data carries over unchanged.
//! - [`leaderboard`]: deterministic ranking + money formatting for the club
//!   standings view.

pub mod leaderboard;
pub mod registry;
