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
    extract::{Path, State},
    http::header,
    response::{Html, IntoResponse, Json},
    routing::get,
    Router,
};
use serde::{Deserialize, Serialize};

/// How long a rendered page is reused before we re-fetch the sim.
const CACHE_TTL: Duration = Duration::from_secs(15);
/// Browser auto-refresh interval (seconds) baked into the page.
const REFRESH_SECS: u32 = 30;

/// The member client, embedded at build time from `examples/predlab.py` (see
/// build.rs) and served as a download at `/predlab.py`.
const CLIENT_PY: &str = include_str!(concat!(env!("OUT_DIR"), "/predlab_client.py"));

#[derive(Clone, Default, Serialize)]
struct Leader {
    username: String,
    net_worth: f64,
}

/// One row of `/leaderboard.json` — what the TUI consumes. Identical to
/// `Leader` plus a `rank` index so clients don't have to re-derive it.
#[derive(Serialize)]
struct LeaderJson {
    rank: usize,
    username: String,
    net_worth: f64,
}

/// Shape returned by the sim's `/admin/user/{name}` endpoint — the data the
/// per-user profile page renders (current breakdown + positions + trades +
/// the net-worth history series for the graph).
#[derive(Debug, Deserialize)]
struct UserDetail {
    username: String,
    #[serde(default)]
    role: String,
    cash: f64,
    positions_value: f64,
    #[serde(default)]
    open_orders_value: f64,
    net_worth: f64,
    #[serde(default)]
    positions: Vec<DetailPosition>,
    #[serde(default)]
    trades: Vec<DetailTrade>,
    #[serde(default)]
    history: Vec<HistoryPoint>,
}

#[derive(Debug, Deserialize)]
struct DetailPosition {
    market_id: String,
    #[serde(default)]
    market_question: Option<String>,
    size: f64,
    #[serde(default)]
    avg_entry_price: Option<f64>,
    current_price: f64,
    unrealized_pnl: f64,
}

#[derive(Debug, Deserialize)]
struct DetailTrade {
    market_id: String,
    side: String,
    price: f64,
    size: f64,
    created_at: String,
}

