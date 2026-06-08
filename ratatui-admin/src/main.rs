//! PredLab admin TUI (Polymarket paper trading).
//!
//! Three tabs. Switch with `l`/`h` (next/prev), or `Tab`/`Shift+Tab`. In the
//! Issue view `h`/`l` type into the username instead, so use `Tab` there.
//! - **Issue key** — type a username, pick a role with ←/→, press `Enter` to
//!   mint a paper API key on the simulator and save the member to the roster.
//!   The credentials are copied to your clipboard to hand to the member.
//! - **Roster** — browse club members (`j`/`k` or ↑/↓). `c` copies a member's
//!   credentials; `r` resets the selected member's balance to the starting
//!   amount, `R` resets *everyone* (start-of-competition wipe), and `x`
//!   permanently removes the selected member. Destructive actions ask for a
//!   `y` confirmation first.
//! - **Leaderboard** — every member ranked by paper net worth (live; `r` refreshes).
//!
//! Quit with `Esc` (or `q` from the roster/leaderboard). Endpoints/secrets come
//! from the environment so this works against a local sim or a deployed instance:
//!   POLY_URL (default http://localhost:8001),
//!   PREDLAB_ADMIN_SECRET (X-Admin-Secret for the Polymarket admin endpoints).

use std::io;
use std::io::Write as _;

use anyhow::{Context, Result};
// Shared formatting helpers (also used by predlab-tui). The shared `truncate`
// is char-boundary-safe, unlike the old byte-slicing copy this replaces.
use predlab_util::{fmt_money, truncate};
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState, Tabs},
    Frame, Terminal,
};
use ratatui_admin::registry::{self, Student};
use rusqlite::Connection;
use tokio::runtime::Runtime;

fn poly_url() -> String {
    std::env::var("POLY_URL").unwrap_or_else(|_| "http://localhost:8001".to_string())
}
fn admin_secret() -> String {
    std::env::var("PREDLAB_ADMIN_SECRET")
        .or_else(|_| std::env::var("ADMIN_SECRET"))
        .unwrap_or_default()
}

/// Roles the owner can grant, lowest → highest. Cycled with ←/→ in the Issue view.
const ROLES: [&str; 3] = ["member", "admin", "owner"];

/// One-line plain-English summary of what a role can do, shown under the picker.
fn role_blurb(role: &str) -> &'static str {
    match role {
        "admin" => "issue/revoke keys & reset balances — e.g. the VP",
        "owner" => "full control, incl. resolving markets & granting roles",
        _ => "trades & views only their own account (the default)",
    }
}

/// Credentials minted in the last successful issuance — kept on screen so the
/// admin can read them off / re-copy them.
#[derive(Clone)]
struct Issued {
    username: String,
    role: String,
    poly_key: String,
}

#[derive(PartialEq, Eq, Clone, Copy)]
enum View {
    Issue,
    Roster,
    Leaderboard,
}

/// A destructive admin action awaiting a `y` confirmation. These touch live
/// paper accounts, so we never fire them on a single keystroke.
#[derive(Clone, PartialEq, Eq)]
enum Pending {
    None,
    ResetMember(String),
    ResetAll,
    RemoveMember(String),
}

/// One row of the leaderboard: a member's paper net worth.
#[derive(Clone, Default)]
struct Leader {
    username: String,
    net_worth: f64,
}

struct App {
    view: View,
    username: String,
    role_idx: usize,
    message: String,
    students: Vec<Student>,
    roster_sel: usize,
    last: Option<Issued>,
    leaders: Vec<Leader>,
    pending: Pending,
    should_quit: bool,
}

impl App {
    fn new(students: Vec<Student>) -> Self {
        Self {
            view: View::Issue,
            username: String::new(),
            role_idx: 0,
            message: "Type a username, pick a role with ←/→, then press Enter.".into(),
            students,
            roster_sel: 0,
            last: None,
            leaders: Vec::new(),
            pending: Pending::None,
            should_quit: false,
        }
    }

