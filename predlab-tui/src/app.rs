//! TUI application state + key dispatch.
//!
//! Vim-style modes: NORMAL (navigate, switch tabs), COMMAND (`:cmd`), SEARCH
//! (`/needle`). Tab content is loaded asynchronously and lives in `LoadState`
//! so the UI can render skeletons/spinners while requests are inflight.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::api::{LeaderRow, Market, Portfolio, Position};

/// Top-level tabs the user can cycle through with 1-4 / h / l.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Leaderboard,
    Markets,
    Portfolio,
    Help,
}

impl Tab {
    pub const ALL: [Tab; 4] = [Tab::Leaderboard, Tab::Markets, Tab::Portfolio, Tab::Help];

    pub fn index(self) -> usize {
        match self {
            Tab::Leaderboard => 0,
            Tab::Markets => 1,
            Tab::Portfolio => 2,
            Tab::Help => 3,
        }
    }

    pub fn from_index(i: usize) -> Tab {
        Tab::ALL[i.min(3)]
    }

    pub fn title(self) -> &'static str {
        match self {
            Tab::Leaderboard => "LEADERBOARD",
            Tab::Markets => "MARKETS",
            Tab::Portfolio => "PORTFOLIO",
            Tab::Help => "HELP",
        }
    }
}

/// Editing mode for the bottom status line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Command,
    Search,
}

/// Lifecycle of a fetched dataset.
#[derive(Debug, Clone, Default)]
pub enum LoadState<T> {
    #[default]
    Idle,
    Loading,
    Loaded { data: T, at: Instant },
    Error(String),
}

impl<T> LoadState<T> {
    pub fn data(&self) -> Option<&T> {
        match self {
            LoadState::Loaded { data, .. } => Some(data),
            _ => None,
        }
    }

    pub fn is_loading(&self) -> bool {
        matches!(self, LoadState::Loading)
    }

    pub fn loaded_at(&self) -> Option<Instant> {
        match self {
            LoadState::Loaded { at, .. } => Some(*at),
            _ => None,
        }
    }
}

/// Result of a background HTTP fetch, posted back to the main loop.
pub enum FetchMsg {
    Leaderboard(Result<Vec<LeaderRow>, String>),
    Markets(Result<Vec<Market>, String>),
    Portfolio {
        portfolio: Result<Portfolio, String>,
        positions: Result<Vec<Position>, String>,
    },
}

/// Events the main loop reacts to.
pub enum Event {
    Key(KeyEvent),
    Resize,
    Tick,
    Fetch(FetchMsg),
}

/// Bounded ring buffer of net-worth snapshots — drives the portfolio sparkline.
const HISTORY_CAP: usize = 240;

pub struct App {
    pub tab: Tab,
    pub mode: Mode,
    pub status: String,
    pub command: String,
    pub search: String,
    /// Persisted between mode toggles so re-entering `/` keeps the last needle.
    pub last_search: String,
    pub leaderboard: LoadState<Vec<LeaderRow>>,
    pub markets: LoadState<Vec<Market>>,
    pub portfolio: LoadState<Portfolio>,
    pub positions: LoadState<Vec<Position>>,
    /// Net-worth history accumulated client-side (the sim's `/portfolio`
    /// endpoint returns only a snapshot). Appended on every successful refresh.
    pub net_worth_history: VecDeque<f64>,
    pub leaderboard_sel: usize,
    pub markets_sel: usize,
    pub positions_sel: usize,
    /// Pending `g` for the `gg` chord (vim "jump to top").
    pub pending_g: bool,
    pub should_quit: bool,
    pub has_key: bool,
}

impl App {
    pub fn new(has_key: bool) -> Self {
        let status = if has_key {
            "Connected. Press ? for help, : for commands.".into()
        } else {
            "No POLY_API_KEY set — public views work; Portfolio needs your key.".into()
        };
        Self {
            tab: Tab::Leaderboard,
            mode: Mode::Normal,
            status,
            command: String::new(),
            search: String::new(),
            last_search: String::new(),
            leaderboard: LoadState::Idle,
            markets: LoadState::Idle,
            portfolio: LoadState::Idle,
            positions: LoadState::Idle,
            net_worth_history: VecDeque::with_capacity(HISTORY_CAP),
            leaderboard_sel: 0,
            markets_sel: 0,
            positions_sel: 0,
            pending_g: false,
            should_quit: false,
            has_key,
        }
    }

    /// How many selectable rows the current tab has (for clamp/scroll math).
    pub fn current_list_len(&self) -> usize {
        self.list_len(self.tab)
    }

