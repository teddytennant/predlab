//! PredLab admin TUI.
//!
//! Two tabs (switch with `Tab`):
//! - **Issue keys** — type a username, pick a role with ←/→, press `Enter` to
//!   mint paper keys on *both* simulators and save the member to the roster.
//!   The freshly issued credentials are copied to your clipboard, and the
//!   Kalshi private key (shown only once by the server) is saved to
//!   `~/.predlab/keys/<username>.pem` so the member can actually sign requests.
//! - **Roster** — browse club members (↑/↓); press `c` to copy a member's
//!   credentials block to the clipboard.
//! - **Leaderboard** — every member ranked by combined paper net worth across
//!   both sims (live; press `r` to refresh).
//!
//! Quit with `Esc` (or `q` from the roster/leaderboard). Endpoints/secrets come from the
//! environment so this works against local sims or a deployed instance:
//!   POLY_URL (default http://localhost:8001), KALSHI_URL (default :8002),
//!   PREDLAB_ADMIN_SECRET  (X-Admin-Secret for the Polymarket admin endpoint),
//!   PREDLAB_KALSHI_SECRET (X-Kalshi-Sim-Admin for the Kalshi generate endpoint;
//!                          falls back to CLUB_ADMIN_SECRET).

use std::io;
use std::io::Write as _;

use anyhow::{Context, Result};
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
fn kalshi_url() -> String {
    std::env::var("KALSHI_URL").unwrap_or_else(|_| "http://localhost:8002".to_string())
}
fn admin_secret() -> String {
    std::env::var("PREDLAB_ADMIN_SECRET")
        .or_else(|_| std::env::var("ADMIN_SECRET"))
        .unwrap_or_default()
}
/// Kalshi's master secret (separate from Polymarket's). Issuing Kalshi keys now
/// requires it. Falls back to CLUB_ADMIN_SECRET.
fn kalshi_admin_secret() -> String {
    std::env::var("PREDLAB_KALSHI_SECRET")
        .or_else(|_| std::env::var("CLUB_ADMIN_SECRET"))
        .unwrap_or_default()
}

/// Roles the owner can grant, lowest → highest. Cycled with ←/→ in the Issue view.
const ROLES: [&str; 3] = ["member", "admin", "owner"];

/// One-line plain-English summary of what a role can do, shown under the picker.
fn role_blurb(role: &str) -> &'static str {
    match role {
        "admin" => "issue/revoke member keys & reset balances — e.g. the VP",
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
    kalshi_key_id: String,
    pem_path: Option<String>,
}

#[derive(PartialEq, Eq, Clone, Copy)]
enum View {
    Issue,
    Roster,
    Leaderboard,
}

/// One row of the combined leaderboard: a member's net worth on each sim.
#[derive(Clone, Default)]
struct Leader {
    username: String,
    poly: f64,
    kalshi: f64,
    total: f64,
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
                        app.view = match app.view {
                            View::Issue => View::Roster,
                            View::Roster => View::Leaderboard,
                            View::Leaderboard => View::Issue,
                        };
                        clamp_selection(app);
                        if app.view == View::Leaderboard {
                            refresh_leaderboard(app, rt);
                        }
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
        KeyCode::Up => app.roster_sel = app.roster_sel.saturating_sub(1),
        KeyCode::Down => {
            if app.roster_sel + 1 < app.students.len() {
                app.roster_sel += 1;
            }
        }
        KeyCode::Char('c') => copy_selected_member(app),
        _ => {}
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
    let pem = pem_path_for(&s.username);
    let block = creds_block(
        &s.username,
        &s.role,
        &s.poly_key,
        &s.kalshi_key,
        std::path::Path::new(&pem).exists().then_some(pem.as_str()),
    );
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
            app.message = format!(
                "{} members ranked by combined net worth. Press r to refresh.",
                rows.len()
            );
            app.leaders = rows;
        }
        Err(e) => app.message = format!("❌ {e}"),
    }
}

