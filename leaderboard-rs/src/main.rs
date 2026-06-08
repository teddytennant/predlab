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
    extract::{Path, Query, State},
    http::{header, StatusCode},
    response::{Html, IntoResponse, Json, Response},
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

struct Leader {
    username: String,
    net_worth: f64,
    /// Free (un-escrowed) cash — used for the club-stats aggregate only.
    cash: f64,
    /// Mark-to-market value of open positions — used for club stats.
    positions_value: f64,
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
#[derive(Debug, Deserialize, Serialize)]
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

#[derive(Debug, Deserialize, Serialize)]
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

#[derive(Debug, Deserialize, Serialize)]
struct DetailTrade {
    market_id: String,
    side: String,
    price: f64,
    size: f64,
    created_at: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct HistoryPoint {
    #[serde(rename = "t")]
    at: String,
    net_worth: f64,
}

/// One row of the sim's public `GET /markets` feed — the subset the markets
/// browser renders. Field names are the camelCase Gamma shape.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Market {
    question: String,
    #[serde(default)]
    outcome_prices: Vec<String>,
    #[serde(default)]
    best_bid: Option<f64>,
    #[serde(default)]
    best_ask: Option<f64>,
    #[serde(default)]
    volume: Option<f64>,
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
    /// Each member's starting paper stake — the baseline for the P&L column
    /// and the "beating start" stat. Override with `START_BALANCE`.
    start_balance: f64,
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
            start_balance: env("START_BALANCE", "25000").parse().unwrap_or(25000.0),
            client: reqwest::Client::new(),
            cache: Arc::new(Mutex::new(Cache { html: String::new(), at: None })),
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let state = AppState::from_env();
    if state.admin_secret.is_empty() {
        eprintln!(
            "warning: PREDLAB_ADMIN_SECRET is unset — every standings fetch will be \
             rejected by the sim and all pages will show the error placeholder."
        );
    }
    let app = Router::new()
        .route("/", get(index))
        .route("/start", get(start))
        .route("/tui", get(tui_page))
        .route("/about", get(about_page))
        .route("/markets", get(markets_page))
        .route("/u/:username", get(profile))
        .route("/api/user/:username", get(profile_json))
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
        Ok(rows) => render_page(&rows, st.start_balance),
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
/// the public board, so no auth required. On a sim outage we return 502 (not
/// an empty 200) so clients can tell "no members" from "backend down".
async fn leaderboard_json(State(st): State<AppState>) -> Response {
    match fetch_leaders(&st).await {
        Ok(leaders) => {
            let rows: Vec<LeaderJson> = leaders
                .into_iter()
                .enumerate()
                .map(|(i, l)| LeaderJson {
                    rank: i + 1,
                    username: l.username,
                    net_worth: l.net_worth,
                })
                .collect();
            Json(rows).into_response()
        }
        Err(e) => (
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

/// Onboarding page for the terminal client: install / run / vim keys.
async fn tui_page() -> Html<String> {
    Html(render_tui_page())
}

/// Static rules / about page — what the game is and how the score works.
async fn about_page() -> Html<String> {
    Html(render_about_page())
}

/// `?q=` search and `?offset=` paging for the markets browser.
#[derive(Debug, Deserialize)]
struct MarketQuery {
    #[serde(default)]
    q: Option<String>,
    #[serde(default)]
    offset: Option<u32>,
}

/// Markets browser — what the club can actually trade, straight from the sim's
/// public `/markets` feed (no admin secret needed).
async fn markets_page(State(st): State<AppState>, Query(mq): Query<MarketQuery>) -> Html<String> {
    let q = mq.q.unwrap_or_default();
    let offset = mq.offset.unwrap_or(0);
    let html = match fetch_markets(&st, &q, offset).await {
        Ok(markets) => render_markets_page(&markets, &q, offset),
        Err(e) => document(
            "predlab · markets",
            None,
            &format!(
                "<pre class=\"board\">{}</pre>\n\
                 <p class=\"nav\"><a href=\"/\">← back to leaderboard</a></p>",
                esc(&format!("  markets temporarily unavailable\n  {e}")),
            ),
        ),
    };
    Html(html)
}

/// Per-member JSON twin of the `/u/:username` HTML profile — lets the TUI and
/// member scripts read the rich profile without scraping HTML. Re-serves only
/// what the public profile page already renders.
async fn profile_json(Path(username): Path<String>, State(st): State<AppState>) -> Response {
    if st.exclude.contains(&username) {
        return (StatusCode::NOT_FOUND, Json(serde_json::json!({ "error": "no such member" })))
            .into_response();
    }
    match fetch_user_detail(&st, &username).await {
        Ok(detail) => Json(detail).into_response(),
        Err(e) => (
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

/// Fetch the club's tradable markets from the sim's public `/markets` feed.
async fn fetch_markets(st: &AppState, q: &str, offset: u32) -> Result<Vec<Market>> {
    let offset_s = offset.to_string();
    let mut params: Vec<(&str, &str)> =
        vec![("active", "true"), ("limit", "50"), ("offset", &offset_s)];
    if !q.is_empty() {
        params.push(("q", q));
    }
    st.client
        .get(format!("{}/markets", st.poly_url))
        .query(&params)
        .send()
        .await
        .context("calling polymarket /markets")?
        .error_for_status()
        .context("polymarket /markets rejected")?
        .json()
        .await
        .context("parsing /markets response")
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
            let f = |k: &str| e.get(k).and_then(|n| n.as_f64()).unwrap_or(0.0);
            Some(Leader {
                username: u.to_string(),
                net_worth: f("net_worth"),
                cash: f("cash"),
                positions_value: f("positions_value"),
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

/// Format a profit/loss figure with an explicit leading sign, e.g. 4444.5 ->
/// "+$4,444.50", -1200.0 -> "-$1,200.00", 0.0 -> "$0.00".
fn fmt_pnl(v: f64) -> String {
    if v > 0.0 {
        format!("+{}", fmt_money(v))
    } else {
        fmt_money(v)
    }
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

/// One table cell. `visible` is the bare text that drives column width and
/// alignment; `html` is what we actually emit (already HTML-escaped). For most
/// cells the two carry the same text, but the leaderboard's member column keeps
/// the bare username in `visible` while `html` holds an `<a>` link — so the
/// link can't throw off padding and we never have to string-replace a rendered
/// table (which used to mislink numeric/duplicate usernames).
struct Cell {
    visible: String,
    html: String,
}

/// A plain auto-escaped cell (`visible` == unescaped text, `html` == escaped).
fn cell(s: &str) -> Cell {
    Cell { visible: s.to_string(), html: esc(s) }
}

/// Render an aligned, box-drawn monospace table from pre-built cells. Borders
/// are raw box-drawing characters; each cell's `html` is emitted verbatim and
/// padded by its `visible` width.
fn build_table_cells(headers: &[&str], aligns: &[Align], rows: &[Vec<Cell>]) -> String {
    let ncols = headers.len();
    let mut widths: Vec<usize> = headers.iter().map(|h| h.chars().count()).collect();
    for row in rows {
        for (i, c) in row.iter().enumerate() {
            widths[i] = widths[i].max(c.visible.chars().count());
        }
    }
    let border = |l: char, m: char, r: char| -> String {
        let mut s = String::new();
        s.push(l);
        for (i, w) in widths.iter().enumerate() {
            s.push_str(&"─".repeat(w + 2));
            s.push(if i + 1 == ncols { r } else { m });
        }
        s
    };
    let row_str = |cells: &[Cell]| -> String {
        let mut s = String::from("│");
        for (i, c) in cells.iter().enumerate() {
            let spaces = " ".repeat(widths[i].saturating_sub(c.visible.chars().count()));
            s.push(' ');
            match aligns[i] {
                Align::Left => {
                    s.push_str(&c.html);
                    s.push_str(&spaces);
                }
                Align::Right => {
                    s.push_str(&spaces);
                    s.push_str(&c.html);
                }
            }
            s.push_str(" │");
        }
        s
    };

    let header_cells: Vec<Cell> = headers.iter().map(|h| cell(h)).collect();
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

/// Convenience wrapper: build a table from plain string cells (auto-escaped).
/// The returned string is already HTML-safe — callers must NOT escape it again.
fn build_table(headers: &[&str], aligns: &[Align], rows: &[Vec<String>]) -> String {
    let cells: Vec<Vec<Cell>> = rows
        .iter()
        .map(|r| r.iter().map(|s| cell(s)).collect())
        .collect();
    build_table_cells(headers, aligns, &cells)
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
form.search { display: flex; gap: 8px; margin: 0; }
form.search input {
  flex: 1; background: #0c0c0c; border: 1px solid #333; border-radius: 3px;
  color: #f0f0f0; font: inherit; font-size: 14px; padding: 6px 10px;
}
form.search input::placeholder { color: #666; }
form.search button {
  background: transparent; border: 1px solid #f0f0f0; border-radius: 3px;
  color: #f0f0f0; font: inherit; font-size: 14px; padding: 6px 14px; cursor: pointer;
}
form.search button:hover { background: #f0f0f0; color: #000; }
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

/// Static rules / about copy. Answers the two questions new members always ask:
/// "is this real money?" and "how is my score calculated?". The net-worth
/// formula mirrors `compute_net_worth` in the sim so it stays accurate.
const ABOUT: &str = r##"<section class="onboard">
<h2><span class="dim">$</span> what is predlab?</h2>
<p>PredLab is the paper-trading playground for the <strong>NCSSM Prediction Markets Club</strong>. You trade Polymarket-style yes/no markets with <strong>$25,000 of fake money</strong>, practice real strategies, and climb a live club leaderboard.</p>
<p class="dim">Paper trading only · not affiliated with Polymarket · educational use · the money is fake, the prices are real.</p>

<h2 style="margin-top:18px"><span class="dim">$</span> how the markets work</h2>
<p>A market asks a yes/no question (“Will X happen?”). You buy <strong>YES</strong> or <strong>NO</strong> shares priced $0.01–$0.99 — the price <em>is</em> the market's implied probability. A winning share pays <strong>$1.00</strong> at resolution; a losing share pays <strong>$0</strong>. Prices are pulled live from the real Polymarket Gamma API, but there is <strong>no house market-maker</strong>: your order fills only when another member takes the other side.</p>

<h2 style="margin-top:18px"><span class="dim">$</span> how your score is computed</h2>
<pre class="snippet">net worth = free cash
          + open positions marked at the current price
          + cash escrowed in your resting buy orders</pre>
<p>That net-worth figure is your leaderboard score, and the <strong>P&amp;L</strong> column shows it against the $25,000 you started with. Your line on your profile page updates on every fill and every few minutes.</p>

<h2 style="margin-top:18px"><span class="dim">$</span> fair play &amp; AI</h2>
<p>AI use is <strong>unrestricted and encouraged</strong> — bring your own model, framework, or none at all. It's fake money and a learning sandbox: experiment freely, but don't try to break the sim for everyone else.</p>
</section>
<p class="nav"><a href="/">← back to leaderboard</a> · <a href="/markets">→ browse markets</a> · <a href="/start">→ get a key</a></p>"##;

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
         <p class=\"nav\"><a href=\"/markets\">→ browse markets</a> · \
         <a href=\"/about\">→ rules</a> · \
         <a href=\"/start\">→ get your key</a> · \
         <a href=\"/tui\">→ terminal client</a></p>",
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

/// Aggregate club metrics rendered as a kv block above the board — gives a
/// sense of the whole cohort, not just the top few. Reads only fields already
/// present in the `/admin/leaderboard` response.
fn club_stats(rows: &[Leader], start_balance: f64) -> String {
    let n = rows.len();
    let total: f64 = rows.iter().map(|l| l.net_worth).sum();
    let avg = total / n as f64;
    let mut nws: Vec<f64> = rows.iter().map(|l| l.net_worth).collect();
    nws.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median = if n % 2 == 1 {
        nws[n / 2]
    } else {
        (nws[n / 2 - 1] + nws[n / 2]) / 2.0
    };
    let beating = rows.iter().filter(|l| l.net_worth > start_balance).count();
    let cash: f64 = rows.iter().map(|l| l.cash).sum();
    let deployed: f64 = rows.iter().map(|l| l.positions_value).sum();
    let kv = format!(
        "  {:<14}{}\n  {:<14}{}\n  {:<14}{}\n  {:<14}{}\n  {:<14}{} / {}\n  {:<14}{}  (free cash {})",
        "members",
        n,
        "paper AUM",
        fmt_money(total),
        "avg net worth",
        fmt_money(avg),
        "median",
        fmt_money(median),
        "beating start",
        beating,
        n,
        "deployed",
        fmt_money(deployed),
        fmt_money(cash),
    );
    format!("<span class=\"dim\"># club stats</span>\n{}", esc(&kv))
}

fn render_page(rows: &[Leader], start_balance: f64) -> String {
    if rows.is_empty() {
        return page_shell(&esc("  (no members yet — check back once keys are issued)"));
    }
    // Each row's MEMBER cell carries the bare (truncated) username for width but
    // an `<a>` link for output — so numeric or duplicate-prefix usernames can no
    // longer collide with rank/money digits the way a flat string-replace did.
    let cells: Vec<Vec<Cell>> = rows
        .iter()
        .enumerate()
        .map(|(i, l)| {
            let disp = truncate(&l.username, 28);
            let link = format!(
                r#"<a href="/u/{}">{}</a>"#,
                url_path_encode(&l.username),
                esc(&disp),
            );
            vec![
                cell(&(i + 1).to_string()),
                Cell { visible: disp, html: link },
                cell(&fmt_money(l.net_worth)),
                cell(&fmt_pnl(l.net_worth - start_balance)),
            ]
        })
        .collect();
    let table = build_table_cells(
        &["#", "MEMBER", "NET WORTH", "P&L"],
        &[Align::Right, Align::Left, Align::Right, Align::Right],
        &cells,
    );
    let stats = club_stats(rows, start_balance);
    page_shell(&format!("{stats}\n\n{table}"))
}

/// The markets browser page: the club's tradable catalog as a terminal table,
/// with a no-JS search box and prev/next paging.
fn render_markets_page(markets: &[Market], q: &str, offset: u32) -> String {
    let table = if markets.is_empty() {
        esc("  (no markets match — try a different search)")
    } else {
        let rows: Vec<Vec<String>> = markets
            .iter()
            .map(|m| {
                let yes = m
                    .outcome_prices
                    .first()
                    .and_then(|s| s.parse::<f64>().ok())
                    .map(|p| format!("{:.0}%", p * 100.0))
                    .unwrap_or_else(|| "—".to_string());
                let spread = match (m.best_bid, m.best_ask) {
                    (Some(b), Some(a)) if a >= b => format!("{:.0}¢", (a - b) * 100.0),
                    _ => "—".to_string(),
                };
                let vol = m.volume.map(fmt_money).unwrap_or_else(|| "—".to_string());
                vec![truncate(&m.question, 52), yes, spread, vol]
            })
            .collect();
        build_table(
            &["MARKET", "YES", "SPREAD", "VOLUME"],
            &[Align::Left, Align::Right, Align::Right, Align::Right],
            &rows,
        )
    };

    // No-JS search form + prev/next paging links.
    let search = format!(
        "<form class=\"search\" method=\"get\" action=\"/markets\">\
         <input type=\"text\" name=\"q\" value=\"{}\" placeholder=\"search markets…\" autocomplete=\"off\">\
         <button type=\"submit\">search</button></form>",
        esc(q),
    );
    let qenc = url_path_encode(q);
    let mut pager = String::new();
    if offset >= 50 {
        pager.push_str(&format!(
            "<a href=\"/markets?q={}&offset={}\">← prev</a>  ",
            qenc,
            offset - 50,
        ));
    }
    if markets.len() == 50 {
        pager.push_str(&format!(
            "<a href=\"/markets?q={}&offset={}\">next →</a>",
            qenc,
            offset + 50,
        ));
    }

    let board = format!(
        "<span class=\"dim\">$</span> predlab markets\n\n{}\n\n\
         <span class=\"dim\"># live Polymarket prices · YES = implied probability · paper trading only</span>",
        table,
    );
    let body = format!(
        "{search}\n<pre class=\"board\">{board}</pre>\n\
         <p class=\"nav\">{pager}</p>\n\
         <p class=\"nav\"><a href=\"/\">← back to leaderboard</a> · \
         <a href=\"/start\">→ get a key &amp; trade</a></p>",
    );
    document("predlab · markets", None, &body)
}

/// Static rules / about page: what the game is and exactly how the score works.
fn render_about_page() -> String {
    document("predlab · rules", None, ABOUT)
}

fn render_error(msg: &str) -> String {
    page_shell(&esc(&format!("  standings temporarily unavailable\n  {msg}")))
}

// ---------------------------------------------------------------------------
// Per-user profile page  (/u/:username)
// ---------------------------------------------------------------------------

async fn profile(Path(username): Path<String>, State(st): State<AppState>) -> Html<String> {
    // Staff/seed accounts are hidden from the public board (EXCLUDE_USERS); keep
    // them off the public profile route too rather than leaking their portfolio.
    if st.exclude.contains(&username) {
        return Html(render_profile_error(&username, "no such member"));
    }
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
    format!("{title}\n\n{table}")
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
    format!("{title}\n\n{table}")
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
        Leader { username: name.into(), net_worth: net, cash: net, positions_value: 0.0 }
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
        let html = render_page(&rows, 25000.0);
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
        assert!(render_page(&[], 25000.0).contains("no members yet"));
    }

    #[test]
    fn board_has_pnl_column_and_club_stats() {
        let rows = vec![leader("alice", 30000.0), leader("bob", 20000.0)];
        let html = render_page(&rows, 25000.0);
        // P&L column header (escaped) + a signed gain and an (unsigned) loss
        assert!(html.contains("P&amp;L"));
        assert!(html.contains("+$5,000.00")); // alice up 5k
        assert!(html.contains("-$5,000.00")); // bob down 5k
        // club-stats block
        assert!(html.contains("# club stats"));
        assert!(html.contains("members"));
        assert!(html.contains("paper AUM"));
        assert!(html.contains("$50,000.00")); // total AUM
        assert!(html.contains("beating start"));
        assert!(html.contains("1 / 2")); // only alice beats the $25k start
    }

    #[test]
    fn numeric_username_links_to_its_own_profile_not_rank_digits() {
        // A member literally named "1" must not have the rank-1 digit linkified
        // out from under it (the old flat string-replace bug).
        let html = render_page(&[leader("1", 30000.0), leader("2", 20000.0)], 25000.0);
        assert!(html.contains(r#"<a href="/u/1">1</a>"#));
        assert!(html.contains(r#"<a href="/u/2">2</a>"#));
    }

    #[test]
    fn leaderboard_links_to_start_and_tui_pages() {
        let html = render_page(&[leader("alice", 25000.0)], 25000.0);
        assert!(html.contains(r#"href="/start""#));
        assert!(html.contains(r#"href="/tui""#));
        assert!(html.contains(r#"href="/markets""#));
        assert!(html.contains(r#"href="/about""#));
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
        let html = render_page(&[leader("<script>", 0.0)], 25000.0);
        assert!(!html.contains("<script>"));
        assert!(html.contains("&lt;script&gt;"));
    }

    // --- profile page --------------------------------------------------

    #[test]
    fn leaderboard_username_is_a_link_to_profile() {
        let html = render_page(&[leader("teddy", 29444.44)], 25000.0);
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

    // --- pnl / markets / about -----------------------------------------

    #[test]
    fn fmt_pnl_signs_gains_and_losses() {
        assert_eq!(fmt_pnl(4444.5), "+$4,444.50");
        assert_eq!(fmt_pnl(-1200.0), "-$1,200.00");
        assert_eq!(fmt_pnl(0.0), "$0.00");
    }

    fn market(question: &str, yes: &str, bid: Option<f64>, ask: Option<f64>, vol: Option<f64>) -> Market {
        Market {
            question: question.into(),
            outcome_prices: vec![yes.into(), "0.5".into()],
            best_bid: bid,
            best_ask: ask,
            volume: vol,
        }
    }

    #[test]
    fn markets_page_renders_table_search_and_questions() {
        let ms = vec![
            market("Will it rain tomorrow?", "0.62", Some(0.60), Some(0.64), Some(12345.0)),
            market("Will the bill pass?", "0.10", None, None, None),
        ];
        let html = render_markets_page(&ms, "rain", 0);
        assert!(html.contains("predlab markets"));
        assert!(html.contains("MARKET") && html.contains("YES") && html.contains("SPREAD"));
        assert!(html.contains("Will it rain tomorrow?"));
        assert!(html.contains("62%")); // YES price as a percent
        assert!(html.contains("$12,345.00")); // volume
        assert!(html.contains("—")); // missing bid/ask -> em dash spread
        // search box echoes the query (escaped) and no auto-refresh
        assert!(html.contains(r#"value="rain""#));
        assert!(!html.contains("http-equiv=\"refresh\""));
    }

    #[test]
    fn markets_page_paging_links_appear_only_when_warranted() {
        // first page, fewer than a full page -> no prev, no next
        let one = vec![market("q", "0.5", None, None, None)];
        let html = render_markets_page(&one, "", 0);
        assert!(!html.contains("prev") && !html.contains("next"));
        // a full page at a later offset -> both prev and next
        let full: Vec<Market> = (0..50).map(|_| market("q", "0.5", None, None, None)).collect();
        let html = render_markets_page(&full, "", 50);
        assert!(html.contains("prev") && html.contains("next"));
        assert!(html.contains("offset=0") && html.contains("offset=100"));
    }

    #[test]
    fn empty_markets_render_placeholder() {
        let html = render_markets_page(&[], "zzz", 0);
        assert!(html.contains("no markets match"));
    }

    #[test]
    fn about_page_explains_money_and_score() {
        let html = render_about_page();
        assert!(html.contains("what is predlab"));
        assert!(html.contains("$25,000"));
        assert!(html.contains("net worth ="));
        assert!(html.contains("escrowed")); // the often-forgotten third term
        assert!(html.contains(r#"href="/""#)); // back to leaderboard
        assert!(!html.contains("http-equiv=\"refresh\"")); // static page
    }

    #[test]
    fn user_detail_serializes_for_json_profile() {
        let d = detail(vec![pt("2026-05-22T00:00:00", 25_000.0)]);
        let s = serde_json::to_string(&d).unwrap();
        assert!(s.contains(r#""username":"teddy""#));
        assert!(s.contains(r#""net_worth":29444.44"#));
        // history point keeps its wire key "t"
        assert!(s.contains(r#""t":"2026-05-22T00:00:00""#));
    }
}