    /// Selectable-row count for a specific tab, honoring the active search
    /// filter but without cloning the underlying data (callers only need the
    /// length, not the rows).
    fn list_len(&self, tab: Tab) -> usize {
        let needle = self.last_search.to_lowercase();
        match tab {
            Tab::Leaderboard => match self.leaderboard.data() {
                Some(rows) if needle.is_empty() => rows.len(),
                Some(rows) => rows
                    .iter()
                    .filter(|r| r.username.to_lowercase().contains(&needle))
                    .count(),
                None => 0,
            },
            Tab::Markets => match self.markets.data() {
                Some(rows) if needle.is_empty() => rows.len(),
                Some(rows) => rows.iter().filter(|m| market_matches(m, &needle)).count(),
                None => 0,
            },
            // Positions aren't search-filtered.
            Tab::Portfolio => self.positions.data().map(|p| p.len()).unwrap_or(0),
            Tab::Help => 0,
        }
    }

    /// Clamp the stored selection for `tab` into `[0, len)` after its dataset
    /// changed. Safe to call regardless of which tab is currently active.
    fn clamp_selection(&mut self, tab: Tab) {
        let len = self.list_len(tab);
        let sel = match tab {
            Tab::Leaderboard => &mut self.leaderboard_sel,
            Tab::Markets => &mut self.markets_sel,
            Tab::Portfolio => &mut self.positions_sel,
            Tab::Help => return,
        };
        if len == 0 {
            *sel = 0;
        } else if *sel >= len {
            *sel = len - 1;
        }
    }

    pub fn selection(&self) -> usize {
        match self.tab {
            Tab::Leaderboard => self.leaderboard_sel,
            Tab::Markets => self.markets_sel,
            Tab::Portfolio => self.positions_sel,
            Tab::Help => 0,
        }
    }

    fn set_selection(&mut self, i: usize) {
        match self.tab {
            Tab::Leaderboard => self.leaderboard_sel = i,
            Tab::Markets => self.markets_sel = i,
            Tab::Portfolio => self.positions_sel = i,
            Tab::Help => {}
        }
    }

    pub fn filtered_leaderboard(&self) -> Option<Vec<LeaderRow>> {
        let rows = self.leaderboard.data()?;
        let needle = self.last_search.to_lowercase();
        if needle.is_empty() {
            return Some(rows.clone());
        }
        Some(
            rows.iter()
                .filter(|r| r.username.to_lowercase().contains(&needle))
                .cloned()
                .collect(),
        )
    }