    fn role(&self) -> &'static str {
        ROLES[self.role_idx]
    }
}

fn main() -> Result<()> {
    let conn = registry::open(&registry::default_db_path())
        .context("opening ~/.predlab/students.db")?;
    let students = registry::list_students(&conn).unwrap_or_default();

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(students);
    let rt = Runtime::new()?;
    let res = run_app(&mut terminal, &mut app, &rt, &conn);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        eprintln!("Error: {err:?}");
    }
    Ok(())
}

fn run_app<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
    rt: &Runtime,
    conn: &Connection,
) -> Result<()> {
    loop {
        terminal.draw(|f| ui(f, app))?;

        if event::poll(std::time::Duration::from_millis(200))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                // A destructive action is awaiting confirmation: `y` executes it,
                // anything else (incl. Tab/Esc) cancels.
                if app.pending != Pending::None {
                    let pending = std::mem::replace(&mut app.pending, Pending::None);
                    if matches!(key.code, KeyCode::Char('y') | KeyCode::Char('Y')) {
                        confirm_pending(app, rt, conn, pending);
                    } else {
                        app.message = "Cancelled.".into();
                    }
                    continue;
                }
                // Global keys (work in any view).
                match key.code {
                    KeyCode::Esc => {
                        app.should_quit = true;
                        continue;
                    }
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        app.should_quit = true;
                        continue;
                    }
                    KeyCode::Tab => {
                        switch_view(app, rt, next_view(app.view));
                        continue;
                    }
                    KeyCode::BackTab => {
                        switch_view(app, rt, prev_view(app.view));
                        continue;
                    }
                    // Vim-style tab switching. Skipped in the Issue view, where
                    // h/l are letters the user is typing into a username.
                    KeyCode::Char('l') if app.view != View::Issue => {
                        switch_view(app, rt, next_view(app.view));
                        continue;
                    }
                    KeyCode::Char('h') if app.view != View::Issue => {
                        switch_view(app, rt, prev_view(app.view));
                        continue;
                    }
                    _ => {}
                }

                match app.view {
                    View::Issue => handle_issue_key(app, rt, conn, key.code),
                    View::Roster => handle_roster_key(app, key.code),
                    View::Leaderboard => handle_leaderboard_key(app, rt, key.code),
                }
            }
        }

        if app.should_quit {
            break;
        }
    }
    Ok(())
}

/// Tab order, forward (l / Tab) and backward (h / Shift+Tab).
fn next_view(v: View) -> View {
    match v {
        View::Issue => View::Roster,
        View::Roster => View::Leaderboard,
        View::Leaderboard => View::Issue,
    }
}
fn prev_view(v: View) -> View {
    match v {
        View::Issue => View::Leaderboard,
        View::Roster => View::Issue,
        View::Leaderboard => View::Roster,
    }
}

/// Switch to `target`, keeping the roster selection valid and auto-refreshing
/// the leaderboard when it becomes visible.
fn switch_view(app: &mut App, rt: &Runtime, target: View) {
    app.view = target;
    clamp_selection(app);
    if app.view == View::Leaderboard {
        refresh_leaderboard(app, rt);
    }
}

fn handle_issue_key(app: &mut App, rt: &Runtime, conn: &Connection, code: KeyCode) {
    match code {
        KeyCode::Enter => issue_and_save(app, rt, conn),
        KeyCode::Left | KeyCode::Up => {
            app.role_idx = (app.role_idx + ROLES.len() - 1) % ROLES.len();
        }
        KeyCode::Right | KeyCode::Down => {
            app.role_idx = (app.role_idx + 1) % ROLES.len();
        }
        KeyCode::Backspace => {
            app.username.pop();
        }
        // Usernames may contain any letter/number/underscore — including 'q'/'c'.
        KeyCode::Char(c) if c.is_ascii_alphanumeric() || c == '_' => {
            app.username.push(c);
        }
        _ => {}
    }
}

