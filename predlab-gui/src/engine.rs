//! The engine thread: owns all HTTP, drains UI commands, polls the sim and
//! the leaderboard site on a tick, and writes results into the shared
//! [`Snapshot`].
//!
//! Network failures never panic or stop the loop — they land in the
//! per-section error strings and the next tick simply retries.

use std::sync::mpsc::{Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chrono::Utc;

use crate::config::Config;
use crate::data::{LoadedProfile, OrderBook, SelectedQuotes, Snapshot};
use crate::domain::leaderboard::LeaderboardClient;
use crate::domain::polymarket::PolyClient;
use crate::domain::{ApiError, Http};
use crate::message::{ActionKind, Command, ConnState, EngineMessage, UiMessage};

/// How often the loop wakes to drain the command channel.
const IDLE_SLEEP: Duration = Duration::from_millis(100);

/// Engine state: current config plus what the user has selected/searched.
pub struct EngineManager {
    config: Config,
    http: Http,
    tx: Sender<UiMessage>,
    snapshot: Arc<Mutex<Snapshot>>,
    search: String,
    markets_offset: u32,
    selected_token: Option<String>,
    /// Admin-capability cache; `None` = probe again on the next tick.
    admin_ok: Option<bool>,
}

impl EngineManager {
    /// Run the engine loop until `Shutdown` arrives or the channel closes.
    /// Spawn this on a dedicated `std::thread`.
    pub fn run(
        rx: Receiver<EngineMessage>,
        tx: Sender<UiMessage>,
        snapshot: Arc<Mutex<Snapshot>>,
        initial: Config,
    ) {
        let mut initial = initial;
        initial.clamp();
        let mut engine = EngineManager {
            config: initial,
            http: Http::new(),
            tx,
            snapshot,
            search: String::new(),
            markets_offset: 0,
            selected_token: None,
            admin_ok: None,
        };

        let mut last_poll: Option<Instant> = None;
        loop {
            // 1. Drain everything the UI sent since the last wake.
            loop {
                match rx.try_recv() {
                    Ok(EngineMessage::Shutdown) => {
                        log::info!("engine: shutdown requested");
                        return;
                    }
                    Ok(EngineMessage::ConfigChanged(mut config)) => {
                        config.clamp();
                        engine.config = config;
                        // Credentials may have changed: re-probe admin rank
                        // and re-poll immediately with the new settings.
                        engine.set_admin_ok(None);
                        last_poll = None;
                    }
                    Ok(EngineMessage::Command(command)) => engine.handle_command(command),
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        log::info!("engine: UI channel closed, exiting");
                        return;
                    }
                }
            }

            // 2. Poll on tick expiry.
            let tick = Duration::from_secs(engine.config.tick_seconds);
            if last_poll.is_none_or(|t| t.elapsed() >= tick) {
                engine.poll();
                last_poll = Some(Instant::now());
            }

            // 3. Sleep briefly so command handling stays responsive.
            std::thread::sleep(IDLE_SLEEP);
        }
    }

    /// One full poll: health, markets, selected book, account sections when
    /// a key is configured, public leaderboard, and the admin roster when
    /// admin-capable. Every failure is recorded, none is fatal.
    fn poll(&mut self) {
        self.check_health();
        self.refresh_markets();
        if let Some(token_id) = self.selected_token.clone() {
            self.refresh_book(&token_id);
        }
        if self.config.has_poly_creds() {
            self.refresh_portfolio();
        }
        self.refresh_leaderboard();
        self.refresh_roster();
    }

    fn handle_command(&mut self, command: Command) {
        match command {
            Command::RefreshAll => {
                self.poll();
                self.send_result(ActionKind::RefreshAll, true, "refreshed".to_string());
            }
            Command::SetSearch(q) => {
                self.search = q;
                self.markets_offset = 0;
                let ok = self.refresh_markets();
                self.send_result(ActionKind::SetSearch, ok, format!("search: {:?}", self.search));
            }
            Command::SetMarketsOffset(offset) => {
                self.markets_offset = offset;
                let ok = self.refresh_markets();
                self.send_result(ActionKind::SetMarketsOffset, ok, format!("offset {offset}"));
            }
            Command::SelectMarket { token_id } => {
                self.selected_token = Some(token_id.clone());
                let ok = self.refresh_book(&token_id);
                self.send_result(ActionKind::SelectMarket, ok, token_id);
            }
            Command::PlaceOrder {
                token_id,
                side,
                price,
                size,
            } => {
                let client = PolyClient::new(&self.http, &self.config);
                match client.place_order(&token_id, side, price, size) {
                    // The sim answers HTTP 200 even on failure; surface errorMsg.
                    Ok(res) if res.success => {
                        self.send_result(
                            ActionKind::PlaceOrder,
                            true,
                            format!("order {} {}", res.order_id, res.status),
                        );
                        self.refresh_portfolio();
                        self.refresh_book(&token_id);
                    }
                    Ok(res) => self.send_result(
                        ActionKind::PlaceOrder,
                        false,
                        res.error_msg.unwrap_or_else(|| "order rejected".to_string()),
                    ),
                    Err(e) => self.send_result(ActionKind::PlaceOrder, false, e.to_string()),
                }
            }
            Command::CancelOrder { order_id } => {
                let client = PolyClient::new(&self.http, &self.config);
                match client.cancel_order(&order_id) {
                    Ok(res) => {
                        self.send_result(
                            ActionKind::CancelOrder,
                            res.success,
                            format!("order {} {}", res.order_id, res.status),
                        );
                        self.refresh_portfolio();
                        if let Some(token_id) = self.selected_token.clone() {
                            self.refresh_book(&token_id);
                        }
                    }
                    Err(e) => self.send_result(ActionKind::CancelOrder, false, e.to_string()),
                }
            }
            Command::FetchProfile { username } => {
                let client = LeaderboardClient::new(&self.http, &self.config);
                match client.user_profile(&username) {
                    Ok(profile) => {
                        self.write_ok(|s| {
                            s.profile = Some(LoadedProfile {
                                username: username.clone(),
                                profile,
                            });
                            s.errors.profile = None;
                        });
                        self.send_result(ActionKind::FetchProfile, true, username);
                    }
                    Err(e) => {
                        let detail = format!("{username}: {e}");
                        self.write_err(e, |s, msg| s.errors.profile = Some(msg));
                        self.send_result(ActionKind::FetchProfile, false, detail);
                    }
                }
            }
            Command::FetchAdminUser { username } => {
                let client = PolyClient::new(&self.http, &self.config);
                match client.admin_user(&username) {
                    Ok(profile) => {
                        self.write_ok(|s| {
                            s.profile = Some(LoadedProfile {
                                username: username.clone(),
                                profile,
                            });
                            s.errors.profile = None;
                        });
                        self.send_result(ActionKind::FetchAdminUser, true, username);
                    }
                    Err(e) => {
                        let detail = format!("{username}: {e}");
                        self.write_err(e, |s, msg| s.errors.profile = Some(msg));
                        self.send_result(ActionKind::FetchAdminUser, false, detail);
                    }
                }
            }
            Command::IssueKey {
                username,
                display_name,
                role,
            } => {
                let client = PolyClient::new(&self.http, &self.config);
                match client.create_paper_key(&username, &display_name, role) {
                    Ok(key) => {
                        // The key exists only here and in the admin's hands.
                        self.send(UiMessage::KeyIssued {
                            username: key.username.clone(),
                            role: key.role.clone(),
                            api_key: key.api_key,
                        });
                        self.send_result(
                            ActionKind::IssueKey,
                            true,
                            format!("issued {} key for {username}", key.role),
                        );
                        self.refresh_roster();
                    }
                    Err(e) => self.send_result(ActionKind::IssueKey, false, e.to_string()),
                }
            }
            Command::SetRole { username, role } => {
                let client = PolyClient::new(&self.http, &self.config);
                match client.set_role(&username, role) {
                    Ok(_) => {
                        self.send_result(
                            ActionKind::SetRole,
                            true,
                            format!("{username} is now {}", role.as_wire()),
                        );
                        self.refresh_roster();
                    }
                    Err(e) => self.send_result(ActionKind::SetRole, false, e.to_string()),
                }
            }
            Command::RevokeKey { username } => {
                let client = PolyClient::new(&self.http, &self.config);
                match client.revoke_key(&username) {
                    Ok(res) => {
                        self.send_result(
                            ActionKind::RevokeKey,
                            true,
                            format!("revoked {} ({} keys disabled)", res.revoked, res.keys_disabled),
                        );
                        self.refresh_roster();
                    }
                    Err(e) => self.send_result(ActionKind::RevokeKey, false, e.to_string()),
                }
            }
            Command::ResetBalance { username } => {
                let client = PolyClient::new(&self.http, &self.config);
                match client.reset_balance(username.as_deref()) {
                    Ok(_) => {
                        self.send_result(
                            ActionKind::ResetBalance,
                            true,
                            format!("reset {}", username.as_deref().unwrap_or("ALL members")),
                        );
                        self.refresh_roster();
                    }
                    Err(e) => self.send_result(ActionKind::ResetBalance, false, e.to_string()),
                }
            }
            Command::DeleteUser { username } => {
                let client = PolyClient::new(&self.http, &self.config);
                match client.delete_user(&username) {
                    Ok(_) => {
                        self.send_result(ActionKind::DeleteUser, true, format!("deleted {username}"));
                        self.refresh_roster();
                    }
                    Err(e) => self.send_result(ActionKind::DeleteUser, false, e.to_string()),
                }
            }
            Command::ForceResolve {
                market_id,
                resolution,
            } => {
                let client = PolyClient::new(&self.http, &self.config);
                match client.force_resolve(&market_id, &resolution) {
                    Ok(_) => {
                        self.send_result(
                            ActionKind::ForceResolve,
                            true,
                            format!("resolved {market_id} to {resolution}"),
                        );
                        self.refresh_markets();
                        self.refresh_roster();
                    }
                    Err(e) => self.send_result(ActionKind::ForceResolve, false, e.to_string()),
                }
            }
            Command::RefreshLeaderboard => {
                let ok = self.refresh_leaderboard();
                self.refresh_roster();
                self.send_result(ActionKind::RefreshLeaderboard, ok, "leaderboard".to_string());
            }
        }
    }

    fn check_health(&self) {
        let poly = match PolyClient::new(&self.http, &self.config).health() {
            Ok(h) => ConnState::Connected(h.version),
            Err(e) => ConnState::Error(e.to_string()),
        };
        self.send(UiMessage::Status { poly });
    }

    fn refresh_markets(&self) -> bool {
        let client = PolyClient::new(&self.http, &self.config);
        match client.markets(self.config.market_limit, self.markets_offset, &self.search) {
            Ok(markets) => self.write_ok(|s| {
                s.markets = markets;
                s.errors.markets = None;
            }),
            Err(e) => self.write_err(e, |s, msg| s.errors.markets = Some(msg)),
        }
    }

    fn refresh_book(&self, token_id: &str) -> bool {
        let client = PolyClient::new(&self.http, &self.config);
        match client.book(token_id) {
            Ok(raw) => {
                // Midpoint / spread are best-effort context; the book is the
                // section that decides success.
                let quotes = match (client.midpoint(token_id), client.spread(token_id)) {
                    (Ok(m), Ok(sp)) => Some(SelectedQuotes {
                        midpoint: m.midpoint,
                        spread: sp.spread,
                    }),
                    _ => None,
                };
                self.write_ok(|s| {
                    s.selected_book = Some(OrderBook::from_poly(&raw));
                    s.selected_quotes = quotes;
                    s.errors.book = None;
                })
            }
            Err(e) => self.write_err(e, |s, msg| s.errors.book = Some(msg)),
        }
    }

    /// Portfolio summary, positions and open orders in one section.
    fn refresh_portfolio(&self) -> bool {
        let client = PolyClient::new(&self.http, &self.config);
        let fetch = client
            .portfolio()
            .and_then(|p| Ok((p, client.positions()?, client.user_orders()?)));
        match fetch {
            Ok((portfolio, positions, orders)) => self.write_ok(|s| {
                s.portfolio = Some(portfolio);
                s.positions = positions;
                s.orders = orders;
                s.errors.portfolio = None;
            }),
            Err(e) => self.write_err(e, |s, msg| s.errors.portfolio = Some(msg)),
        }
    }

    /// Public standings from the leaderboard site (cheap, no auth).
    fn refresh_leaderboard(&self) -> bool {
        let client = LeaderboardClient::new(&self.http, &self.config);
        match client.leaderboard() {
            Ok(rows) => self.write_ok(|s| {
                s.leaderboard = rows;
                s.errors.leaderboard = None;
            }),
            Err(e) => self.write_err(e, |s, msg| s.errors.leaderboard = Some(msg)),
        }
    }

    /// Admin roster refresh doubling as the admin-capability probe:
    /// `GET /admin/leaderboard` answers 200 only for admin/owner rank (via
    /// the master secret or the caller's own key role).
    fn refresh_roster(&mut self) -> bool {
        let secret_set = !self.config.poly_admin_secret.is_empty();
        let has_any_cred = secret_set || self.config.has_poly_creds();
        if !has_any_cred {
            self.set_admin_ok(Some(false));
            return false;
        }
        // Skip the call once a member key has been confirmed non-admin; a
        // config change resets this to unknown.
        if !secret_set && self.admin_ok == Some(false) {
            return false;
        }
        let client = PolyClient::new(&self.http, &self.config);
        match client.admin_leaderboard() {
            Ok(rows) => {
                self.set_admin_ok(Some(true));
                self.write_ok(|s| {
                    s.roster = rows;
                    s.errors.roster = None;
                })
            }
            Err(e) if matches!(e.status(), Some(401) | Some(403)) => {
                // Expected for member-role keys: locked, not an error.
                self.set_admin_ok(Some(false));
                self.write_ok(|s| {
                    s.roster.clear();
                    s.errors.roster = None;
                });
                false
            }
            Err(e) => self.write_err(e, |s, msg| s.errors.roster = Some(msg)),
        }
    }

    fn set_admin_ok(&mut self, value: Option<bool>) {
        self.admin_ok = value;
        let mut snap = self.snapshot.lock().unwrap_or_else(|e| e.into_inner());
        snap.admin_ok = value;
    }

    fn send(&self, message: UiMessage) {
        // A closed channel means the UI is gone; the drain loop will exit.
        let _ = self.tx.send(message);
    }

    fn send_result(&self, kind: ActionKind, ok: bool, detail: String) {
        if !ok {
            log::warn!("engine: {kind:?} failed: {detail}");
        }
        self.send(UiMessage::ActionResult { kind, ok, detail });
    }

    /// Apply a successful section update; stamps `last_updated`.
    /// The lock is held only for the closure — never across HTTP calls.
    fn write_ok(&self, apply: impl FnOnce(&mut Snapshot)) -> bool {
        let mut snap = self.snapshot.lock().unwrap_or_else(|e| e.into_inner());
        apply(&mut snap);
        snap.last_updated = Some(Utc::now());
        true
    }

    /// Record a section error. Always returns `false` for tail-position use.
    fn write_err(&self, error: ApiError, apply: impl FnOnce(&mut Snapshot, String)) -> bool {
        let message = error.to_string();
        log::debug!("engine: section refresh failed: {message}");
        let mut snap = self.snapshot.lock().unwrap_or_else(|e| e.into_inner());
        apply(&mut snap, message);
        false
    }
}
