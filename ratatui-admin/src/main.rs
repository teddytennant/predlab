//! PredLab admin TUI.
//!
//! Two views (toggle with `Tab`):
//! - **Issue**: type a username, press `Enter` to mint paper keys on *both*
//!   simulators and persist the member to the shared roster.
//! - **Roster**: the club's students (from `~/.predlab/students.db`).
//!
//! Pick a role with ↑/↓ before issuing (member/admin/owner) — granting
//! admin/owner requires an owner-level secret on the servers.
//!
//! Endpoints / secrets are read from the environment so this works against
//! local sims or a deployed instance:
//!   POLY_URL (default http://localhost:8001), KALSHI_URL (default :8002),
//!   PREDLAB_ADMIN_SECRET  (X-Admin-Secret for the Polymarket admin endpoint),
//!   PREDLAB_KALSHI_SECRET (X-Kalshi-Sim-Admin for the Kalshi generate endpoint;
//!                          falls back to CLUB_ADMIN_SECRET).

use std::io;

use anyhow::{Context, Result};
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table, Tabs},
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

/// Roles the owner can grant, lowest → highest. Cycled with ↑/↓ in the Issue view.
const ROLES: [&str; 3] = ["member", "admin", "owner"];

#[derive(PartialEq, Eq, Clone, Copy)]
enum View {
    Issue,
    Roster,
}

struct App {
    view: View,
    username: String,
    role_idx: usize,
    message: String,
    students: Vec<Student>,
    should_quit: bool,
}

impl App {
    fn new(students: Vec<Student>) -> Self {
        Self {
            view: View::Issue,
            username: String::new(),
            role_idx: 0,
            message: "Type a username · ↑/↓ role · Enter issue · Tab roster · q quit".into(),
            students,
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
                match key.code {
                    KeyCode::Char('q') => app.should_quit = true,
                    KeyCode::Tab => {
                        app.view = match app.view {
                            View::Issue => View::Roster,
                            View::Roster => View::Issue,
                        };
                    }
                    KeyCode::Enter if app.view == View::Issue => {
                        issue_and_save(app, rt, conn);
                    }
                    KeyCode::Up if app.view == View::Issue => {
                        app.role_idx = (app.role_idx + 1) % ROLES.len();
                    }
                    KeyCode::Down if app.view == View::Issue => {
                        app.role_idx = (app.role_idx + ROLES.len() - 1) % ROLES.len();
                    }
                    KeyCode::Char(c)
                        if app.view == View::Issue && (c.is_ascii_alphanumeric() || c == '_') =>
                    {
                        app.username.push(c);
                    }
                    KeyCode::Backspace if app.view == View::Issue => {
                        app.username.pop();
                    }
                    _ => {}
                }
            }
        }

        if app.should_quit {
            break;
        }
    }
    Ok(())
}

fn issue_and_save(app: &mut App, rt: &Runtime, conn: &Connection) {
    let username = app.username.trim().to_string();
    if username.is_empty() {
        app.message = "Enter a username first".into();
        return;
    }
    let role = app.role().to_string();
    app.message = format!("Issuing {role} keys for '{username}'...");
    match rt.block_on(issue_both(&username, &role)) {
        Ok((poly_key, kalshi_key)) => {
            let student = Student {
                username: username.clone(),
                display_name: username.clone(),
                poly_key: poly_key.clone(),
                kalshi_key: kalshi_key.clone(),
                role: role.clone(),
                created_at: chrono::Utc::now().to_rfc3339(),
            };
            match registry::save_student(conn, &student) {
                Ok(()) => {
                    app.students = registry::list_students(conn).unwrap_or_default();
                    app.username.clear();
                    app.message = format!(
                        "✅ {username} [{role}]: poly={poly_key}  kalshi={kalshi_key} (saved)"
                    );
                }
                Err(e) => app.message = format!("Issued but failed to save roster: {e}"),
            }
        }
        Err(e) => app.message = format!("Error issuing keys: {e}"),
    }
}

/// Mint a paper key with role `role` on each simulator. Returns (polymarket_key, kalshi_key_id).
async fn issue_both(username: &str, role: &str) -> Result<(String, String)> {
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
        .context("polymarket response missing api_key (check PREDLAB_ADMIN_SECRET)")?;

    // Kalshi: generate endpoint (now admin-gated) returns an RSA keypair; record the key id.
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
    let kalshi_key = kalshi_json
        .get("api_key_id")
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .context("kalshi response missing api_key_id (check PREDLAB_KALSHI_SECRET / role)")?;

    Ok((poly_key, kalshi_key))
}

fn ui(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(8),
            Constraint::Length(3),
        ])
        .split(f.area());

    let tab_index = match app.view {
        View::Issue => 0,
        View::Roster => 1,
    };
    let tabs = Tabs::new(vec!["ISSUE KEYS", "ROSTER"])
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("PREDLAB ADMIN"),
        )
        .select(tab_index)
        .highlight_style(Style::default().fg(Color::Green).add_modifier(Modifier::BOLD));
    f.render_widget(tabs, chunks[0]);

    match app.view {
        View::Issue => {
            let body = Paragraph::new(format!(
                "Username:  {}\nRole:      {}   (↑/↓ to change)\n\nPress ENTER to create the user and mint paper keys on both simulators\n(Polymarket + Kalshi). Granting admin/owner needs an OWNER secret.",
                app.username,
                app.role(),
            ))
            .block(
                Block::default()
                    .title("Issue dual paper keys")
                    .borders(Borders::ALL),
            );
            f.render_widget(body, chunks[1]);
        }
        View::Roster => {
            let header = Row::new(["USERNAME", "ROLE", "POLY KEY", "KALSHI KEY", "CREATED"])
                .style(Style::default().fg(Color::Green).add_modifier(Modifier::BOLD));
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
            let table = Table::new(rows, widths).header(header).block(
                Block::default()
                    .title(format!("Club roster ({} members)", app.students.len()))
                    .borders(Borders::ALL),
            );
            f.render_widget(table, chunks[1]);
        }
    }

    let footer = Paragraph::new(app.message.clone())
        .block(Block::default().title("Status").borders(Borders::ALL))
        .style(Style::default().fg(Color::Yellow));
    f.render_widget(footer, chunks[2]);
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max.saturating_sub(1)])
    }
}