/// Fetch both sims' per-user net worth and merge into a combined ranking.
async fn fetch_leaderboard() -> Result<Vec<Leader>> {
    use std::collections::BTreeMap;

    let client = reqwest::Client::new();
    let net = |v: &serde_json::Value| v.get("net_worth").and_then(|n| n.as_f64()).unwrap_or(0.0);

    let poly: Vec<serde_json::Value> = client
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

    let kalshi: Vec<serde_json::Value> = client
        .get(format!("{}/trade-api/v2/admin/leaderboard", kalshi_url()))
        .header("X-Kalshi-Sim-Admin", kalshi_admin_secret())
        .send()
        .await
        .context("calling kalshi /admin/leaderboard")?
        .error_for_status()
        .context("kalshi leaderboard rejected (check PREDLAB_KALSHI_SECRET)")?
        .json()
        .await
        .context("parsing kalshi leaderboard")?;

    // Merge by username; a member may exist on one sim only.
    let mut by_user: BTreeMap<String, Leader> = BTreeMap::new();
    for e in &poly {
        if let Some(u) = e.get("username").and_then(|v| v.as_str()) {
            by_user.entry(u.to_string()).or_default().poly = net(e);
        }
    }
    for e in &kalshi {
        if let Some(u) = e.get("username").and_then(|v| v.as_str()) {
            by_user.entry(u.to_string()).or_default().kalshi = net(e);
        }
    }

    let mut rows: Vec<Leader> = by_user
        .into_iter()
        .map(|(username, mut l)| {
            l.username = username;
            l.total = l.poly + l.kalshi;
            l
        })
        .collect();
    rows.sort_by(|a, b| b.total.partial_cmp(&a.total).unwrap_or(std::cmp::Ordering::Equal));
    Ok(rows)
}

fn issue_and_save(app: &mut App, rt: &Runtime, conn: &Connection) {
    let username = app.username.trim().to_string();
    if username.is_empty() {
        app.message = "Enter a username first.".into();
        return;
    }
    let role = app.role().to_string();
    app.message = format!("Issuing {role} keys for '{username}'…");

    match rt.block_on(issue_both(&username, &role)) {
        Ok((poly_key, kalshi_key_id, kalshi_private_key)) => {
            // Persist the Kalshi private key — the server only returns it once.
            let pem_path = if kalshi_private_key.is_empty() {
                None
            } else {
                save_pem(&username, &kalshi_private_key).ok()
            };

            let student = Student {
                username: username.clone(),
                display_name: username.clone(),
                poly_key: poly_key.clone(),
                kalshi_key: kalshi_key_id.clone(),
                role: role.clone(),
                created_at: chrono::Utc::now().to_rfc3339(),
            };
            match registry::save_student(conn, &student) {
                Ok(()) => {
                    app.students = registry::list_students(conn).unwrap_or_default();
                    let block = creds_block(
                        &username,
                        &role,
                        &poly_key,
                        &kalshi_key_id,
                        pem_path.as_deref(),
                    );
                    let copied = copy_to_clipboard(&block);
                    app.message = if copied {
                        format!("✅ Issued {role} keys for {username} — credentials copied to clipboard.")
                    } else {
                        format!("✅ Issued {role} keys for {username}. (Install wl-copy to auto-copy.)")
                    };
                    app.last = Some(Issued {
                        username,
                        role,
                        poly_key,
                        kalshi_key_id,
                        pem_path,
                    });
                    app.username.clear();
                }
                Err(e) => app.message = format!("Issued, but failed to save to roster: {e}"),
            }
        }
        Err(e) => app.message = format!("❌ {e}"),
    }
}

/// Mint a paper key with role `role` on each simulator.
/// Returns (polymarket_key, kalshi_key_id, kalshi_private_key_pem).
async fn issue_both(username: &str, role: &str) -> Result<(String, String, String)> {
    let client = reqwest::Client::new();

    // Polymarket: admin endpoint, gated by X-Admin-Secret (master secret = owner).
    let poly_resp = client
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
    let poly_json: serde_json::Value = poly_resp.json().await.context("parsing poly response")?;
    let poly_key = poly_json
        .get("api_key")
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .context("polymarket response missing api_key (check PREDLAB_ADMIN_SECRET / role)")?;

    // Kalshi: generate endpoint (admin-gated) returns an RSA keypair. The private
    // key is shown ONLY here, so we capture it for the member.
    let kalshi_resp = client
        .post(format!("{}/trade-api/v2/api_keys/generate", kalshi_url()))
        .header("X-Kalshi-Sim-Admin", kalshi_admin_secret())
        .query(&[("username", username), ("role", role)])
        .json(&serde_json::json!({ "name": username }))
        .send()
        .await
        .context("calling kalshi api_keys/generate")?;
    let kalshi_json: serde_json::Value =
        kalshi_resp.json().await.context("parsing kalshi response")?;
    let kalshi_key_id = kalshi_json
        .get("api_key_id")
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .context("kalshi response missing api_key_id (check PREDLAB_KALSHI_SECRET / role)")?;
    let kalshi_private_key = kalshi_json
        .get("private_key")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();

    Ok((poly_key, kalshi_key_id, kalshi_private_key))
}

