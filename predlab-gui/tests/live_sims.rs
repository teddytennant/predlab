//! Live end-to-end tests against a *running* local stack.
//!
//! Everything here is `#[ignore]` so the offline CI suite stays green. Bring
//! the stack up from the repo root first (sim on :8001, leaderboard on
//! :8003, dev secrets):
//!
//! ```sh
//! ENVIRONMENT=development ADMIN_SECRET=change-me-in-prod-for-club \
//!   docker compose up -d --build
//! cargo test -p predlab-gui --test live_sims -- --ignored --test-threads=1
//! ```
//!
//! Environment knobs (all optional):
//! - `PREDLAB_LIVE_POLY_URL`          default `http://127.0.0.1:8001`
//! - `PREDLAB_LIVE_LEADERBOARD_URL`   default `http://127.0.0.1:8003`
//! - `PREDLAB_LIVE_ADMIN_SECRET`      default `change-me-in-prod-for-club`
//!   (the sim's dev-mode default master secret)
//!
//! `--test-threads=1` keeps server-side state (users, orders) deterministic
//! between scenarios.

use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use predlab_gui::config::Config;
use predlab_gui::data::Snapshot;
use predlab_gui::domain::leaderboard::LeaderboardClient;
use predlab_gui::domain::polymarket::{PolyClient, PolyMarket};
use predlab_gui::domain::Http;
use predlab_gui::engine::EngineManager;
use predlab_gui::message::{ActionKind, Command, ConnState, EngineMessage, OrderSide, Role, UiMessage};

fn env_or(key: &str, default: &str) -> String {
    match std::env::var(key) {
        Ok(v) if !v.is_empty() => v,
        _ => default.to_string(),
    }
}

/// A config pointing at the live stack, mirroring what the GUI would hold.
fn live_config() -> Config {
    let mut cfg = Config {
        onboarded: true,
        poly_url: env_or("PREDLAB_LIVE_POLY_URL", "http://127.0.0.1:8001"),
        leaderboard_url: env_or("PREDLAB_LIVE_LEADERBOARD_URL", "http://127.0.0.1:8003"),
        poly_admin_secret: env_or("PREDLAB_LIVE_ADMIN_SECRET", "change-me-in-prod-for-club"),
        tick_seconds: 1,
        ..Config::default()
    };
    cfg.clamp();
    cfg
}

/// Fresh username per test run so repeated runs never collide server-side.
fn unique(prefix: &str) -> String {
    format!("{prefix}_{}", &uuid::Uuid::new_v4().simple().to_string()[..12])
}

/// Pick a market that exposes clob token ids (index 0 = YES).
fn tradeable_market(markets: &[PolyMarket]) -> &PolyMarket {
    markets
        .iter()
        .find(|m| m.clob_token_ids.as_ref().is_some_and(|t| !t.is_empty()))
        .expect("at least one market has clob token ids")
}

struct EngineHandle {
    tx: Sender<EngineMessage>,
    rx: Receiver<UiMessage>,
    snapshot: Arc<Mutex<Snapshot>>,
    thread: std::thread::JoinHandle<()>,
}

impl EngineHandle {
    fn spawn(config: Config) -> Self {
        let (tx, engine_rx) = mpsc::channel();
        let (engine_tx, rx) = mpsc::channel();
        let snapshot = Arc::new(Mutex::new(Snapshot::default()));
        let snap = Arc::clone(&snapshot);
        let thread =
            std::thread::spawn(move || EngineManager::run(engine_rx, engine_tx, snap, config));
        Self { tx, rx, snapshot, thread }
    }

    /// Drain UI messages until the `ActionResult` for `kind` arrives; every
    /// other message seen along the way is returned for inspection.
    fn wait_for_result(
        &self,
        kind: ActionKind,
        timeout: Duration,
    ) -> (bool, String, Vec<UiMessage>) {
        let deadline = Instant::now() + timeout;
        let mut seen = Vec::new();
        while Instant::now() < deadline {
            match self.rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
                Ok(UiMessage::ActionResult { kind: k, ok, detail }) if k == kind => {
                    return (ok, detail, seen);
                }
                Ok(other) => seen.push(other),
                Err(_) => break,
            }
        }
        panic!("timed out waiting for ActionResult for {kind:?}; saw {} other messages", seen.len());
    }

    fn shutdown(self) {
        self.tx
            .send(EngineMessage::Shutdown)
            .expect("engine channel open");
        self.thread.join().expect("engine thread joins cleanly");
    }
}