fn handle_roster_key(app: &mut App, code: KeyCode) {
    match code {
        KeyCode::Char('q') => app.should_quit = true,
        KeyCode::Up | KeyCode::Char('k') => app.roster_sel = app.roster_sel.saturating_sub(1),
        KeyCode::Down | KeyCode::Char('j') => {
            if app.roster_sel + 1 < app.students.len() {
                app.roster_sel += 1;
            }
        }
        KeyCode::Char('c') => copy_selected_member(app),
        KeyCode::Char('r') => request_reset_member(app),
        KeyCode::Char('R') => request_reset_all(app),
        KeyCode::Char('x') | KeyCode::Delete => request_remove_member(app),
        _ => {}
    }
}

fn selected_username(app: &App) -> Option<String> {
    app.students.get(app.roster_sel).map(|s| s.username.clone())
}

/// Stage a single-member balance reset, pending `y` confirmation.
fn request_reset_member(app: &mut App) {
    match selected_username(app) {
        Some(u) => {
            app.message =
                format!("⚠ Reset {u} to the starting balance? Press y to confirm.");
            app.pending = Pending::ResetMember(u);
        }
        None => app.message = "Roster is empty — nothing to reset.".into(),
    }
}

/// Stage a wipe of every member's balance, pending `y` confirmation.
fn request_reset_all(app: &mut App) {
    app.message = "⚠ Reset ALL members to the starting balance? Press y to confirm.".into();
    app.pending = Pending::ResetAll;
}

/// Stage permanent removal of a member, pending `y` confirmation.
fn request_remove_member(app: &mut App) {
    match selected_username(app) {
        Some(u) => {
            app.message =
                format!("⚠ PERMANENTLY remove {u} from the sim and roster? Press y to confirm.");
            app.pending = Pending::RemoveMember(u);
        }
        None => app.message = "Roster is empty — nothing to remove.".into(),
    }
}

/// Execute a confirmed destructive action against the live sim.
fn confirm_pending(app: &mut App, rt: &Runtime, conn: &Connection, pending: Pending) {
    match pending {
        Pending::ResetMember(u) => {
            app.message = format!("Resetting {u}…");
            app.message = match rt.block_on(reset_member(&u)) {
                Ok(msg) => msg,
                Err(e) => format!("❌ {e}"),
            };
        }
        Pending::ResetAll => {
            app.message = "Resetting all members…".into();
            app.message = match rt.block_on(reset_all()) {
                Ok(msg) => msg,
                Err(e) => format!("❌ {e}"),
            };
        }
        Pending::RemoveMember(u) => {
            app.message = format!("Removing {u}…");
            app.message = match rt.block_on(remove_member(&u)) {
                Ok(msg) => {
                    let _ = registry::delete_student(conn, &u);
                    app.students = registry::list_students(conn).unwrap_or_default();
                    clamp_selection(app);
                    msg
                }
                Err(e) => format!("❌ {e}"),
            };
        }
        Pending::None => {}
    }
}

fn clamp_selection(app: &mut App) {
    if app.roster_sel >= app.students.len() {
        app.roster_sel = app.students.len().saturating_sub(1);
    }
}

fn copy_selected_member(app: &mut App) {
    let Some(s) = app.students.get(app.roster_sel) else {
        app.message = "Roster is empty — issue a key first.".into();
        return;
    };
    let block = creds_block(&s.username, &s.role, &s.poly_key);
    app.message = if copy_to_clipboard(&block) {
        format!("📋 Copied {}'s credentials to the clipboard.", s.username)
    } else {
        format!("Couldn't reach wl-copy — credentials for {} not copied.", s.username)
    };
}

fn handle_leaderboard_key(app: &mut App, rt: &Runtime, code: KeyCode) {
    match code {
        KeyCode::Char('q') => app.should_quit = true,
        KeyCode::Char('r') => refresh_leaderboard(app, rt),
        _ => {}
    }
}

