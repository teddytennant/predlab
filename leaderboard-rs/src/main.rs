//! PredLab club leaderboard — a single clean web page at the club subdomain.
//!
//! Fetches the Polymarket simulator's admin standings *server-side* (so the
//! admin secret never leaves the box) and serves one self-contained HTML page
//! styled as a plain monochrome terminal table. The standings are cached
//! briefly so a burst of visitors doesn't hammer the sim; the page also
//! auto-refreshes.
//!
//! Config via env:
//!   BIND                   listen address (default 0.0.0.0:8003)
//!   POLY_URL               Polymarket sim base (default http://localhost:8001)
//!   PREDLAB_ADMIN_SECRET   X-Admin-Secret for Polymarket /admin/leaderboard
//!   EXCLUDE_USERS          comma-separated usernames hidden from the board
//!                          (default "club_admin,demo_trader" — staff/seed accounts)

use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use axum::{
    extract::State,
    http::header,
    response::{Html, IntoResponse},
    routing::get,
    Router,
};

/// How long a rendered page is reused before we re-fetch the sim.
const CACHE_TTL: Duration = Duration::from_secs(15);
/// Browser auto-refresh interval (seconds) baked into the page.
const REFRESH_SECS: u32 = 30;

/// The member client, embedded at build time from `examples/predlab.py` (see
/// build.rs) and served as a download at `/predlab.py`.
const CLIENT_PY: &str = include_str!(concat!(env!("OUT_DIR"), "/predlab_client.py"));

#[derive(Clone, Default)]
struct Leader {
    username: String,
    net_worth: f64,
}

struct Cache {
    html: String,
    at: Option<Instant>,
}