// ---------------------------------------------------------------------------
// (a) Health
// ---------------------------------------------------------------------------

#[test]
#[ignore = "requires the running local stack"]
fn a_health() {
    let http = Http::new();
    let cfg = live_config();
    let health = PolyClient::new(&http, &cfg).health().expect("/health reachable");
    assert_eq!(health.status, "ok");
    assert!(!health.version.is_empty(), "health carries a version");
    assert_eq!(health.environment, "development", "stack must run dev mode");
}

// ---------------------------------------------------------------------------
// (b) Admin key issuance: master secret path, then the admin-role-key path
// ---------------------------------------------------------------------------

#[test]
#[ignore = "requires the running local stack"]
fn b_issue_keys_via_secret_and_via_admin_role_key() {
    let http = Http::new();
    let cfg = live_config();
    let owner = PolyClient::new(&http, &cfg);

    // Member key minted with X-Admin-Secret (owner rank).
    let member_name = unique("live_member");
    let member = owner
        .create_paper_key(&member_name, "Live Member", Role::Member)
        .expect("create member key via X-Admin-Secret");
    assert_eq!(member.username, member_name);
    assert_eq!(member.role, "member");
    assert!(member.api_key.starts_with("pm_paper_"), "key shape: {}", member.api_key);
    assert!(!member.secret.is_empty(), "one-time secret returned");

    // Admin key minted the same way (only owner rank may grant admin).
    let admin_name = unique("live_admin");
    let admin_key = owner
        .create_paper_key(&admin_name, "Live Admin", Role::Admin)
        .expect("create admin key via X-Admin-Secret");
    assert_eq!(admin_key.role, "admin");

    // Now authenticate with the ADMIN-ROLE KEY only (no master secret) and
    // mint another member — proves the role path the GUI relies on.
    let admin_cfg = Config {
        poly_api_key: admin_key.api_key.clone(),
        poly_admin_secret: String::new(),
        ..live_config()
    };
    let admin = PolyClient::new(&http, &admin_cfg);
    let second_name = unique("live_member2");
    let second = admin
        .create_paper_key(&second_name, "Live Member Two", Role::Member)
        .expect("admin-role key can mint member keys");
    assert_eq!(second.role, "member");

    // An admin-role key must NOT be able to grant admin (owner only).
    let err = admin
        .create_paper_key(&unique("live_escalate"), "Nope", Role::Admin)
        .expect_err("admin-role key cannot grant admin");
    assert_eq!(err.status(), Some(403), "got: {err}");

    // A member-role key must not pass the admin gate at all.
    let member_cfg = Config {
        poly_api_key: member.api_key.clone(),
        poly_admin_secret: String::new(),
        ..live_config()
    };
    let member_client = PolyClient::new(&http, &member_cfg);
    let err = member_client
        .admin_leaderboard()
        .expect_err("member key is not admin-capable");
    assert!(matches!(err.status(), Some(401) | Some(403)), "got: {err}");
}

// ---------------------------------------------------------------------------
// (c) Markets list, q search, book fetch
// ---------------------------------------------------------------------------