fn refresh_leaderboard(app: &mut App, rt: &Runtime) {
    app.message = "Loading leaderboard…".into();
    match rt.block_on(fetch_leaderboard()) {
        Ok(rows) => {
            app.message = format!("{} members ranked by net worth. Press r to refresh.", rows.len());
            app.leaders = rows;
        }
        Err(e) => app.message = format!("❌ {e}"),
    }
}

/// Fetch the sim's per-user net worth, ranked highest first.
async fn fetch_leaderboard() -> Result<Vec<Leader>> {
    let rows: Vec<serde_json::Value> = reqwest::Client::new()
        .get(format!("{}/admin/leaderboard", poly_url()))
        .header("X-Admin-Secret", admin_secret())
        .send()
        .await
        .context("calling polymarket /admin/leaderboard")?
        .error_for_status()
        .context("polymarket leaderboard rejected (check PREDLAB_ADMIN_SECRET)")?
        .json()
        .await
        .context("parsing polymarket leaderboard")?;

    let mut leaders: Vec<Leader> = rows
        .iter()
        .filter_map(|e| {
            Some(Leader {
                username: e.get("username").and_then(|v| v.as_str())?.to_string(),
                net_worth: e.get("net_worth").and_then(|n| n.as_f64()).unwrap_or(0.0),
            })
        })
        .collect();
    leaders.sort_by(|a, b| {
        b.net_worth
            .partial_cmp(&a.net_worth)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    Ok(leaders)
}

/// POST an admin action to the sim. `tolerate_404` makes removal idempotent.
async fn post_admin(query: &[(&str, &str)], path: &str, tolerate_404: bool) -> Result<()> {
    let url = format!("{}{path}", poly_url());
    let resp = reqwest::Client::new()
        .post(&url)
        .header("X-Admin-Secret", admin_secret())
        .query(query)
        .send()
        .await
        .with_context(|| format!("calling {url}"))?;
    if tolerate_404 && resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(());
    }
    resp.error_for_status()
        .with_context(|| format!("{url} rejected (check PREDLAB_ADMIN_SECRET / role)"))?;
    Ok(())
}

/// Reset one member to the starting balance (clears positions/orders).
async fn reset_member(username: &str) -> Result<String> {
    post_admin(&[("username", username)], "/admin/reset-balance", false).await?;
    Ok(format!("♻ Reset {username} to the starting balance."))
}

/// Reset every member to the starting balance.
async fn reset_all() -> Result<String> {
    post_admin(&[], "/admin/reset-balance", false).await?;
    Ok("♻ Reset ALL members to the starting balance.".to_string())
}

/// Permanently delete a member from the sim.
async fn remove_member(username: &str) -> Result<String> {
    post_admin(&[("username", username)], "/admin/delete-user", true).await?;
    Ok(format!("🗑 Removed {username} from the sim and the roster."))
}

fn issue_and_save(app: &mut App, rt: &Runtime, conn: &Connection) {
    let username = app.username.trim().to_string();
    if username.is_empty() {
        app.message = "Enter a username first.".into();
        return;
    }
    let role = app.role().to_string();
    app.message = format!("Issuing a {role} key for '{username}'…");

    match rt.block_on(issue_poly(&username, &role)) {
        Ok(poly_key) => {
            let student = Student {
                username: username.clone(),
                display_name: username.clone(),
                poly_key: poly_key.clone(),
                kalshi_key: String::new(), // legacy column, unused
                role: role.clone(),
                created_at: chrono::Utc::now().to_rfc3339(),
            };
            match registry::save_student(conn, &student) {
                Ok(()) => {
                    app.students = registry::list_students(conn).unwrap_or_default();
                    let block = creds_block(&username, &role, &poly_key);
                    let copied = copy_to_clipboard(&block);
                    app.message = if copied {
                        format!("✅ Issued a {role} key for {username} — credentials copied to clipboard.")
                    } else {
                        format!("✅ Issued a {role} key for {username}. (Install wl-copy to auto-copy.)")
                    };
                    app.last = Some(Issued { username, role, poly_key });
                    app.username.clear();
                }
                Err(e) => app.message = format!("Issued, but failed to save to roster: {e}"),
            }
        }
        Err(e) => app.message = format!("❌ {e}"),
    }
}

/// Mint a Polymarket paper key with role `role`. Returns the API key.
async fn issue_poly(username: &str, role: &str) -> Result<String> {
    let resp = reqwest::Client::new()
        .post(format!("{}/admin/create-paper-key", poly_url()))
        .header("X-Admin-Secret", admin_secret())
        .query(&[
            ("username", username),
            ("display_name", username),
            ("role", role),
        ])
        .send()
        .await
        .context("calling polymarket admin/create-paper-key")?;
    let json: serde_json::Value = resp.json().await.context("parsing response")?;
    json.get("api_key")
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .context("response missing api_key (check PREDLAB_ADMIN_SECRET / role)")
}

/// A copy-pasteable credentials block to hand to a member (e.g. via DM).
fn creds_block(username: &str, role: &str, poly_key: &str) -> String {
    format!(
        "PredLab credentials for {username} (role: {role})\n\
         \n\
         base URL: {}\n  API key:  {poly_key}\n\
         → send the key as the POLY_API_KEY header\n\
         \n\
         Quick start: examples/predlab.py (set POLY_KEY and trade).\n",
        poly_url(),
    )
}

/// Best-effort clipboard copy via wl-copy (Wayland). Returns false if unavailable.
fn copy_to_clipboard(text: &str) -> bool {
    use std::process::{Command, Stdio};
    let Ok(mut child) = Command::new("wl-copy").stdin(Stdio::piped()).spawn() else {
        return false;
    };
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(text.as_bytes());
        // stdin dropped here → wl-copy sees EOF and stores the selection.
    }
    child.wait().map(|s| s.success()).unwrap_or(false)
}

