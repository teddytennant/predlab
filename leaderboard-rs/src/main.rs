//! PredLab club leaderboard — a single clean web page at the club subdomain.
//!
//! Fetches both simulators' admin standings *server-side* (so the admin secrets
//! never leave the box), merges them into a combined paper net-worth ranking,
//! and serves one self-contained HTML page styled as a plain monochrome
//! terminal table. The standings are cached briefly so a burst of visitors
//! doesn't hammer the sims; the page also auto-refreshes.
//!
//! Config via env:
//!   BIND                   listen address (default 0.0.0.0:8003)
//!   POLY_URL               Polymarket sim base (default http://localhost:8001)
//!   KALSHI_URL             Kalshi sim base     (default http://localhost:8002)
//!   PREDLAB_ADMIN_SECRET   X-Admin-Secret for Polymarket /admin/leaderboard
//!   PREDLAB_KALSHI_SECRET  X-Kalshi-Sim-Admin for Kalshi /admin/leaderboard
//!   EXCLUDE_USERS          comma-separated usernames hidden from the board
//!                          (default "club_admin,demo_trader" — staff/seed accounts)

use std::collections::{BTreeMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use axum::{extract::State, response::Html, routing::get, Router};

/// How long a rendered page is reused before we re-fetch the sims.
const CACHE_TTL: Duration = Duration::from_secs(15);
/// Browser auto-refresh interval (seconds) baked into the page.
const REFRESH_SECS: u32 = 30;

#[derive(Clone, Default)]
struct Leader {
    username: String,
    poly: f64,
    kalshi: f64,
    total: f64,
}

struct Cache {
    html: String,
    at: Option<Instant>,
}

#[derive(Clone)]
struct AppState {
    poly_url: String,
    kalshi_url: String,
    admin_secret: String,
    kalshi_secret: String,
    exclude: HashSet<String>,
    client: reqwest::Client,
    cache: Arc<Mutex<Cache>>,
}

impl AppState {
    fn from_env() -> Self {
        let env = |k: &str, d: &str| std::env::var(k).unwrap_or_else(|_| d.to_string());
        let exclude = env("EXCLUDE_USERS", "club_admin,demo_trader")
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        Self {
            poly_url: env("POLY_URL", "http://localhost:8001")
                .trim_end_matches('/')
                .to_string(),
            kalshi_url: env("KALSHI_URL", "http://localhost:8002")
                .trim_end_matches('/')
                .to_string(),
            admin_secret: env("PREDLAB_ADMIN_SECRET", ""),
            kalshi_secret: env("PREDLAB_KALSHI_SECRET", ""),
            exclude,
            client: reqwest::Client::new(),
            cache: Arc::new(Mutex::new(Cache { html: String::new(), at: None })),
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let state = AppState::from_env();
    let app = Router::new()
        .route("/", get(index))
        .route("/healthz", get(|| async { "ok" }))
        .with_state(state);

    let bind = std::env::var("BIND").unwrap_or_else(|_| "0.0.0.0:8003".to_string());
    let listener = tokio::net::TcpListener::bind(&bind)
        .await
        .with_context(|| format!("binding {bind}"))?;
    println!("leaderboard listening on http://{bind}");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn index(State(st): State<AppState>) -> Html<String> {
    // Serve a fresh-enough cached render without touching the sims.
    if let Ok(c) = st.cache.lock() {
        if let Some(at) = c.at {
            if at.elapsed() < CACHE_TTL && !c.html.is_empty() {
                return Html(c.html.clone());
            }
        }
    }

    let html = match fetch_leaders(&st).await {
        Ok(rows) => render_page(&rows),
        Err(e) => render_error(&e.to_string()),
    };

    if let Ok(mut c) = st.cache.lock() {
        c.html = html.clone();
        c.at = Some(Instant::now());
    }
    Html(html)
}

/// Fetch both sims' per-user net worth and merge into a combined ranking,
/// dropping any excluded (staff/seed) usernames.
async fn fetch_leaders(st: &AppState) -> Result<Vec<Leader>> {
    let net = |v: &serde_json::Value| v.get("net_worth").and_then(|n| n.as_f64()).unwrap_or(0.0);

    let poly: Vec<serde_json::Value> = st
        .client
        .get(format!("{}/admin/leaderboard", st.poly_url))
        .header("X-Admin-Secret", &st.admin_secret)
        .send()
        .await
        .context("calling polymarket /admin/leaderboard")?
        .error_for_status()
        .context("polymarket leaderboard rejected")?
        .json()
        .await
        .context("parsing polymarket leaderboard")?;

    let kalshi: Vec<serde_json::Value> = st
        .client
        .get(format!("{}/trade-api/v2/admin/leaderboard", st.kalshi_url))
        .header("X-Kalshi-Sim-Admin", &st.kalshi_secret)
        .send()
        .await
        .context("calling kalshi /admin/leaderboard")?
        .error_for_status()
        .context("kalshi leaderboard rejected")?
        .json()
        .await
        .context("parsing kalshi leaderboard")?;

    let mut by_user: BTreeMap<String, Leader> = BTreeMap::new();
    for e in &poly {
        if let Some(u) = e.get("username").and_then(|v| v.as_str()) {
            if st.exclude.contains(u) {
                continue;
            }
            by_user.entry(u.to_string()).or_default().poly = net(e);
        }
    }
    for e in &kalshi {
        if let Some(u) = e.get("username").and_then(|v| v.as_str()) {
            if st.exclude.contains(u) {
                continue;
            }
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

/// Format a dollar amount with thousands separators, e.g. 25431.5 -> "$25,431.50".
fn fmt_money(v: f64) -> String {
    let neg = v < 0.0;
    let cents = (v.abs() * 100.0).round() as u64;
    let (whole, frac) = (cents / 100, cents % 100);
    let digits = whole.to_string();
    let bytes = digits.as_bytes();
    let mut grouped = String::new();
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 && (bytes.len() - i) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(*b as char);
    }
    format!("{}${}.{:02}", if neg { "-" } else { "" }, grouped, frac)
}

/// Minimal HTML escaping for user-supplied text (usernames).
fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut t: String = s.chars().take(max.saturating_sub(1)).collect();
        t.push('…');
        t
    }
}

enum Align {
    Left,
    Right,
}

/// Render an aligned, box-drawn monospace table (a real terminal table).
fn build_table(headers: &[&str], aligns: &[Align], rows: &[Vec<String>]) -> String {
    let ncols = headers.len();
    let mut widths: Vec<usize> = headers.iter().map(|h| h.chars().count()).collect();
    for row in rows {
        for (i, cell) in row.iter().enumerate() {
            widths[i] = widths[i].max(cell.chars().count());
        }
    }
    let pad = |cell: &str, i: usize| -> String {
        let fill = widths[i].saturating_sub(cell.chars().count());
        match aligns[i] {
            Align::Left => format!("{}{}", cell, " ".repeat(fill)),
            Align::Right => format!("{}{}", " ".repeat(fill), cell),
        }
    };
    let border = |l: char, m: char, r: char| -> String {
        let mut s = String::new();
        s.push(l);
        for (i, w) in widths.iter().enumerate() {
            s.push_str(&"─".repeat(w + 2));
            s.push(if i + 1 == ncols { r } else { m });
        }
        s
    };
    let row_str = |cells: &[String]| -> String {
        let mut s = String::from("│");
        for (i, cell) in cells.iter().enumerate() {
            s.push(' ');
            s.push_str(&pad(cell, i));
            s.push_str(" │");
        }
        s
    };

    let header_cells: Vec<String> = headers.iter().map(|h| h.to_string()).collect();
    let mut out = String::new();
    out.push_str(&border('┌', '┬', '┐'));
    out.push('\n');
    out.push_str(&row_str(&header_cells));
    out.push('\n');
    out.push_str(&border('├', '┼', '┤'));
    out.push('\n');
    for r in rows {
        out.push_str(&row_str(r));
        out.push('\n');
    }
    out.push_str(&border('└', '┴', '┘'));
    out
}

const STYLE: &str = r#"
:root { color-scheme: dark; }
html, body { margin: 0; height: 100%; background: #000; }
body {
  display: flex; align-items: flex-start; justify-content: center;
  padding: 48px 16px;
}
pre {
  margin: 0;
  font-family: "DejaVu Sans Mono", "SFMono-Regular", ui-monospace, Menlo, Consolas, monospace;
  font-size: 14px;
  /* line-height: 1 so the box-drawing chars touch and form continuous lines */
  line-height: 1;
  color: #f0f0f0;
  white-space: pre;
}
.dim { color: #666; }
@media (max-width: 640px) { pre { font-size: 11px; } }
"#;

/// Wrap a pre-rendered table (or message) in the terminal-style page chrome.
fn page_shell(table_text: &str) -> String {
    let content = format!(
        "<span class=\"dim\">$</span> predlab leaderboard\n\n{}\n\n\
         <span class=\"dim\"># combined net worth · both simulators · \
         refreshes every {}s · paper trading only</span>",
        esc(table_text),
        REFRESH_SECS,
    );
    format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta http-equiv="refresh" content="{refresh}">
<title>predlab · leaderboard</title>
<style>{style}</style>
</head>
<body><main><pre>{content}</pre></main></body>
</html>"#,
        refresh = REFRESH_SECS,
        style = STYLE,
        content = content,
    )
}

fn render_page(rows: &[Leader]) -> String {
    if rows.is_empty() {
        return page_shell("  (no members yet — check back once keys are issued)");
    }
    let data: Vec<Vec<String>> = rows
        .iter()
        .enumerate()
        .map(|(i, l)| {
            vec![
                (i + 1).to_string(),
                truncate(&l.username, 24),
                fmt_money(l.poly),
                fmt_money(l.kalshi),
                fmt_money(l.total),
            ]
        })
        .collect();
    let table = build_table(
        &["#", "MEMBER", "POLYMARKET", "KALSHI", "TOTAL"],
        &[Align::Right, Align::Left, Align::Right, Align::Right, Align::Right],
        &data,
    );
    page_shell(&table)
}

fn render_error(msg: &str) -> String {
    page_shell(&format!("  standings temporarily unavailable\n  {msg}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leader(name: &str, poly: f64, kalshi: f64) -> Leader {
        Leader { username: name.into(), poly, kalshi, total: poly + kalshi }
    }

    #[test]
    fn money_groups_thousands() {
        assert_eq!(fmt_money(25000.0), "$25,000.00");
        assert_eq!(fmt_money(1234567.5), "$1,234,567.50");
        assert_eq!(fmt_money(0.0), "$0.00");
    }

    #[test]
    fn table_lists_members_in_order() {
        let rows = vec![leader("alice", 30000.0, 25000.0), leader("bob", 20000.0, 20000.0)];
        let html = render_page(&rows);
        assert!(html.contains("MEMBER"));
        assert!(html.contains("alice"));
        assert!(html.contains("$55,000.00"));
        assert!(html.find("alice").unwrap() < html.find("bob").unwrap());
        // box-drawing terminal table, no medals
        assert!(html.contains('┌') && html.contains('│'));
        assert!(!html.contains('🥇'));
    }

    #[test]
    fn empty_roster_renders_placeholder() {
        assert!(render_page(&[]).contains("no members yet"));
    }

    #[test]
    fn username_is_escaped() {
        let html = render_page(&[leader("<script>", 0.0, 0.0)]);
        assert!(!html.contains("<script>"));
        assert!(html.contains("&lt;script&gt;"));
    }
}