#[derive(Clone)]
struct AppState {
    poly_url: String,
    admin_secret: String,
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
            admin_secret: env("PREDLAB_ADMIN_SECRET", ""),
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
        .route("/predlab.py", get(client_py))
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
    // Serve a fresh-enough cached render without touching the sim.
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

/// Serve the embedded member client as a file download.
async fn client_py() -> impl IntoResponse {
    (
        [
            (header::CONTENT_TYPE, "text/x-python; charset=utf-8"),
            (header::CONTENT_DISPOSITION, "attachment; filename=\"predlab.py\""),
        ],
        CLIENT_PY,
    )
}

/// Fetch the sim's per-user net worth, dropping excluded (staff/seed) usernames.
async fn fetch_leaders(st: &AppState) -> Result<Vec<Leader>> {
    let rows: Vec<serde_json::Value> = st
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

    let mut leaders: Vec<Leader> = rows
        .iter()
        .filter_map(|e| {
            let u = e.get("username").and_then(|v| v.as_str())?;
            if st.exclude.contains(u) {
                return None;
            }
            Some(Leader {
                username: u.to_string(),
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
html, body { margin: 0; min-height: 100%; background: #000; }
body {
  display: flex; align-items: flex-start; justify-content: center;
  padding: 48px 16px;
  font-family: "DejaVu Sans Mono", "SFMono-Regular", ui-monospace, Menlo, Consolas, monospace;
  color: #f0f0f0;
}
main { display: flex; flex-direction: column; align-items: stretch; gap: 30px; width: 100%; max-width: 680px; }
pre.board {
  margin: 0;
  font-size: 14px;
  /* line-height: 1 so the box-drawing chars touch and form continuous lines */
  line-height: 1;
  white-space: pre;
  overflow-x: auto;
}
.dim { color: #666; }
a { color: #8ab4ff; }
.onboard { font-size: 14px; line-height: 1.55; }
.onboard h2 { font-size: 14px; font-weight: normal; margin: 0 0 12px; }
.onboard ol { margin: 0; padding-left: 1.5em; }
.onboard li { margin: 7px 0; }
.onboard code { color: #d7d7d7; }
.btn {
  display: inline-block; margin-left: 6px;
  border: 1px solid #f0f0f0; color: #f0f0f0; text-decoration: none;
  padding: 4px 12px; border-radius: 3px; font-weight: bold;
}
.btn:hover { background: #f0f0f0; color: #000; }
pre.snippet {
  margin: 10px 0 0; font-size: 13px; line-height: 1.45; white-space: pre;
  background: #0c0c0c; border: 1px solid #222; border-radius: 4px;
  padding: 12px 14px; overflow-x: auto; color: #d7d7d7;
}
@media (max-width: 640px) { pre.board { font-size: 11px; } }
"#;

/// Onboarding block shown under the board: how a student goes from zero to
/// trading, with a one-click download of the client served from this domain.
const ONBOARD: &str = r##"<section class="onboard">
<h2><span class="dim">$</span> new here? start trading in 4 steps</h2>
<ol>
<li>Ask a club admin for your <strong>API key</strong> (it looks like <code>pm_paper_…</code>). You start with $25,000 of paper money.</li>
<li>Download the one-file client:<a class="btn" href="/predlab.py" download="predlab.py">⬇ predlab.py</a></li>
<li>Install its only dependency: <code>pip install requests</code></li>
<li>Drop in your key and trade:</li>
</ol>
<pre class="snippet">from predlab import PolymarketClient

poly = PolymarketClient(api_key="pm_paper_…")    # your key
print(poly.markets(limit=5))                     # browse markets
yes_token = poly.markets(limit=1)[0]["clobTokenIds"][0]
poly.place_order(token_id=yes_token, side="BUY", price=0.55, size=10)
print(poly.positions())                          # what you now hold</pre>
<p class="dim">paper trading only · full step-by-step guide → <a href="https://github.com/teddytennant/predlab#getting-started-members">github.com/teddytennant/predlab</a></p>
</section>"##;

/// Wrap a pre-rendered table (or message) in the terminal-style page chrome.
fn page_shell(table_text: &str) -> String {
    let board = format!(
        "<span class=\"dim\">$</span> predlab leaderboard\n\n{}\n\n\
         <span class=\"dim\"># paper net worth · refreshes every {}s · paper trading only</span>",
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
<body><main><pre class="board">{board}</pre>{onboard}</main></body>
</html>"#,
        refresh = REFRESH_SECS,
        style = STYLE,
        board = board,
        onboard = ONBOARD,
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
                truncate(&l.username, 28),
                fmt_money(l.net_worth),
            ]
        })
        .collect();
    let table = build_table(
        &["#", "MEMBER", "NET WORTH"],
        &[Align::Right, Align::Left, Align::Right],
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

    fn leader(name: &str, net: f64) -> Leader {
        Leader { username: name.into(), net_worth: net }
    }

    #[test]
    fn money_groups_thousands() {
        assert_eq!(fmt_money(25000.0), "$25,000.00");
        assert_eq!(fmt_money(1234567.5), "$1,234,567.50");
        assert_eq!(fmt_money(0.0), "$0.00");
    }

    #[test]
    fn table_lists_members_in_order() {
        let rows = vec![leader("alice", 55000.0), leader("bob", 40000.0)];
        let html = render_page(&rows);
        assert!(html.contains("MEMBER") && html.contains("NET WORTH"));
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
    fn page_has_onboarding_and_download_button() {
        let html = render_page(&[leader("alice", 25000.0)]);
        assert!(html.contains("start trading in 4 steps"));
        assert!(html.contains(r#"href="/predlab.py""#));
        assert!(html.contains("download=\"predlab.py\""));
        assert!(html.contains("pip install requests"));
    }

    #[test]
    fn embedded_client_is_the_real_one() {
        // build.rs pulls in examples/predlab.py; sanity-check it embedded.
        assert!(CLIENT_PY.contains("PolymarketClient"));
        assert!(CLIENT_PY.len() > 500);
    }

    #[test]
    fn username_is_escaped() {
        let html = render_page(&[leader("<script>", 0.0)]);
        assert!(!html.contains("<script>"));
        assert!(html.contains("&lt;script&gt;"));
    }
}
