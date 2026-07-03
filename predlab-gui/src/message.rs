//! Typed messages flowing between the UI thread and the engine thread.
//!
//! The UI sends [`EngineMessage`]s (config pushes, commands, shutdown) and
//! receives [`UiMessage`]s (connection status, action results, one-time key
//! material). Bulk display data travels through the shared
//! [`crate::data::Snapshot`] instead.

use crate::config::Config;

/// Order side, serialized as `"BUY"` / `"SELL"` on the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderSide {
    Buy,
    Sell,
}

impl OrderSide {
    /// Wire representation for the sim's `POST /order`.
    pub fn as_wire(self) -> &'static str {
        match self {
            OrderSide::Buy => "BUY",
            OrderSide::Sell => "SELL",
        }
    }
}

/// Account role in the sim's member/admin/owner hierarchy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Role {
    #[default]
    Member,
    Admin,
    Owner,
}

impl Role {
    /// Wire representation for the admin endpoints' `role` query parameter.
    pub fn as_wire(self) -> &'static str {
        match self {
            Role::Member => "member",
            Role::Admin => "admin",
            Role::Owner => "owner",
        }
    }

    /// Human label for pickers.
    pub fn label(self) -> &'static str {
        match self {
            Role::Member => "member",
            Role::Admin => "admin",
            Role::Owner => "owner",
        }
    }
}

/// UI -> engine.
#[derive(Debug, Clone)]
pub enum EngineMessage {
    /// Full config push; the engine adopts it wholesale.
    ConfigChanged(Config),
    /// A user action to execute immediately.
    Command(Command),
    /// Stop the engine loop and exit the thread.
    Shutdown,
}

/// A user action the engine executes with HTTP calls.
#[derive(Debug, Clone)]
pub enum Command {
    /// Re-poll everything now instead of waiting for the tick.
    RefreshAll,
    /// Set the `q` filter for `/markets` and refresh the list.
    SetSearch(String),
    /// Set the `/markets` pagination offset and refresh the list.
    SetMarketsOffset(u32),
    /// Select an outcome token and fetch its book / midpoint / spread.
    SelectMarket { token_id: String },
    /// Place an order; `price: None` means a market order.
    PlaceOrder {
        token_id: String,
        side: OrderSide,
        price: Option<f64>,
        size: f64,
    },
    /// Cancel an order by id.
    CancelOrder { order_id: String },
    /// Fetch a member profile from the public leaderboard server.
    FetchProfile { username: String },
    /// Admin: fetch a member's full detail from the sim (incl. history).
    FetchAdminUser { username: String },
    /// Admin: mint a paper key with a role (admin/owner grants need owner rank).
    IssueKey {
        username: String,
        display_name: String,
        role: Role,
    },
    /// Owner: change an existing user's role.
    SetRole { username: String, role: Role },
    /// Admin: deactivate all of a user's API keys.
    RevokeKey { username: String },
    /// Admin: reset a paper balance (`None` = reset EVERYONE).
    ResetBalance { username: Option<String> },
    /// Admin: permanently delete a user and their data.
    DeleteUser { username: String },
    /// Owner: force-resolve a market to `yes` or `no`.
    ForceResolve { market_id: String, resolution: String },
    /// Re-fetch the public leaderboard (and admin roster if capable) now.
    RefreshLeaderboard,
}

/// Health of the simulator connection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnState {
    /// Not yet checked.
    Unknown,
    /// Healthy; carries the reported server version.
    Connected(String),
    /// Last check failed; carries the error text.
    Error(String),
}

/// Which [`Command`] an [`UiMessage::ActionResult`] refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionKind {
    RefreshAll,
    SetSearch,
    SetMarketsOffset,
    SelectMarket,
    PlaceOrder,
    CancelOrder,
    FetchProfile,
    FetchAdminUser,
    IssueKey,
    SetRole,
    RevokeKey,
    ResetBalance,
    DeleteUser,
    ForceResolve,
    RefreshLeaderboard,
}

/// Engine -> UI.
#[derive(Debug, Clone)]
pub enum UiMessage {
    /// Connection status of the sim, refreshed every tick.
    Status { poly: ConnState },
    /// Outcome of a [`Command`], for toasts.
    ActionResult {
        kind: ActionKind,
        ok: bool,
        detail: String,
    },
    /// Admin key issuance result; the key is shown once and never stored.
    KeyIssued {
        username: String,
        role: String,
        api_key: String,
    },
}