    pub fn filtered_markets(&self) -> Option<Vec<Market>> {
        let rows = self.markets.data()?;
        let needle = self.last_search.to_lowercase();
        if needle.is_empty() {
            return Some(rows.clone());
        }
        Some(
            rows.iter()
                .filter(|m| market_matches(m, &needle))
                .cloned()
                .collect(),
        )
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> KeyOutcome {
        // Ctrl-C always exits, regardless of mode.
        if key.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(key.code, KeyCode::Char('c'))
        {
            self.should_quit = true;
            return KeyOutcome::None;
        }

        match self.mode {
            Mode::Normal => self.handle_normal_key(key),
            Mode::Command => self.handle_command_key(key),
            Mode::Search => self.handle_search_key(key),
        }
    }

    fn handle_normal_key(&mut self, key: KeyEvent) -> KeyOutcome {
        // The `gg` chord: a leading `g` arms the chord; the next keystroke
        // either completes it (`g` → jump to top) or cancels.
        if self.pending_g {
            self.pending_g = false;
            if matches!(key.code, KeyCode::Char('g')) {
                self.set_selection(0);
                return KeyOutcome::None;
            }
        }
        let list_len = self.current_list_len();
        let max_idx = list_len.saturating_sub(1);

        match key.code {
            KeyCode::Char('q') => {
                self.should_quit = true;
            }
            KeyCode::Esc => {
                if !self.last_search.is_empty() {
                    self.last_search.clear();
                    self.status = "Search cleared.".into();
                } else {
                    self.should_quit = true;
                }
            }
            KeyCode::Char(':') => {
                self.mode = Mode::Command;
                self.command.clear();
            }
            KeyCode::Char('/') => {
                self.mode = Mode::Search;
                self.search.clear();
            }
            KeyCode::Char('?') => {
                self.tab = Tab::Help;
            }
            // Tab cycling.
            KeyCode::Tab | KeyCode::Char('l') => self.tab = next_tab(self.tab),
            KeyCode::BackTab | KeyCode::Char('h') => self.tab = prev_tab(self.tab),
            KeyCode::Char('1') => self.tab = Tab::Leaderboard,
            KeyCode::Char('2') => self.tab = Tab::Markets,
            KeyCode::Char('3') => self.tab = Tab::Portfolio,
            KeyCode::Char('4') => self.tab = Tab::Help,
            // Movement.
            KeyCode::Char('j') | KeyCode::Down => {
                let cur = self.selection();
                if cur < max_idx {
                    self.set_selection(cur + 1);
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                let cur = self.selection();
                if cur > 0 {
                    self.set_selection(cur - 1);
                }
            }
            KeyCode::Char('g') => {
                // First `g` of a possible `gg` chord — wait for the second key.
                self.pending_g = true;
            }
            KeyCode::Char('G') => {
                self.set_selection(max_idx);
            }
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let cur = self.selection();
                self.set_selection((cur + 10).min(max_idx));
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let cur = self.selection();
                self.set_selection(cur.saturating_sub(10));
            }
            KeyCode::Char('r') => return KeyOutcome::RefreshTab(self.tab),
            KeyCode::Char('R') => return KeyOutcome::RefreshAll,
            KeyCode::Char('n') => {
                // Jump to the next match. The list is already filtered to
                // matches, so this advances the selection, wrapping at the end.
                if !self.last_search.is_empty() && list_len > 0 {
                    let next = if self.selection() >= max_idx {
                        0
                    } else {
                        self.selection() + 1
                    };
                    self.set_selection(next);
                }
            }
            _ => {}
        }

        if self.tab == Tab::Help {
            return KeyOutcome::None;
        }
        // Clamp selection: filter changes or new data can shrink the list.
        let new_len = self.current_list_len();
        if new_len > 0 && self.selection() >= new_len {
            self.set_selection(new_len - 1);
        }
        KeyOutcome::None
    }

    fn handle_command_key(&mut self, key: KeyEvent) -> KeyOutcome {
        match key.code {
            KeyCode::Esc => {
                self.mode = Mode::Normal;
                self.command.clear();
            }
            KeyCode::Backspace => {
                if self.command.pop().is_none() {
                    // Backspace on an empty command line cancels — matches vim.
                    self.mode = Mode::Normal;
                }
            }
            KeyCode::Enter => {
                let cmd = std::mem::take(&mut self.command);
                self.mode = Mode::Normal;
                return self.execute_command(&cmd);
            }
            KeyCode::Char(c) => {
                self.command.push(c);
            }
            _ => {}
        }
        KeyOutcome::None
    }

    fn handle_search_key(&mut self, key: KeyEvent) -> KeyOutcome {
        match key.code {
            KeyCode::Esc => {
                self.mode = Mode::Normal;
                self.search.clear();
            }
            KeyCode::Backspace => {
                if self.search.pop().is_none() {
                    self.mode = Mode::Normal;
                }
            }
            KeyCode::Enter => {
                self.last_search = std::mem::take(&mut self.search);
                self.mode = Mode::Normal;
                self.set_selection(0);
                self.status = if self.last_search.is_empty() {
                    "Search cleared.".into()
                } else {
                    format!("Filter: /{}", self.last_search)
                };
            }
            KeyCode::Char(c) => {
                self.search.push(c);
            }
            _ => {}
        }
        KeyOutcome::None
    }

    fn execute_command(&mut self, cmd: &str) -> KeyOutcome {
        let cmd = cmd.trim();
        match cmd {
            "" => KeyOutcome::None,
            "q" | "quit" | "exit" => {
                self.should_quit = true;
                KeyOutcome::None
            }
            "h" | "help" => {
                self.tab = Tab::Help;
                KeyOutcome::None
            }
            "r" | "refresh" => KeyOutcome::RefreshTab(self.tab),
            "R" | "refresh!" => KeyOutcome::RefreshAll,
            "leaderboard" | "lb" => {
                self.tab = Tab::Leaderboard;
                KeyOutcome::None
            }
            "markets" | "m" => {
                self.tab = Tab::Markets;
                KeyOutcome::None
            }
            "portfolio" | "p" => {
                self.tab = Tab::Portfolio;
                KeyOutcome::None
            }
            other if other.starts_with("tab ") => {
                if let Ok(n) = other[4..].trim().parse::<usize>() {
                    if (1..=4).contains(&n) {
                        self.tab = Tab::from_index(n - 1);
                    } else {
                        self.status = format!("Unknown tab: {n} (1–4)");
                    }
                }
                KeyOutcome::None
            }
            other => {
                self.status = format!("Unknown command: :{other}");
                KeyOutcome::None
            }
        }
    }

    /// Called after every successful portfolio refresh — append the snapshot
    /// to the rolling window so the sparkline can draw.
    pub fn push_net_worth(&mut self, nw: f64) {
        if self.net_worth_history.len() == HISTORY_CAP {
            self.net_worth_history.pop_front();
        }
        self.net_worth_history.push_back(nw);
    }

    /// Apply a completed fetch to the corresponding `LoadState`.
    pub fn apply_fetch(&mut self, msg: FetchMsg) {
        let now = Instant::now();
        match msg {
            FetchMsg::Leaderboard(res) => {
                self.leaderboard = match res {
                    Ok(data) => LoadState::Loaded { data, at: now },
                    Err(e) => LoadState::Error(e),
                };
                // Clamp against the leaderboard specifically — this fetch can
                // land while the user is viewing a different tab.
                self.clamp_selection(Tab::Leaderboard);
            }
            FetchMsg::Markets(res) => {
                self.markets = match res {
                    Ok(data) => LoadState::Loaded { data, at: now },
                    Err(e) => LoadState::Error(e),
                };
                self.clamp_selection(Tab::Markets);
            }
            FetchMsg::Portfolio { portfolio, positions } => {
                self.portfolio = match portfolio {
                    Ok(p) => {
                        self.push_net_worth(p.net_worth);
                        LoadState::Loaded { data: p, at: now }
                    }
                    Err(e) => LoadState::Error(e),
                };
                self.positions = match positions {
                    Ok(data) => LoadState::Loaded { data, at: now },
                    Err(e) => LoadState::Error(e),
                };
                self.clamp_selection(Tab::Portfolio);
            }
        }
    }

    pub fn mark_loading(&mut self, tab: Tab) {
        match tab {
            Tab::Leaderboard => self.leaderboard = LoadState::Loading,
            Tab::Markets => self.markets = LoadState::Loading,
            Tab::Portfolio => {
                self.portfolio = LoadState::Loading;
                self.positions = LoadState::Loading;
            }
            Tab::Help => {}
        }
    }

    pub fn elapsed_since_load(&self, tab: Tab) -> Option<Duration> {
        let at = match tab {
            Tab::Leaderboard => self.leaderboard.loaded_at(),
            Tab::Markets => self.markets.loaded_at(),
            Tab::Portfolio => self.portfolio.loaded_at(),
            Tab::Help => None,
        }?;
        Some(at.elapsed())
    }
}

/// Outcomes that need to propagate to the event loop (which owns the runtime).
pub enum KeyOutcome {
    None,
    RefreshTab(Tab),
    RefreshAll,
}

/// Whether a market matches a lowercased search needle (question or category).
fn market_matches(m: &Market, needle: &str) -> bool {
    m.question.to_lowercase().contains(needle)
        || m.category
            .as_deref()
            .map(|c| c.to_lowercase().contains(needle))
            .unwrap_or(false)
}

fn next_tab(t: Tab) -> Tab {
    Tab::from_index((t.index() + 1) % Tab::ALL.len())
}
fn prev_tab(t: Tab) -> Tab {
    Tab::from_index((t.index() + Tab::ALL.len() - 1) % Tab::ALL.len())
}

// `fmt_money` lives in the shared `predlab-util` crate (also used by the admin
// TUI). Re-exported here so existing `crate::app::fmt_money` call sites stay put.
pub use predlab_util::fmt_money;

/// Compact human-readable age ("12s ago", "3m ago").
pub fn fmt_age(d: Duration) -> String {
    let s = d.as_secs();
    if s < 60 {
        format!("{s}s ago")
    } else if s < 3600 {
        format!("{}m ago", s / 60)
    } else {
        format!("{}h ago", s / 3600)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn k(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
    }

    #[test]
    fn money_groups_thousands() {
        assert_eq!(fmt_money(25_000.0), "$25,000.00");
        assert_eq!(fmt_money(1_234_567.5), "$1,234,567.50");
        assert_eq!(fmt_money(0.0), "$0.00");
        assert_eq!(fmt_money(-42.10), "-$42.10");
    }

    #[test]
    fn tab_indices_round_trip() {
        for t in Tab::ALL {
            assert_eq!(Tab::from_index(t.index()), t);
        }
    }

    #[test]
    fn vim_jk_navigates_within_bounds() {
        let mut app = App::new(false);
        app.leaderboard = LoadState::Loaded {
            data: vec![
                LeaderRow { username: "a".into(), net_worth: 1.0, rank: None },
                LeaderRow { username: "b".into(), net_worth: 2.0, rank: None },
                LeaderRow { username: "c".into(), net_worth: 3.0, rank: None },
            ],
            at: Instant::now(),
        };
        // j moves down
        app.handle_key(k('j'));
        assert_eq!(app.leaderboard_sel, 1);
        // k moves up
        app.handle_key(k('k'));
        assert_eq!(app.leaderboard_sel, 0);
        // k at top stays at 0
        app.handle_key(k('k'));
        assert_eq!(app.leaderboard_sel, 0);
        // G jumps to bottom
        app.handle_key(KeyEvent::new(KeyCode::Char('G'), KeyModifiers::NONE));
        assert_eq!(app.leaderboard_sel, 2);
        // gg jumps to top
        app.handle_key(k('g'));
        app.handle_key(k('g'));
        assert_eq!(app.leaderboard_sel, 0);
    }

    #[test]
    fn number_keys_jump_to_tab() {
        let mut app = App::new(false);
        app.handle_key(k('2'));
        assert_eq!(app.tab, Tab::Markets);
        app.handle_key(k('4'));
        assert_eq!(app.tab, Tab::Help);
        app.handle_key(k('1'));
        assert_eq!(app.tab, Tab::Leaderboard);
    }

    #[test]
    fn l_and_h_cycle_tabs() {
        let mut app = App::new(false);
        app.handle_key(k('l'));
        assert_eq!(app.tab, Tab::Markets);
        app.handle_key(k('l'));
        assert_eq!(app.tab, Tab::Portfolio);
        app.handle_key(k('h'));
        assert_eq!(app.tab, Tab::Markets);
    }

    #[test]
    fn colon_q_quits() {
        let mut app = App::new(false);
        app.handle_key(k(':'));
        assert_eq!(app.mode, Mode::Command);
        app.handle_key(k('q'));
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(app.should_quit);
        assert_eq!(app.mode, Mode::Normal);
    }

    #[test]
    fn unknown_command_reports_error() {
        let mut app = App::new(false);
        app.handle_key(k(':'));
        for c in "zxc".chars() {
            app.handle_key(k(c));
        }
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(app.status.starts_with("Unknown command"));
        assert!(!app.should_quit);
    }

    #[test]
    fn search_filters_leaderboard() {
        let mut app = App::new(false);
        app.leaderboard = LoadState::Loaded {
            data: vec![
                LeaderRow { username: "alice".into(), net_worth: 5.0, rank: None },
                LeaderRow { username: "bob".into(), net_worth: 4.0, rank: None },
                LeaderRow { username: "alicia".into(), net_worth: 3.0, rank: None },
            ],
            at: Instant::now(),
        };
        // / a l i Enter
        app.handle_key(k('/'));
        for c in "ali".chars() {
            app.handle_key(k(c));
        }
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        let filtered = app.filtered_leaderboard().unwrap();
        assert_eq!(filtered.len(), 2);
        assert_eq!(filtered[0].username, "alice");
        assert_eq!(filtered[1].username, "alicia");
    }

    #[test]
    fn esc_clears_search_before_quitting() {
        let mut app = App::new(false);
        app.last_search = "ali".into();
        app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(!app.should_quit);
        assert!(app.last_search.is_empty());
        // Second Esc with no search quits.
        app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(app.should_quit);
    }

    #[test]
    fn ctrl_c_quits_from_any_mode() {
        let mut app = App::new(false);
        app.mode = Mode::Command;
        app.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert!(app.should_quit);
    }

    #[test]
    fn fetch_clamps_its_own_tabs_selection_not_the_active_one() {
        let mut app = App::new(false);
        // Pretend the user had scrolled deep into a large markets list.
        app.tab = Tab::Markets;
        app.markets_sel = 40;
        // Now they're viewing the (shorter) Leaderboard tab.
        app.tab = Tab::Leaderboard;
        app.leaderboard = LoadState::Loaded {
            data: (0..100)
                .map(|i| LeaderRow { username: format!("u{i}"), net_worth: i as f64, rank: None })
                .collect(),
            at: Instant::now(),
        };
        // A background markets refresh lands with only 5 rows while we're on
        // the leaderboard. markets_sel must be clamped against the 5 markets,
        // not against the 100-row leaderboard.
        app.apply_fetch(FetchMsg::Markets(Ok(vec![Market::default(); 5])));
        assert_eq!(app.markets_sel, 4, "markets_sel clamped to its own length");
        // The leaderboard selection is untouched by a markets fetch.
        assert_eq!(app.leaderboard_sel, 0);
    }

    #[test]
    fn fmt_age_formats_into_buckets() {
        assert_eq!(fmt_age(Duration::from_secs(5)), "5s ago");
        assert_eq!(fmt_age(Duration::from_secs(125)), "2m ago");
        assert_eq!(fmt_age(Duration::from_secs(7300)), "2h ago");
    }
}