#[derive(Debug, Deserialize)]
struct HistoryPoint {
    #[serde(rename = "t")]
    at: String,
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
        .route("/start", get(start))
        .route("/tui", get(tui_page))
        .route("/u/:username", get(profile))
        .route("/predlab.py", get(client_py))
        .route("/leaderboard.json", get(leaderboard_json))
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

/// Standalone onboarding / download page.
async fn start() -> Html<String> {
    Html(render_start_page())
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

/// Public JSON ranking — same data the HTML page shows. Consumed by the
/// terminal client (`predlab-tui`). No PII beyond the usernames already on
/// the public board, so no auth required.
async fn leaderboard_json(State(st): State<AppState>) -> impl IntoResponse {
    let leaders = fetch_leaders(&st).await.unwrap_or_default();
    let rows: Vec<LeaderJson> = leaders
        .into_iter()
        .enumerate()
        .map(|(i, l)| LeaderJson {
            rank: i + 1,
            username: l.username,
            net_worth: l.net_worth,
        })
        .collect();
    Json(rows)
}

/// Onboarding page for the terminal client: install / run / vim keys.
async fn tui_page() -> Html<String> {
    Html(render_tui_page())
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

/// Percent-encode anything outside the URL path-safe set so `/u/{name}`
/// survives unusual usernames intact.
fn url_path_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'.' | b'_' | b'~' | b'-' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
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
.nav { margin: 0; font-size: 14px; }
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
svg.chart {
  display: block; width: 100%; max-width: 100%; height: auto;
  background: #050505; border: 1px solid #1a1a1a; border-radius: 3px;
  margin: 4px 0 14px;
}
.chart-empty {
  font-size: 13px; padding: 28px 14px; text-align: center;
  background: #050505; border: 1px solid #1a1a1a; border-radius: 3px;
  margin: 4px 0 14px;
}
@media (max-width: 640px) { pre.board { font-size: 11px; } }
"#;

/// The standalone get-started page: how a student goes from zero to trading,
/// with a one-click download of the client served from this domain.
const ONBOARD: &str = r##"<section class="onboard">
<h2><span class="dim">$</span> get started — trade in 4 steps</h2>
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
<p class="dim">prefer a terminal? <a href="/tui">→ install the TUI client (predlab-tui)</a></p>
</section>
<p class="nav"><a href="/">← back to leaderboard</a></p>"##;

/// Standalone install page for the terminal client. Mirrors the look of
/// `/start` so members feel like they're still on the same site.
const TUI_ONBOARD: &str = r##"<section class="onboard">
<h2><span class="dim">$</span> predlab-tui — paper trading in your shell</h2>
<p>A vim-flavored TUI clone of this site: leaderboard, markets, and your portfolio in one window. Same data, no browser.</p>

<h2 style="margin-top:18px"><span class="dim">$</span> install (one line)</h2>
<pre class="snippet">cargo install --git https://github.com/teddytennant/predlab predlab-tui</pre>
<p class="dim">requires Rust (<a href="https://rustup.rs">rustup.rs</a>). drops the <code>predlab-tui</code> binary onto your PATH.</p>

<h2 style="margin-top:18px"><span class="dim">$</span> run it</h2>
<pre class="snippet">export POLY_API_KEY=pm_paper_…   # your key (admin issues it)
predlab-tui</pre>
<p class="dim">no key? the Leaderboard and Markets tabs still work. ask an admin for one to unlock Portfolio.</p>

<h2 style="margin-top:18px"><span class="dim">$</span> keys (vim)</h2>
<pre class="snippet">h l   1 2 3 4   switch tab
j k   gg / G    move selection / jump
r R             refresh tab / refresh all
/needle         filter the current list
:cmd            ex command — try :help, :q
?               full help screen
q   Ctrl-c      quit</pre>

<p class="dim">screens:</p>
<pre class="snippet"><span class="dim">┌─ PREDLAB v0.1.0  ·  paper trading ────────────  ● connected ─┐</span>
<span class="dim"> </span> 1 <strong>LEADERBOARD</strong>   2 MARKETS    3 PORTFOLIO   4 HELP
<span class="dim">┌─ LEADERBOARD ────────────────────── 42 members · 12s ago ─┐</span>
<span class="dim">│</span>  #   MEMBER                        NET WORTH
<span class="dim">│</span>  ▶ 1 teddy                         $29,444.44 ★
<span class="dim">│</span>    2 alice                         $26,300.00
<span class="dim">│</span>    3 bob                           $25,420.10
<span class="dim">└────────────────────────────────────────────────────────────┘</span>
<span class="dim"> NORMAL </span> /ali                                ? help  : cmd  q quit</pre>

<p class="dim">runs anywhere with Rust + a 256-color terminal · paper money only</p>
</section>
<p class="nav"><a href="/">← back to leaderboard</a>  ·  <a href="/start">→ python client</a></p>"##;

/// Full HTML document with the shared terminal styling. `refresh` adds a
/// meta-refresh (the auto-updating leaderboard uses it; the start page omits it).
fn document(title: &str, refresh: Option<u32>, body: &str) -> String {
    let refresh_tag = match refresh {
        Some(s) => format!("<meta http-equiv=\"refresh\" content=\"{s}\">"),
        None => String::new(),
    };
    format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
{refresh_tag}
<title>{title}</title>
<style>{style}</style>
</head>
<body><main>{body}</main></body>
</html>"#,
        refresh_tag = refresh_tag,
        title = esc(title),
        style = STYLE,
        body = body,
    )
}

/// Wrap pre-escaped (or pre-built) HTML in the leaderboard page chrome with a
/// link out to the get-started page. The caller is responsible for escaping
/// any user-supplied text inside ``board_inner`` — this lets ``render_page``
/// inject ``<a>`` links into the table after escaping it.
fn page_shell(board_inner: &str) -> String {
    let board = format!(
        "<span class=\"dim\">$</span> predlab leaderboard\n\n{}\n\n\
         <span class=\"dim\"># click a member to see their graph · refreshes every {}s · paper trading only</span>",
        board_inner,
        REFRESH_SECS,
    );
    let body = format!(
        "<pre class=\"board\">{board}</pre>\n\
         <p class=\"nav\">new here? <a href=\"/start\">→ get your key &amp; start trading</a> · \
         <a href=\"/tui\">→ install the terminal client</a></p>",
        board = board,
    );
    document("predlab · leaderboard", Some(REFRESH_SECS), &body)
}

/// The standalone onboarding / download page.
fn render_start_page() -> String {
    document("predlab · get started", None, ONBOARD)
}

/// Standalone install page for the terminal client.
fn render_tui_page() -> String {
    document("predlab · TUI client", None, TUI_ONBOARD)
}

fn render_page(rows: &[Leader]) -> String {
    if rows.is_empty() {
        return page_shell(&esc("  (no members yet — check back once keys are issued)"));
    }
    let displays: Vec<String> = rows.iter().map(|l| truncate(&l.username, 28)).collect();
    let data: Vec<Vec<String>> = rows
        .iter()
        .enumerate()
        .map(|(i, l)| {
            vec![
                (i + 1).to_string(),
                displays[i].clone(),
                fmt_money(l.net_worth),
            ]
        })
        .collect();
    let table = build_table(
        &["#", "MEMBER", "NET WORTH"],
        &[Align::Right, Align::Left, Align::Right],
        &data,
    );
    // Escape the whole table first so all user-supplied text is HTML-safe,
    // then turn each (still unique) username into a link to its profile.
    let mut html = esc(&table);
    for (i, l) in rows.iter().enumerate() {
        let escaped = esc(&displays[i]);
        let link = format!(
            r#"<a href="/u/{}">{}</a>"#,
            url_path_encode(&l.username),
            escaped,
        );
        // Replace exactly once: usernames are unique on the leaderboard.
        html = html.replacen(&escaped, &link, 1);
    }
    page_shell(&html)
}

fn render_error(msg: &str) -> String {
    page_shell(&esc(&format!("  standings temporarily unavailable\n  {msg}")))
}

// ---------------------------------------------------------------------------
// Per-user profile page  (/u/:username)
// ---------------------------------------------------------------------------

async fn profile(Path(username): Path<String>, State(st): State<AppState>) -> Html<String> {
    let detail = match fetch_user_detail(&st, &username).await {
        Ok(d) => d,
        Err(e) => return Html(render_profile_error(&username, &e.to_string())),
    };
    // The sim doesn't include rank in /admin/user; reuse the leaderboard call.
    let rank = fetch_leaders(&st)
        .await
        .ok()
        .and_then(|ls| ls.iter().position(|l| l.username == detail.username).map(|i| i + 1));
    Html(render_profile_page(&detail, rank))
}

async fn fetch_user_detail(st: &AppState, username: &str) -> Result<UserDetail> {
    let url = format!("{}/admin/user/{}", st.poly_url, url_path_encode(username));
    st.client
        .get(&url)
        .header("X-Admin-Secret", &st.admin_secret)
        .send()
        .await
        .context("calling polymarket /admin/user")?
        .error_for_status()
        .context("polymarket /admin/user rejected (unknown member?)")?
        .json::<UserDetail>()
        .await
        .context("parsing /admin/user response")
}

/// Trim a 26-char ISO datetime down to `YYYY-MM-DD HH:MM` for the trades table.
fn fmt_time_short(s: &str) -> String {
    let head: String = s.chars().take(16).collect();
    head.replace('T', " ")
}

/// Trim an ISO timestamp to just the date for the chart's x-axis labels.
fn fmt_date(s: &str) -> String {
    s.chars().take(10).collect()
}

/// Pure-SVG monochrome line chart of net worth over time. Renders inline (no
/// JS, no external assets). Returns a friendly placeholder when there aren't
/// enough points yet to draw a line.
fn svg_chart(history: &[HistoryPoint]) -> String {
    if history.len() < 2 {
        return r#"<div class="chart-empty dim">collecting net-worth points — your line will appear here after a couple of snapshots (recorded on every fill and every 5 min)</div>"#.to_string();
    }

    let n = history.len();
    let ys: Vec<f64> = history.iter().map(|p| p.net_worth).collect();
    let mut ymin = ys.iter().cloned().fold(f64::INFINITY, f64::min);
    let mut ymax = ys.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    if (ymax - ymin).abs() < 1e-9 {
        // Flat line — synthesize a small range so the polyline is visible.
        ymin -= 1.0;
        ymax += 1.0;
    }
    let pad_y = (ymax - ymin) * 0.08;
    ymin -= pad_y;
    ymax += pad_y;

    let (w, h) = (680.0_f64, 220.0_f64);
    let (pl, pr, pt, pb) = (78.0_f64, 12.0_f64, 14.0_f64, 28.0_f64);
    let plot_w = w - pl - pr;
    let plot_h = h - pt - pb;
    let xp = |i: usize| pl + (i as f64) / ((n as f64) - 1.0) * plot_w;
    let yp = |v: f64| pt + (1.0 - (v - ymin) / (ymax - ymin)) * plot_h;

    let points: String = (0..n)
        .map(|i| format!("{:.1},{:.1}", xp(i), yp(ys[i])))
        .collect::<Vec<_>>()
        .join(" ");

    let ymid = (ymin + ymax) / 2.0;
    let labels = format!(
        r##"<text x="{lx}" y="{y1}" text-anchor="end" font-size="10" fill="#888">{lmax}</text>
<text x="{lx}" y="{y2}" text-anchor="end" font-size="10" fill="#888">{lmid}</text>
<text x="{lx}" y="{y3}" text-anchor="end" font-size="10" fill="#888">{lmin}</text>"##,
        lx = pl - 6.0,
        y1 = pt + 4.0,
        y2 = pt + plot_h / 2.0 + 4.0,
        y3 = pt + plot_h + 4.0,
        lmax = esc(&fmt_money(ymax)),
        lmid = esc(&fmt_money(ymid)),
        lmin = esc(&fmt_money(ymin)),
    );

    let bottom = format!(
        r##"<text x="{xs}" y="{yb}" text-anchor="start" font-size="10" fill="#888">{ls}</text>
<text x="{xe}" y="{yb}" text-anchor="end" font-size="10" fill="#888">{le}</text>"##,
        xs = pl,
        xe = pl + plot_w,
        yb = pt + plot_h + 20.0,
        ls = esc(&fmt_date(&history.first().unwrap().at)),
        le = esc(&fmt_date(&history.last().unwrap().at)),
    );

    let axes = format!(
        r##"<line x1="{pl}" y1="{pt}" x2="{pl}" y2="{ay}" stroke="#333" stroke-width="1"/>
<line x1="{pl}" y1="{ay}" x2="{ax}" y2="{ay}" stroke="#333" stroke-width="1"/>"##,
        pl = pl,
        pt = pt,
        ay = pt + plot_h,
        ax = pl + plot_w,
    );

    format!(
        r##"<svg class="chart" viewBox="0 0 {w} {h}" xmlns="http://www.w3.org/2000/svg" role="img" aria-label="Net worth over time">
{axes}
<polyline fill="none" stroke="#f0f0f0" stroke-width="1.5" points="{pts}"/>
{labels}
{bottom}
</svg>"##,
        w = w,
        h = h,
        axes = axes,
        pts = points,
        labels = labels,
        bottom = bottom,
    )
}

/// Render the small key/value summary block at the top of the profile page.
fn render_summary_block(d: &UserDetail, rank: Option<usize>) -> String {
    let role_suffix = if d.role.is_empty() || d.role == "member" {
        String::new()
    } else {
        format!(" · {}", d.role)
    };
    let header = format!(
        "<span class=\"dim\">$</span> predlab member · {}{}",
        esc(&d.username),
        esc(&role_suffix),
    );
    let rank_str = rank
        .map(|r| format!("#{r}"))
        .unwrap_or_else(|| "—".to_string());
    let kv = format!(
        "  {:<10}{}\n  {:<10}{}\n  {:<10}{}\n  {:<10}{}\n  {:<10}{}",
        "rank",
        rank_str,
        "cash",
        fmt_money(d.cash),
        "position",
        fmt_money(d.positions_value),
        "orders",
        fmt_money(d.open_orders_value),
        "net worth",
        fmt_money(d.net_worth),
    );
    format!("{header}\n\n{kv}", header = header, kv = esc(&kv))
}

fn render_positions_block(positions: &[DetailPosition]) -> String {
    let title = "<span class=\"dim\"># positions</span>";
    if positions.is_empty() {
        return format!("{title}\n\n  <span class=\"dim\">(no open positions)</span>");
    }
    let rows: Vec<Vec<String>> = positions
        .iter()
        .map(|p| {
            vec![
                truncate(&p.market_id, 8),
                truncate(p.market_question.as_deref().unwrap_or(""), 40),
                format!("{:.4}", p.size),
                p.avg_entry_price
                    .map(|v| format!("{v:.4}"))
                    .unwrap_or_else(|| "—".to_string()),
                format!("{:.4}", p.current_price),
                fmt_money(p.unrealized_pnl),
            ]
        })
        .collect();
    let table = build_table(
        &["MARKET", "QUESTION", "SIZE", "ENTRY", "MARK", "UNREALIZED"],
        &[
            Align::Left,
            Align::Left,
            Align::Right,
            Align::Right,
            Align::Right,
            Align::Right,
        ],
        &rows,
    );
    format!("{title}\n\n{}", esc(&table))
}

fn render_trades_block(trades: &[DetailTrade]) -> String {
    let title = "<span class=\"dim\"># recent trades</span>";
    if trades.is_empty() {
        return format!("{title}\n\n  <span class=\"dim\">(no trades yet)</span>");
    }
    let rows: Vec<Vec<String>> = trades
        .iter()
        .map(|t| {
            vec![
                fmt_time_short(&t.created_at),
                truncate(&t.market_id, 8),
                t.side.to_uppercase(),
                format!("{:.4}", t.size),
                format!("{:.4}", t.price),
            ]
        })
        .collect();
    let table = build_table(
        &["TIME", "MARKET", "SIDE", "SIZE", "PRICE"],
        &[
            Align::Left,
            Align::Left,
            Align::Left,
            Align::Right,
            Align::Right,
        ],
        &rows,
    );
    format!("{title}\n\n{}", esc(&table))
}

fn render_profile_page(d: &UserDetail, rank: Option<usize>) -> String {
    let summary = render_summary_block(d, rank);
    let chart = svg_chart(&d.history);
    let positions = render_positions_block(&d.positions);
    let trades = render_trades_block(&d.trades);

    let body = format!(
        "<pre class=\"board\">{summary}</pre>\n\
         {chart}\n\
         <pre class=\"board\">{positions}</pre>\n\
         <pre class=\"board\">{trades}</pre>\n\
         <p class=\"nav\"><a href=\"/\">← back to leaderboard</a></p>",
        summary = summary,
        chart = chart,
        positions = positions,
        trades = trades,
    );
    document(
        &format!("predlab · {}", d.username),
        None,
        &body,
    )
}

fn render_profile_error(username: &str, msg: &str) -> String {
    let title = format!(
        "<span class=\"dim\">$</span> predlab member · {}",
        esc(username),
    );
    let body_pre = format!(
        "{title}\n\n  {}\n  <span class=\"dim\">{}</span>",
        esc("profile unavailable"),
        esc(msg),
    );
    let body = format!(
        "<pre class=\"board\">{body_pre}</pre>\n\
         <p class=\"nav\"><a href=\"/\">← back to leaderboard</a></p>",
        body_pre = body_pre,
    );
    document(
        &format!("predlab · {}", username),
        None,
        &body,
    )
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
    fn leaderboard_links_to_start_and_tui_pages() {
        let html = render_page(&[leader("alice", 25000.0)]);
        assert!(html.contains(r#"href="/start""#));
        assert!(html.contains(r#"href="/tui""#));
        // onboarding itself is NOT on the board page anymore
        assert!(!html.contains("pip install requests"));
    }

    #[test]
    fn start_page_has_onboarding_and_download_button() {
        let html = render_start_page();
        assert!(html.contains("get started"));
        assert!(html.contains(r#"href="/predlab.py""#));
        assert!(html.contains("download=\"predlab.py\""));
        assert!(html.contains("pip install requests"));
        assert!(html.contains(r#"href="/""#)); // back to leaderboard
        // the standalone page should not auto-refresh
        assert!(!html.contains("http-equiv=\"refresh\""));
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

    // --- profile page --------------------------------------------------

    #[test]
    fn leaderboard_username_is_a_link_to_profile() {
        let html = render_page(&[leader("teddy", 29444.44)]);
        assert!(html.contains(r#"<a href="/u/teddy">teddy</a>"#));
    }

    #[test]
    fn url_path_encode_preserves_safe_chars_and_encodes_unsafe() {
        assert_eq!(url_path_encode("teddy_42.bot-1"), "teddy_42.bot-1");
        assert_eq!(url_path_encode("a/b c"), "a%2Fb%20c");
        assert_eq!(url_path_encode("<script>"), "%3Cscript%3E");
    }

    #[test]
    fn fmt_time_short_drops_seconds_and_tz() {
        assert_eq!(fmt_time_short("2026-05-22T16:39:27.340254"), "2026-05-22 16:39");
        assert_eq!(fmt_time_short("2026-01-01T00:00:00"), "2026-01-01 00:00");
    }

    fn detail(history: Vec<HistoryPoint>) -> UserDetail {
        UserDetail {
            username: "teddy".into(),
            role: "member".into(),
            cash: 15000.0,
            positions_value: 14444.44,
            open_orders_value: 0.0,
            net_worth: 29444.44,
            positions: vec![DetailPosition {
                market_id: "631144".into(),
                market_question: Some("Will it rain?".into()),
                size: 2_222_222.0,
                avg_entry_price: Some(0.0045),
                current_price: 0.0065,
                unrealized_pnl: 4444.44,
            }],
            trades: vec![DetailTrade {
                market_id: "631144".into(),
                side: "buy".into(),
                price: 0.0045,
                size: 2_222_222.0,
                created_at: "2026-05-22T16:39:27".into(),
            }],
            history,
        }
    }

    fn pt(at: &str, nw: f64) -> HistoryPoint {
        HistoryPoint { at: at.into(), net_worth: nw }
    }

    #[test]
    fn svg_chart_renders_polyline_when_enough_points() {
        let svg = svg_chart(&[
            pt("2026-05-22T00:00:00", 25_000.0),
            pt("2026-05-23T00:00:00", 26_500.0),
            pt("2026-05-24T00:00:00", 29_444.44),
        ]);
        assert!(svg.contains("<svg") && svg.contains("</svg>"));
        assert!(svg.contains("<polyline"));
        // y-axis labels show money-formatted ticks (padded so the exact value
        // depends on the chart's headroom; just check the formatter ran).
        assert!(svg.matches("$").count() >= 3);
        // x-axis labels show the start/end dates.
        assert!(svg.contains("2026-05-22") && svg.contains("2026-05-24"));
    }

    #[test]
    fn svg_chart_falls_back_when_sparse() {
        assert!(svg_chart(&[]).contains("collecting"));
        assert!(svg_chart(&[pt("2026-05-22T00:00:00", 25_000.0)]).contains("collecting"));
    }

    #[test]
    fn svg_chart_handles_flat_history_without_panic() {
        let svg = svg_chart(&[
            pt("2026-05-22T00:00:00", 25_000.0),
            pt("2026-05-23T00:00:00", 25_000.0),
        ]);
        assert!(svg.contains("<polyline"));
    }

    #[test]
    fn profile_page_has_summary_chart_positions_trades_and_back_link() {
        let html = render_profile_page(
            &detail(vec![
                pt("2026-05-22T00:00:00", 25_000.0),
                pt("2026-05-24T00:00:00", 29_444.44),
            ]),
            Some(1),
        );
        assert!(html.contains("predlab member · teddy"));
        assert!(html.contains("#1"));
        assert!(html.contains("$29,444.44"));
        assert!(html.contains("<svg") && html.contains("</svg>"));
        assert!(html.contains("POSITIONS") || html.contains("# positions"));
        assert!(html.contains("UNREALIZED"));
        assert!(html.contains("$4,444.44")); // unrealized P&L
        assert!(html.contains("# recent trades"));
        assert!(html.contains("BUY"));
        assert!(html.contains(r#"href="/""#)); // back to leaderboard
        // No auto-refresh on the profile page.
        assert!(!html.contains("http-equiv=\"refresh\""));
    }

    #[test]
    fn profile_handles_empty_positions_and_trades() {
        let mut d = detail(vec![]);
        d.positions.clear();
        d.trades.clear();
        let html = render_profile_page(&d, None);
        assert!(html.contains("no open positions"));
        assert!(html.contains("no trades yet"));
        assert!(html.contains("collecting net-worth points"));
    }

    #[test]
    fn profile_error_page_includes_username_and_back_link() {
        let html = render_profile_error("teddy", "boom");
        assert!(html.contains("predlab member · teddy"));
        assert!(html.contains("boom"));
        assert!(html.contains(r#"href="/""#));
    }

    // --- TUI install page ---------------------------------------------

    #[test]
    fn tui_page_has_install_run_and_keys_sections() {
        let html = render_tui_page();
        // install command for the terminal client
        assert!(html.contains("cargo install --git"));
        assert!(html.contains("predlab-tui"));
        // run instructions reference the env var
        assert!(html.contains("POLY_API_KEY"));
        // vim keys are documented
        assert!(html.contains("gg") && html.contains(": cmd"));
        // back-to-leaderboard nav
        assert!(html.contains(r#"href="/""#));
        // and a cross-link to the python client onboarding
        assert!(html.contains(r#"href="/start""#));
        // no auto-refresh on a static page
        assert!(!html.contains("http-equiv=\"refresh\""));
    }

    #[test]
    fn start_page_links_to_tui_install() {
        let html = render_start_page();
        assert!(html.contains(r#"href="/tui""#));
    }

    #[test]
    fn leader_json_struct_serializes_with_rank() {
        let l = LeaderJson { rank: 1, username: "teddy".into(), net_worth: 29_444.44 };
        let s = serde_json::to_string(&l).unwrap();
        assert!(s.contains(r#""rank":1"#));
        assert!(s.contains(r#""username":"teddy""#));
        assert!(s.contains(r#""net_worth":29444.44"#));
    }
}