#[test]
#[ignore = "requires the running local stack"]
fn c_markets_search_and_book() {
    let http = Http::new();
    let cfg = live_config();
    let client = PolyClient::new(&http, &cfg);

    let markets = client.markets(50, 0, "").expect("GET /markets");
    assert!(!markets.is_empty(), "sim synced live data; markets must be non-empty");
    let market = tradeable_market(&markets);
    let token_id = market.clob_token_ids.as_ref().unwrap()[0].clone();

    // q search: a word from a known question must find that market.
    let word = market
        .question
        .split_whitespace()
        .max_by_key(|w| w.len())
        .unwrap()
        .trim_matches(|c: char| !c.is_alphanumeric())
        .to_string();
    let found = client.markets(50, 0, &word).expect("GET /markets?q=");
    assert!(
        found
            .iter()
            .any(|m| m.question.to_lowercase().contains(&word.to_lowercase())),
        "search {word:?} must return matching markets (got {})",
        found.len()
    );

    // Offset pagination advances through the catalog.
    let first_page = client.markets(5, 0, "").expect("page 0");
    let second_page = client.markets(5, 5, "").expect("page 1");
    if second_page.len() == 5 {
        assert_ne!(
            first_page.first().map(|m| &m.id),
            second_page.first().map(|m| &m.id),
            "offset paging returns a different slice"
        );
    }

    // Order book for a real token (well-formed even when empty), plus the
    // midpoint / spread quotes the detail pane shows.
    let book = client.book(&token_id).expect("GET /book");
    assert_eq!(book.asset_id.as_deref(), Some(token_id.as_str()));
    let mid = client.midpoint(&token_id).expect("GET /midpoint");
    assert!(!mid.midpoint.is_empty());
    let spread = client.spread(&token_id).expect("GET /spread");
    assert!(!spread.spread.is_empty());
}

// ---------------------------------------------------------------------------
// (d) Member order lifecycle: order -> portfolio escrow -> orders -> cancel
// ---------------------------------------------------------------------------

#[test]
#[ignore = "requires the running local stack"]
fn d_order_lifecycle_and_portfolio_escrow() {
    let http = Http::new();
    let cfg = live_config();
    let owner = PolyClient::new(&http, &cfg);

    let markets = owner.markets(50, 0, "").expect("GET /markets");
    let token_id = tradeable_market(&markets).clob_token_ids.as_ref().unwrap()[0].clone();

    // Fresh member for a deterministic starting balance.
    let username = unique("live_trader");
    let key = owner
        .create_paper_key(&username, "Live Trader", Role::Member)
        .expect("mint trader key");
    let trader_cfg = Config {
        poly_api_key: key.api_key.clone(),
        poly_admin_secret: String::new(),
        ..live_config()
    };
    let trader = PolyClient::new(&http, &trader_cfg);

    let before = trader.portfolio().expect("GET /portfolio before");
    assert!(before.cash > 0.0, "fresh member gets a starting balance");
    assert_eq!(before.open_orders_value, 0.0);

    // Resting limit buy far from any plausible ask: escrows price*size.
    let placed = trader
        .place_order(&token_id, OrderSide::Buy, Some(0.02), 5.0)
        .expect("POST /order");
    assert!(placed.success, "order accepted: {:?}", placed.error_msg);
    assert!(!placed.order_id.is_empty() && placed.order_id != "0");

    // /portfolio reflects the escrow: cash down, open_orders_value up,
    // net worth unchanged (escrow still counts toward the score).
    let during = trader.portfolio().expect("GET /portfolio during");
    assert!(during.cash < before.cash, "cash decreased: {} -> {}", before.cash, during.cash);
    assert!(during.open_orders_value > 0.0, "escrow visible: {}", during.open_orders_value);
    assert!(
        (during.net_worth - before.net_worth).abs() < 0.01,
        "net worth stable across escrow: {} -> {}",
        before.net_worth,
        during.net_worth
    );

    // It shows up in /user/orders.
    let orders = trader.user_orders().expect("GET /user/orders");
    let mine = orders
        .iter()
        .find(|o| o.id.to_string() == placed.order_id)
        .expect("placed order listed");
    assert_eq!(mine.side, "buy");
    assert_eq!(mine.size, 5.0);
    assert_eq!(mine.price, Some(0.02));

    // Positions endpoint answers (no fills expected on a resting order).
    let positions = trader.positions().expect("GET /positions");
    assert!(positions.iter().all(|p| p.size >= 0.0));

    // Cancel releases the escrow.
    let cancelled = trader.cancel_order(&placed.order_id).expect("DELETE /order");
    assert!(cancelled.success);
    assert_eq!(cancelled.order_id, placed.order_id);
    assert_eq!(cancelled.status, "cancelled");

    let after = trader.user_orders().expect("GET /user/orders after cancel");
    assert!(
        !after.iter().any(|o| o.id.to_string() == placed.order_id),
        "cancelled order no longer open"
    );
    let restored = trader.portfolio().expect("GET /portfolio after cancel");
    assert!(
        (restored.cash - before.cash).abs() < 0.01,
        "escrow released: {} vs {}",
        restored.cash,
        before.cash
    );
}