/// `~/.predlab/keys/<username>.pem` — where a member's Kalshi private key lives.
fn pem_path_for(username: &str) -> String {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    std::path::Path::new(&home)
        .join(".predlab")
        .join("keys")
        .join(format!("{username}.pem"))
        .to_string_lossy()
        .into_owned()
}

/// Write a member's Kalshi private key to a 0600 file and return its path.
fn save_pem(username: &str, pem: &str) -> Result<String> {
    let path = pem_path_for(username);
    if let Some(parent) = std::path::Path::new(&path).parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, pem)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(path)
}

/// A copy-pasteable credentials block to hand to a member (e.g. via DM).
fn creds_block(
    username: &str,
    role: &str,
    poly_key: &str,
    kalshi_key_id: &str,
    pem_path: Option<&str>,
) -> String {
    let mut s = format!(
        "PredLab credentials for {username} (role: {role})\n\
         \n\
         Polymarket\n  base URL: {}\n  API key:  {poly_key}\n  → send as the POLY_API_KEY header\n\
         \n\
         Kalshi\n  base URL: {}/trade-api/v2\n  key id:   {kalshi_key_id}\n",
        poly_url(),
        kalshi_url(),
    );
    match pem_path {
        Some(p) => s.push_str(&format!(
            "  private key file: {p}\n  → needed to sign requests; send this file securely\n"
        )),
        None => s.push_str("  (no private key captured — re-issue to get one)\n"),
    }
    s
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
    let tabs = Tabs::new(vec!["ISSUE KEYS", "ROSTER", "LEADERBOARD"])
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
            "           ↵ Enter mints keys on both sims and saves the member.",
            Style::default().fg(Color::Cyan),
        )),
    ])
    .block(
        Block::default()
            .title("Issue dual paper keys")
            .borders(Borders::ALL),
    );
    f.render_widget(form, body[0]);

    // --- last issued credentials ---
    let result = match &app.last {
        Some(l) => {
            let mut lines = vec![
                Line::from(Span::styled(
                    format!("Last issued: {} [{}]", l.username, l.role),
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                )),
                Line::from(format!("  Polymarket key:  {}", l.poly_key)),
                Line::from(format!("  Kalshi key id:   {}", l.kalshi_key_id)),
            ];
            lines.push(match &l.pem_path {
                Some(p) => Line::from(format!("  Kalshi key file: {p}")),
                None => Line::from("  Kalshi private key: (not captured)"),
            });
            lines
        }
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
    let header = Row::new(["USERNAME", "ROLE", "POLY KEY", "KALSHI KEY", "CREATED"])
        .style(
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
                Cell::from(truncate(&s.poly_key, 16)),
                Cell::from(truncate(&s.kalshi_key, 16)),
                Cell::from(truncate(&s.created_at, 19)),
            ])
        })
        .collect();
    let widths = [
        Constraint::Percentage(22),
        Constraint::Percentage(12),
        Constraint::Percentage(25),
        Constraint::Percentage(25),
        Constraint::Percentage(16),
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
    let header = Row::new(["#", "MEMBER", "POLYMARKET", "KALSHI", "TOTAL"]).style(
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
                Cell::from(fmt_money(l.poly)),
                Cell::from(fmt_money(l.kalshi)),
                Cell::from(fmt_money(l.total)),
            ])
            .style(style)
        })
        .collect();
    let widths = [
        Constraint::Length(4),
        Constraint::Percentage(28),
        Constraint::Percentage(23),
        Constraint::Percentage(23),
        Constraint::Percentage(23),
    ];
    let title = if app.leaders.is_empty() {
        "Leaderboard — press r to load".to_string()
    } else {
        format!("Leaderboard — combined net worth ({} members)", app.leaders.len())
    };
    let table = Table::new(rows, widths)
        .header(header)
        .block(Block::default().title(title).borders(Borders::ALL));
    f.render_widget(table, area);
}

/// Format a dollar amount with thousands separators, e.g. 25431.5 -> "$25,431.50".
fn fmt_money(v: f64) -> String {
    let neg = v < 0.0;
    let cents = (v.abs() * 100.0).round() as u64;
    let whole = cents / 100;
    let frac = cents % 100;
    let digits = whole.to_string();
    let bytes = digits.as_bytes();
    let mut grouped = String::new();
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 && (bytes.len() - i).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(*b as char);
    }
    format!("{}${}.{:02}", if neg { "-" } else { "" }, grouped, frac)
}

fn render_footer(f: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let help = match app.view {
        View::Issue => "Tab next · ←/→ role · Enter issue · Esc quit",
        View::Roster => "Tab next · ↑/↓ select · c copy creds · q/Esc quit",
        View::Leaderboard => "Tab next · r refresh · q/Esc quit",
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

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max.saturating_sub(1)])
    }
}