fn ui(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // tabs
            Constraint::Min(7),    // body
            Constraint::Length(4), // status + help
        ])
        .split(f.area());

    let tab_index = match app.view {
        View::Issue => 0,
        View::Roster => 1,
        View::Leaderboard => 2,
    };
    let tabs = Tabs::new(vec!["ISSUE KEY", "ROSTER", "LEADERBOARD"])
        .block(Block::default().borders(Borders::ALL).title("PREDLAB ADMIN"))
        .select(tab_index)
        .highlight_style(
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        );
    f.render_widget(tabs, chunks[0]);

    match app.view {
        View::Issue => render_issue(f, app, chunks[1]),
        View::Roster => render_roster(f, app, chunks[1]),
        View::Leaderboard => render_leaderboard(f, app, chunks[1]),
    }

    render_footer(f, app, chunks[2]);
}

fn render_issue(f: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let body = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(7), Constraint::Min(0)])
        .split(area);

    // --- form: username + role picker ---
    let mut role_spans = vec![Span::raw("Role:      ")];
    for (i, r) in ROLES.iter().enumerate() {
        let style = if i == app.role_idx {
            Style::default()
                .fg(Color::Black)
                .bg(Color::Green)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        role_spans.push(Span::styled(format!(" {r} "), style));
        role_spans.push(Span::raw(" "));
    }

    let form = Paragraph::new(vec![
        Line::from(format!("Username:  {}▏", app.username)),
        Line::from(""),
        Line::from(role_spans),
        Line::from(Span::styled(
            format!("           {}", role_blurb(app.role())),
            Style::default().fg(Color::DarkGray),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "           ↵ Enter mints a paper key and saves the member.",
            Style::default().fg(Color::Cyan),
        )),
    ])
    .block(
        Block::default()
            .title("Issue a paper key")
            .borders(Borders::ALL),
    );
    f.render_widget(form, body[0]);

    // --- last issued credentials ---
    let result = match &app.last {
        Some(l) => vec![
            Line::from(Span::styled(
                format!("Last issued: {} [{}]", l.username, l.role),
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(format!("  API key: {}", l.poly_key)),
            Line::from(Span::styled(
                "  → send as the POLY_API_KEY header",
                Style::default().fg(Color::DarkGray),
            )),
        ],
        None => vec![Line::from(Span::styled(
            "Issued credentials will appear here and copy to your clipboard.",
            Style::default().fg(Color::DarkGray),
        ))],
    };
    let result = Paragraph::new(result).block(
        Block::default()
            .title("Result")
            .borders(Borders::ALL),
    );
    f.render_widget(result, body[1]);
}

fn render_roster(f: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let header = Row::new(["USERNAME", "ROLE", "API KEY", "CREATED"]).style(
        Style::default()
            .fg(Color::Green)
            .add_modifier(Modifier::BOLD),
    );
    let rows: Vec<Row> = app
        .students
        .iter()
        .map(|s| {
            Row::new(vec![
                Cell::from(s.username.clone()),
                Cell::from(s.role.clone()),
                Cell::from(truncate(&s.poly_key, 28)),
                Cell::from(truncate(&s.created_at, 19)),
            ])
        })
        .collect();
    let widths = [
        Constraint::Percentage(26),
        Constraint::Percentage(14),
        Constraint::Percentage(38),
        Constraint::Percentage(22),
    ];
    let table = Table::new(rows, widths)
        .header(header)
        .row_highlight_style(
            Style::default()
                .bg(Color::Green)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ")
        .block(
            Block::default()
                .title(format!("Club roster ({} members)", app.students.len()))
                .borders(Borders::ALL),
        );

    let mut state = TableState::default();
    if !app.students.is_empty() {
        state.select(Some(app.roster_sel.min(app.students.len() - 1)));
    }
    f.render_stateful_widget(table, area, &mut state);
}

fn render_leaderboard(f: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let header = Row::new(["#", "MEMBER", "NET WORTH"]).style(
        Style::default()
            .fg(Color::Green)
            .add_modifier(Modifier::BOLD),
    );
    let rows: Vec<Row> = app
        .leaders
        .iter()
        .enumerate()
        .map(|(i, l)| {
            // Highlight the leader.
            let style = if i == 0 {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            Row::new(vec![
                Cell::from(format!("{}", i + 1)),
                Cell::from(l.username.clone()),
                Cell::from(fmt_money(l.net_worth)),
            ])
            .style(style)
        })
        .collect();
    let widths = [
        Constraint::Length(4),
        Constraint::Percentage(55),
        Constraint::Percentage(40),
    ];
    let title = if app.leaders.is_empty() {
        "Leaderboard — press r to load".to_string()
    } else {
        format!("Leaderboard — paper net worth ({} members)", app.leaders.len())
    };
    let table = Table::new(rows, widths)
        .header(header)
        .block(Block::default().title(title).borders(Borders::ALL));
    f.render_widget(table, area);
}

fn render_footer(f: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let help = match app.view {
        View::Issue => "Tab/⇧Tab switch tab · ←/→ role · Enter issue · Esc quit",
        View::Roster => "h/l tab · j/k select · c copy · r reset · x remove · R reset-all · q quit",
        View::Leaderboard => "h/l tab · r refresh · q/Esc quit",
    };
    let footer = Paragraph::new(vec![
        Line::from(Span::styled(
            app.message.clone(),
            Style::default().fg(Color::Yellow),
        )),
        Line::from(Span::styled(help, Style::default().fg(Color::DarkGray))),
    ])
    .block(Block::default().title("Status").borders(Borders::ALL));
    f.render_widget(footer, area);
}