// ---------------------------------------------------------------------------
// (e) Admin roster, member detail (history), reset, revoke
// ---------------------------------------------------------------------------

#[test]
#[ignore = "requires the running local stack"]
fn e_admin_roster_detail_reset_and_revoke() {
    let http = Http::new();
    let cfg = live_config();
    let owner = PolyClient::new(&http, &cfg);

    let username = unique("live_roster");
    let key = owner
        .create_paper_key(&username, "Live Roster", Role::Member)
        .expect("mint key");

    // /admin/leaderboard contains the new user with a member role.
    let roster = owner.admin_leaderboard().expect("GET /admin/leaderboard");
    let row = roster
        .iter()
        .find(|r| r.username == username)
        .expect("new user on the admin roster");
    assert_eq!(row.role, "member");
    assert!(row.net_worth > 0.0);

    // Trade once so the balance drifts, then check the detail endpoint.
    let markets = owner.markets(50, 0, "").expect("markets");
    let token_id = tradeable_market(&markets).clob_token_ids.as_ref().unwrap()[0].clone();
    let trader_cfg = Config {
        poly_api_key: key.api_key.clone(),
        poly_admin_secret: String::new(),
        ..live_config()
    };
    let trader = PolyClient::new(&http, &trader_cfg);
    let placed = trader
        .place_order(&token_id, OrderSide::Buy, Some(0.02), 5.0)
        .expect("resting order");
    assert!(placed.success);

    // /admin/user/{username} returns the full profile shape.
    let detail = owner.admin_user(&username).expect("GET /admin/user/{username}");
    assert_eq!(detail.username, username);
    assert_eq!(detail.role, "member");
    assert!((detail.open_orders_value - 0.1).abs() < 0.01, "escrow visible in detail");

    // Reset restores the starting state (no open orders, full cash).
    owner.reset_balance(Some(&username)).expect("POST /admin/reset-balance");
    let after_reset = trader.portfolio().expect("portfolio after reset");
    assert_eq!(after_reset.open_orders_value, 0.0, "reset cancelled the order");
    assert!(
        (after_reset.net_worth - after_reset.cash).abs() < 0.01,
        "reset member is all cash"
    );

    // The sim drops a net-worth history point on fills / resets / its 60s
    // tick — the reset above guarantees at least one for the graph.
    let detail = owner.admin_user(&username).expect("detail after reset");
    assert!(!detail.history.is_empty(), "history point recorded by the reset");
    assert!(detail.history.iter().all(|h| !h.t.is_empty()));

    // Revoke, then the key must 401.
    let revoked = owner.revoke_key(&username).expect("POST /admin/revoke-key");
    assert_eq!(revoked.revoked, username);
    assert!(revoked.keys_disabled >= 1);
    let err = trader.portfolio().expect_err("revoked key rejected");
    assert_eq!(err.status(), Some(401), "got: {err}");
}

// ---------------------------------------------------------------------------
// (f) Public leaderboard site: leaderboard.json + profile proxy
// ---------------------------------------------------------------------------

#[test]
#[ignore = "requires the running local stack"]
fn f_public_leaderboard_and_profile() {
    let http = Http::new();
    let cfg = live_config();
    let owner = PolyClient::new(&http, &cfg);
    let public = LeaderboardClient::new(&http, &cfg);

    // A fresh member must appear on the public board (dev compose excludes
    // only club_admin/demo_trader).
    let username = unique("live_public");
    owner
        .create_paper_key(&username, "Live Public", Role::Member)
        .expect("mint key");
    // Force one net-worth history point (the sim records them on fills,
    // resets and its periodic tick — not at account creation).
    owner.reset_balance(Some(&username)).expect("seed a history point");

    let rows = public.leaderboard().expect("GET /leaderboard.json");
    assert!(!rows.is_empty());
    assert!(rows.iter().all(|r| r.rank >= 1), "ranks are 1-based");
    let row = rows
        .iter()
        .find(|r| r.username == username)
        .expect("new member visible on the public board");
    assert!(row.net_worth > 0.0);

    // Row click path: the public profile proxy returns the member detail.
    let profile = public.user_profile(&username).expect("GET /api/user/:username");
    assert_eq!(profile.username, username);
    assert!(profile.net_worth > 0.0);
    assert!(!profile.history.is_empty(), "profile carries the history series");

    // Excluded (staff/seed) users are hidden: 404. Unknown users surface as
    // a 502 from the sim proxy. Both land in the GUI's profile error string.
    let err = public
        .user_profile("demo_trader")
        .expect_err("excluded user hidden");
    assert_eq!(err.status(), Some(404), "got: {err}");
    let err = public
        .user_profile("definitely_not_a_member_xyz")
        .expect_err("unknown user rejected");
    assert_eq!(err.status(), Some(502), "got: {err}");
}

