//! PredLab club leaderboard — a single clean web page at the club subdomain.
//!
//! Fetches both simulators' admin standings *server-side* (so the admin secrets
//! never leave the box), merges them into a combined paper net-worth ranking,
//! and serves one self-contained HTML page. The standings are cached briefly so
//! a burst of visitors doesn't hammer the sims; the page also auto-refreshes.
//!
//! Config via env:
//!   BIND                   listen address (default 0.0.0.0:8003)
//!   POLY_URL               Polymarket sim base (default http://localhost:8001)
//!   KALSHI_URL             Kalshi sim base     (default http://localhost:8002)
//!   PREDLAB_ADMIN_SECRET   X-Admin-Secret for Polymarket /admin/leaderboard
//!   PREDLAB_KALSHI_SECRET  X-Kalshi-Sim-Admin for Kalshi /admin/leaderboard

use std::collections::BTreeMap;
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
    client: reqwest::Client,
    cache: Arc<Mutex<Cache>>,
}

impl AppState {
    fn from_env() -> Self {
        let env = |k: &str, d: &str| std::env::var(k).unwrap_or_else(|_| d.to_string());
        Self {
            poly_url: env("POLY_URL", "http://localhost:8001")
                .trim_end_matches('/')
                .to_string(),
            kalshi_url: env("KALSHI_URL", "http://localhost:8002")
                .trim_end_matches('/')
                .to_string(),
            admin_secret: env("PREDLAB_ADMIN_SECRET", ""),
            kalshi_secret: env("PREDLAB_KALSHI_SECRET", ""),
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

/// Fetch both sims' per-user net worth and merge into a combined ranking.
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

const STYLE: &str = r#"
:root { color-scheme: dark; }
* { box-sizing: border-box; }
body {
  margin: 0; min-height: 100vh;
  font-family: ui-sans-serif, system-ui, -apple-system, Segoe UI, Roboto, sans-serif;
  background: radial-gradient(1200px 600px at 50% -10%, #1b2440 0%, #0b0f1a 60%);
  color: #e6e9f2; display: flex; justify-content: center; padding: 40px 16px 64px;
}
.wrap { width: 100%; max-width: 760px; }
header { text-align: center; margin-bottom: 28px; }
h1 { margin: 0; font-size: clamp(28px, 6vw, 44px); letter-spacing: -0.5px; }
.sub { color: #9aa3b8; margin-top: 8px; font-size: 15px; }
table { width: 100%; border-collapse: collapse; background: #121829;
  border: 1px solid #1f2940; border-radius: 14px; overflow: hidden; }
th, td { padding: 14px 16px; text-align: left; }
th { font-size: 12px; text-transform: uppercase; letter-spacing: 0.06em;
  color: #8b94ad; border-bottom: 1px solid #1f2940; }
td { border-bottom: 1px solid #161d30; }
tr:last-child td { border-bottom: none; }
td.num, th.num { text-align: right; font-variant-numeric: tabular-nums;
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace; }
.rank { width: 56px; font-size: 20px; text-align: center; }
.member { font-weight: 600; }
.total { font-weight: 700; }
tr.gold { background: linear-gradient(90deg, rgba(255,209,102,0.14), transparent); }
tr.silver { background: linear-gradient(90deg, rgba(197,205,224,0.10), transparent); }
tr.bronze { background: linear-gradient(90deg, rgba(205,127,80,0.12), transparent); }
.foot { text-align: center; color: #6b7488; margin-top: 18px; font-size: 13px; }
.empty { text-align: center; color: #8b94ad; padding: 36px; }
"#;

fn render_page(rows: &[Leader]) -> String {
    let mut body = String::new();
    if rows.is_empty() {
        body.push_str(r#"<tr><td colspan="5" class="empty">No members yet — check back once keys are issued.</td></tr>"#);
    } else {
        for (i, l) in rows.iter().enumerate() {
            let rank = i + 1;
            let (cls, badge) = match rank {
                1 => ("gold", "🥇".to_string()),
                2 => ("silver", "🥈".to_string()),
                3 => ("bronze", "🥉".to_string()),
                n => ("", n.to_string()),
            };
            body.push_str(&format!(
                "<tr class=\"{cls}\">\
                   <td class=\"rank\">{badge}</td>\
                   <td class=\"member\">{name}</td>\
                   <td class=\"num\">{poly}</td>\
                   <td class=\"num\">{kalshi}</td>\
                   <td class=\"num total\">{total}</td>\
                 </tr>",
                name = esc(&l.username),
                poly = fmt_money(l.poly),
                kalshi = fmt_money(l.kalshi),
                total = fmt_money(l.total),
            ));
        }
    }

    format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta http-equiv="refresh" content="{refresh}">
<title>PredLab — Club Leaderboard</title>
<style>{style}</style>
</head>
<body>
  <div class="wrap">
    <header>
      <h1>🏆 PredLab Leaderboard</h1>
      <div class="sub">Combined paper net worth across both simulators · refreshes every {refresh}s</div>
    </header>
    <table>
      <thead>
        <tr>
          <th class="rank">#</th>
          <th>Member</th>
          <th class="num">Polymarket</th>
          <th class="num">Kalshi</th>
          <th class="num">Total</th>
        </tr>
      </thead>
      <tbody>
        {body}
      </tbody>
    </table>
    <div class="foot">Paper trading only · Prediction Markets Club</div>
  </div>
</body>
</html>"#,
        refresh = REFRESH_SECS,
        style = STYLE,
        body = body,
    )
}

fn render_error(msg: &str) -> String {
    format!(
        r#"<!doctype html>
<html lang="en"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta http-equiv="refresh" content="{refresh}">
<title>PredLab — Leaderboard</title><style>{style}</style></head>
<body><div class="wrap">
  <header><h1>🏆 PredLab Leaderboard</h1>
  <div class="sub">Standings are momentarily unavailable — retrying…</div></header>
  <div class="empty">{msg}</div>
</div></body></html>"#,
        refresh = REFRESH_SECS,
        style = STYLE,
        msg = esc(msg),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn money_groups_thousands() {
        assert_eq!(fmt_money(25000.0), "$25,000.00");
        assert_eq!(fmt_money(1234567.5), "$1,234,567.50");
        assert_eq!(fmt_money(0.0), "$0.00");
    }

    #[test]
    fn page_ranks_and_medals() {
        let rows = vec![
            Leader { username: "alice".into(), poly: 30000.0, kalshi: 25000.0, total: 55000.0 },
            Leader { username: "bob".into(), poly: 20000.0, kalshi: 20000.0, total: 40000.0 },
        ];
        let html = render_page(&rows);
        assert!(html.contains("🥇"));
        assert!(html.contains("alice"));
        assert!(html.contains("$55,000.00"));
        // leader appears before the runner-up
        assert!(html.find("alice").unwrap() < html.find("bob").unwrap());
    }

    #[test]
    fn empty_roster_renders_placeholder() {
        assert!(render_page(&[]).contains("No members yet"));
    }

    #[test]
    fn username_is_escaped() {
        let rows = vec![Leader { username: "<script>".into(), ..Default::default() }];
        let html = render_page(&rows);
        assert!(!html.contains("<script>"));
        assert!(html.contains("&lt;script&gt;"));
    }
}
