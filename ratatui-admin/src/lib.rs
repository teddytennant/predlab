//! PredLab admin library.
//!
//! Pure, testable building blocks shared by the `ratatui` admin TUI:
//! - [`registry`]: the club student roster persisted in `~/.predlab/students.db`
//!   (legacy kalshi_key column kept for backward compat with old rosters).

pub mod registry;