// ---------------------------------------------------------------------------
// (g) Engine loop smoke: real thread, real tick, snapshot fills
// ---------------------------------------------------------------------------

#[test]
#[ignore = "requires the running local stack"]
fn g_engine_loop_smoke() {
    let http = Http::new();
    let cfg = live_config();

    // Mint a member key so the portfolio section is exercised too. The
    // engine config keeps the master secret, so the admin probe must land
    // admin_ok = Some(true) and fill the roster.
    let owner = PolyClient::new(&http, &cfg);
    let username = unique("live_smoke");
    let key = owner
        .create_paper_key(&username, "Engine Smoke", Role::Member)
        .expect("paper key for the engine");
    let engine_cfg = Config {
        poly_api_key: key.api_key,
        ..live_config()
    };

    let engine = EngineHandle::spawn(engine_cfg);
    engine
        .tx
        .send(EngineMessage::Command(Command::RefreshAll))
        .unwrap();
    let (ok, detail, seen) = engine.wait_for_result(ActionKind::RefreshAll, Duration::from_secs(30));
    assert!(ok, "RefreshAll reports success: {detail}");

    // Let a couple of 1s ticks run on top of the explicit refresh.
    std::thread::sleep(Duration::from_secs(3));

    // Connection status flowed to the UI channel.
    let mut messages = seen;
    while let Ok(m) = engine.rx.try_recv() {
        messages.push(m);
    }
    let status = messages
        .iter()
        .rev()
        .find_map(|m| match m {
            UiMessage::Status { poly } => Some(poly.clone()),
            _ => None,
        })
        .expect("engine emitted Status");
    assert!(matches!(status, ConnState::Connected(_)), "sim: {status:?}");

    // The snapshot filled with live data and no section errors.
    {
        let snap = engine.snapshot.lock().unwrap();
        assert!(!snap.markets.is_empty(), "markets fetched");
        assert!(snap.portfolio.is_some(), "portfolio present");
        assert!(!snap.leaderboard.is_empty(), "public leaderboard fetched");
        assert_eq!(snap.admin_ok, Some(true), "admin probe succeeded via the master secret");
        assert!(
            snap.roster.iter().any(|r| r.username == username),
            "admin roster fetched and contains the smoke user"
        );
        assert!(snap.last_updated.is_some(), "snapshot stamped");
        let e = &snap.errors;
        for (name, err) in [
            ("markets", &e.markets),
            ("portfolio", &e.portfolio),
            ("leaderboard", &e.leaderboard),
            ("roster", &e.roster),
        ] {
            assert!(err.is_none(), "section {name} errored: {err:?}");
        }
    }

    // A member-only engine (no secret) must probe to admin_ok = false.
    engine.shutdown();

    let member_cfg = Config {
        poly_admin_secret: String::new(),
        ..live_config()
    };
    // No API key either: the engine should immediately mark non-admin.
    let engine = EngineHandle::spawn(member_cfg);
    engine
        .tx
        .send(EngineMessage::Command(Command::RefreshAll))
        .unwrap();
    let (ok, detail, _) = engine.wait_for_result(ActionKind::RefreshAll, Duration::from_secs(30));
    assert!(ok, "RefreshAll (anonymous) reports success: {detail}");
    {
        let snap = engine.snapshot.lock().unwrap();
        assert_eq!(snap.admin_ok, Some(false), "anonymous engine is not admin-capable");
        assert!(snap.roster.is_empty(), "no roster without admin rank");
        assert!(!snap.markets.is_empty(), "public data still flows");
    }
    engine.shutdown();
}
