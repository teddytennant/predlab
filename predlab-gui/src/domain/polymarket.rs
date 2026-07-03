//! Polymarket-sim adapter (default `https://poly.teddytennant.com`).
//!
//! Account endpoints authenticate with the `POLY_API_KEY` header. Admin
//! endpoints accept either the owner-rank master secret (`X-Admin-Secret`)
//! or an admin/owner-role API key in `POLY_API_KEY`; the adapter sends the
//! master secret when configured and otherwise relies on the user's own key
//! having the admin role. Serde structs mirror the sim's response models.

use serde::{Deserialize, Serialize};

use super::{ApiError, Headers, Health, Http};
use crate::config::Config;
use crate::message::{OrderSide, Role};

/// One market row from `GET /markets` (camelCase on the wire).
/// `clob_token_ids` index 0 is the YES outcome token, index 1 is NO.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PolyMarket {
    pub id: String,
    pub condition_id: String,
    pub question: String,
    pub slug: String,
    pub outcomes: Vec<String>,
    pub outcome_prices: Vec<String>,
    pub clob_token_ids: Option<Vec<String>>,
    pub best_bid: Option<f64>,
    pub best_ask: Option<f64>,
    pub last_trade_price: Option<f64>,
    pub volume: Option<f64>,
    pub liquidity: Option<f64>,
    pub active: bool,
    pub closed: bool,
    pub updated_at: Option<String>,
}

/// One price level from `GET /book` (decimal strings, full precision).
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct PolyBookLevel {
    pub price: String,
    pub size: String,
}

/// `GET /book?token_id=` response.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct PolyBook {
    pub bids: Vec<PolyBookLevel>,
    pub asks: Vec<PolyBookLevel>,
    pub asset_id: Option<String>,
    pub timestamp: Option<String>,
}

/// `GET /midpoint?token_id=` response.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Midpoint {
    pub midpoint: String,
}

/// `GET /spread?token_id=` response.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Spread {
    pub spread: String,
}

/// `GET /last-trade-price?token_id=` response.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct LastTradePrice {
    #[serde(rename = "lastTradePrice")]
    pub last_trade_price: String,
}

/// `POST /order` body. `price: None` (market order) is omitted entirely.
#[derive(Debug, Clone, Serialize)]
pub struct PolyOrderRequest {
    pub token_id: String,
    /// `"BUY"` or `"SELL"`.
    pub side: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price: Option<f64>,
    pub size: f64,
}

/// `POST /order` response. The sim returns HTTP 200 even on failure, so
/// callers must check `success` / `error_msg`.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct PolyOrderResponse {
    pub success: bool,
    #[serde(rename = "orderID")]
    pub order_id: String,
    pub status: String,
    #[serde(rename = "errorMsg")]
    pub error_msg: Option<String>,
}

/// `DELETE /order` response.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct PolyCancelResponse {
    pub success: bool,
    #[serde(rename = "orderID")]
    pub order_id: String,
    pub status: String,
}

/// One row from `GET /positions` (snake_case on the wire).
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct PolyPosition {
    pub market_id: String,
    pub clob_token_id: String,
    pub size: f64,
    pub avg_entry_price: Option<f64>,
    pub current_price: f64,
    pub unrealized_pnl: f64,
    pub market_question: Option<String>,
}

/// `GET /portfolio` response: the paper-accounting summary. `net_worth`
/// is the number the leaderboard ranks by.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct Portfolio {
    pub cash: f64,
    pub positions_value: f64,
    /// Cash escrowed in resting buy orders.
    pub open_orders_value: f64,
    pub net_worth: f64,
}

/// One row from `GET /user/orders`.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct PolyOrder {
    pub id: i64,
    pub market_id: String,
    pub clob_token_id: Option<String>,
    pub side: String,
    pub price: Option<f64>,
    pub size: f64,
    pub filled_size: f64,
    pub status: String,
    pub created_at: String,
}

/// `POST /admin/create-paper-key` response.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct PaperKey {
    pub username: String,
    pub role: String,
    pub api_key: String,
    pub secret: String,
    pub note: String,
}

/// `POST /admin/revoke-key` response.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct RevokedKey {
    pub revoked: String,
    pub keys_disabled: u32,
}

/// One row from `GET /admin/leaderboard` — the server-side club roster,
/// ranked by paper net worth.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct AdminLeaderboardRow {
    pub username: String,
    pub role: String,
    pub cash: f64,
    pub positions_value: f64,
    pub open_orders_value: f64,
    pub net_worth: f64,
}

/// One trade row inside a member profile.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct ProfileTrade {
    pub market_id: String,
    pub side: String,
    pub price: f64,
    pub size: f64,
    pub created_at: String,
}

/// One point of the net-worth-over-time series inside a member profile.
/// The public leaderboard proxy strips `cash`/`positions_value`, so those
/// default to zero there.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct HistoryPoint {
    /// ISO-8601 timestamp (named `t` on the wire).
    pub t: String,
    pub net_worth: f64,
    pub cash: f64,
    pub positions_value: f64,
}

/// A member's full profile: `GET /admin/user/{username}` on the sim, and
/// the same shape (minus `display_name`, with trimmed history points) from
/// the public leaderboard's `GET /api/user/:username`.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct UserProfile {
    pub username: String,
    pub display_name: Option<String>,
    pub role: String,
    pub cash: f64,
    pub positions_value: f64,
    pub open_orders_value: f64,
    pub net_worth: f64,
    pub positions: Vec<PolyPosition>,
    pub trades: Vec<ProfileTrade>,
    pub history: Vec<HistoryPoint>,
}

/// Polymarket-sim client; construct per use from the current [`Config`].
pub struct PolyClient<'a> {
    http: &'a Http,
    base: String,
    api_key: String,
    admin_secret: String,
}

impl<'a> PolyClient<'a> {
    pub fn new(http: &'a Http, config: &Config) -> Self {
        Self {
            http,
            base: config.poly_url.trim_end_matches('/').to_string(),
            api_key: config.poly_api_key.clone(),
            admin_secret: config.poly_admin_secret.clone(),
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base)
    }

    fn auth(&self) -> Headers {
        vec![("POLY_API_KEY".to_string(), self.api_key.clone())]
    }

    /// Admin auth: the master secret when configured (owner rank),
    /// otherwise the member key — the sim honors admin/owner-role keys.
    fn admin(&self) -> Headers {
        if !self.admin_secret.is_empty() {
            vec![("X-Admin-Secret".to_string(), self.admin_secret.clone())]
        } else {
            self.auth()
        }
    }

    /// `GET /health`.
    pub fn health(&self) -> Result<Health, ApiError> {
        self.http.get_json(&self.url("/health"), &[], &vec![])
    }

    /// `GET /markets?active=true&limit=&offset=&q=`.
    pub fn markets(&self, limit: u32, offset: u32, q: &str) -> Result<Vec<PolyMarket>, ApiError> {
        let mut query = vec![
            ("active", "true".to_string()),
            ("limit", limit.to_string()),
            ("offset", offset.to_string()),
        ];
        if !q.is_empty() {
            query.push(("q", q.to_string()));
        }
        self.http.get_json(&self.url("/markets"), &query, &vec![])
    }

    /// `GET /book?token_id=`.
    pub fn book(&self, token_id: &str) -> Result<PolyBook, ApiError> {
        self.http.get_json(
            &self.url("/book"),
            &[("token_id", token_id.to_string())],
            &vec![],
        )
    }

    /// `GET /midpoint?token_id=`.
    pub fn midpoint(&self, token_id: &str) -> Result<Midpoint, ApiError> {
        self.http.get_json(
            &self.url("/midpoint"),
            &[("token_id", token_id.to_string())],
            &vec![],
        )
    }

    /// `GET /spread?token_id=`.
    pub fn spread(&self, token_id: &str) -> Result<Spread, ApiError> {
        self.http.get_json(
            &self.url("/spread"),
            &[("token_id", token_id.to_string())],
            &vec![],
        )
    }

    /// `GET /last-trade-price?token_id=`.
    pub fn last_trade_price(&self, token_id: &str) -> Result<LastTradePrice, ApiError> {
        self.http.get_json(
            &self.url("/last-trade-price"),
            &[("token_id", token_id.to_string())],
            &vec![],
        )
    }

    /// `POST /order` (authed). `price: None` places a market order.
    pub fn place_order(
        &self,
        token_id: &str,
        side: OrderSide,
        price: Option<f64>,
        size: f64,
    ) -> Result<PolyOrderResponse, ApiError> {
        let body = PolyOrderRequest {
            token_id: token_id.to_string(),
            side: side.as_wire().to_string(),
            price,
            size,
        };
        self.http
            .post_json(&self.url("/order"), &[], &self.auth(), &body)
    }

    /// `DELETE /order` (authed) with body `{"orderID": ...}`.
    pub fn cancel_order(&self, order_id: &str) -> Result<PolyCancelResponse, ApiError> {
        let body = serde_json::json!({ "orderID": order_id });
        self.http
            .delete_json(&self.url("/order"), &self.auth(), &body)
    }

    /// `GET /positions` (authed).
    pub fn positions(&self) -> Result<Vec<PolyPosition>, ApiError> {
        self.http
            .get_json(&self.url("/positions"), &[], &self.auth())
    }

    /// `GET /portfolio` (authed): cash / positions / escrow / net worth.
    pub fn portfolio(&self) -> Result<Portfolio, ApiError> {
        self.http
            .get_json(&self.url("/portfolio"), &[], &self.auth())
    }

    /// `GET /user/orders` (authed).
    pub fn user_orders(&self) -> Result<Vec<PolyOrder>, ApiError> {
        self.http
            .get_json(&self.url("/user/orders"), &[], &self.auth())
    }

    /// `POST /admin/create-paper-key?username=&display_name=&role=` (admin;
    /// granting admin/owner needs owner rank).
    pub fn create_paper_key(
        &self,
        username: &str,
        display_name: &str,
        role: Role,
    ) -> Result<PaperKey, ApiError> {
        self.http.post_empty(
            &self.url("/admin/create-paper-key"),
            &[
                ("username", username.to_string()),
                ("display_name", display_name.to_string()),
                ("role", role.as_wire().to_string()),
            ],
            &self.admin(),
        )
    }

    /// `POST /admin/set-role?username=&role=` (owner).
    pub fn set_role(&self, username: &str, role: Role) -> Result<serde_json::Value, ApiError> {
        self.http.post_empty(
            &self.url("/admin/set-role"),
            &[
                ("username", username.to_string()),
                ("role", role.as_wire().to_string()),
            ],
            &self.admin(),
        )
    }

    /// `POST /admin/revoke-key?username=` (admin).
    pub fn revoke_key(&self, username: &str) -> Result<RevokedKey, ApiError> {
        self.http.post_empty(
            &self.url("/admin/revoke-key"),
            &[("username", username.to_string())],
            &self.admin(),
        )
    }

    /// `POST /admin/reset-balance?username=` (admin). `None` resets ALL.
    pub fn reset_balance(&self, username: Option<&str>) -> Result<serde_json::Value, ApiError> {
        let mut query = Vec::new();
        if let Some(u) = username {
            query.push(("username", u.to_string()));
        }
        self.http
            .post_empty(&self.url("/admin/reset-balance"), &query, &self.admin())
    }

    /// `POST /admin/delete-user?username=` (admin).
    pub fn delete_user(&self, username: &str) -> Result<serde_json::Value, ApiError> {
        self.http.post_empty(
            &self.url("/admin/delete-user"),
            &[("username", username.to_string())],
            &self.admin(),
        )
    }

    /// `POST /admin/force-resolve?market_id=&resolution=` (owner).
    pub fn force_resolve(
        &self,
        market_id: &str,
        resolution: &str,
    ) -> Result<serde_json::Value, ApiError> {
        self.http.post_empty(
            &self.url("/admin/force-resolve"),
            &[
                ("market_id", market_id.to_string()),
                ("resolution", resolution.to_string()),
            ],
            &self.admin(),
        )
    }

    /// `GET /admin/leaderboard` (admin) — the server-side roster. Also used
    /// as the admin-capability probe: 200 means the caller has admin rank.
    pub fn admin_leaderboard(&self) -> Result<Vec<AdminLeaderboardRow>, ApiError> {
        self.http
            .get_json(&self.url("/admin/leaderboard"), &[], &self.admin())
    }

    /// `GET /admin/user/{username}` (admin) — full member profile.
    pub fn admin_user(&self, username: &str) -> Result<UserProfile, ApiError> {
        self.http.get_json(
            &self.url(&format!("/admin/user/{username}")),
            &[],
            &self.admin(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn market_decodes_from_camel_case_json() {
        let json = r#"{
            "id": "mkt-1",
            "conditionId": "0xabc",
            "question": "Will it rain?",
            "slug": "will-it-rain",
            "outcomes": ["Yes", "No"],
            "outcomePrices": ["0.62", "0.38"],
            "clobTokenIds": ["tok-yes", "tok-no"],
            "bestBid": 0.61,
            "bestAsk": 0.63,
            "lastTradePrice": 0.62,
            "volume": 1234.5,
            "liquidity": 987.6,
            "active": true,
            "closed": false,
            "updatedAt": "2026-07-01T00:00:00Z",
            "someFutureField": 42
        }"#;
        let m: PolyMarket = serde_json::from_str(json).unwrap();
        assert_eq!(m.condition_id, "0xabc");
        assert_eq!(m.clob_token_ids.as_deref(), Some(&["tok-yes".to_string(), "tok-no".to_string()][..]));
        assert_eq!(m.best_bid, Some(0.61));
        assert!(m.active && !m.closed);
        assert_eq!(m.outcome_prices, vec!["0.62", "0.38"]);
    }

    #[test]
    fn market_tolerates_missing_optionals() {
        let m: PolyMarket = serde_json::from_str(
            r#"{"id":"m","conditionId":"c","question":"q","slug":"s","outcomes":[],"outcomePrices":[],"active":true,"closed":false}"#,
        )
        .unwrap();
        assert_eq!(m.clob_token_ids, None);
        assert_eq!(m.best_bid, None);
        assert_eq!(m.updated_at, None);
    }

    #[test]
    fn book_midpoint_spread_and_last_trade_decode() {
        let b: PolyBook = serde_json::from_str(
            r#"{"bids":[{"price":"0.61","size":"100"}],"asks":[{"price":"0.63","size":"50"}],"asset_id":"tok-yes","timestamp":"1719900000"}"#,
        )
        .unwrap();
        assert_eq!(b.bids[0].price, "0.61");
        assert_eq!(b.asks[0].size, "50");
        assert_eq!(b.asset_id.as_deref(), Some("tok-yes"));

        let m: Midpoint = serde_json::from_str(r#"{"midpoint":"0.62"}"#).unwrap();
        assert_eq!(m.midpoint, "0.62");

        let s: Spread = serde_json::from_str(r#"{"spread":"0.02"}"#).unwrap();
        assert_eq!(s.spread, "0.02");

        let l: LastTradePrice =
            serde_json::from_str(r#"{"lastTradePrice":"0.62"}"#).unwrap();
        assert_eq!(l.last_trade_price, "0.62");
    }

    #[test]
    fn order_body_omits_price_for_market_orders() {
        let market = PolyOrderRequest {
            token_id: "tok".to_string(),
            side: OrderSide::Buy.as_wire().to_string(),
            price: None,
            size: 10.0,
        };
        let json = serde_json::to_value(&market).unwrap();
        assert_eq!(json["token_id"], "tok");
        assert_eq!(json["side"], "BUY");
        assert_eq!(json["size"], 10.0);
        assert!(
            json.get("price").is_none(),
            "market orders must omit price entirely"
        );

        let limit = PolyOrderRequest {
            price: Some(0.55),
            side: OrderSide::Sell.as_wire().to_string(),
            ..market
        };
        let json = serde_json::to_value(&limit).unwrap();
        assert_eq!(json["price"], 0.55);
        assert_eq!(json["side"], "SELL");
    }

    #[test]
    fn order_response_surfaces_error_msg() {
        let r: PolyOrderResponse = serde_json::from_str(
            r#"{"success":false,"orderID":"0","status":"error","errorMsg":"insufficient balance"}"#,
        )
        .unwrap();
        assert!(!r.success);
        assert_eq!(r.error_msg.as_deref(), Some("insufficient balance"));

        let ok: PolyOrderResponse =
            serde_json::from_str(r#"{"success":true,"orderID":"17","status":"open"}"#).unwrap();
        assert!(ok.success);
        assert_eq!(ok.order_id, "17");
        assert_eq!(ok.error_msg, None);
    }

    #[test]
    fn cancel_response_decodes() {
        let r: PolyCancelResponse =
            serde_json::from_str(r#"{"success":true,"orderID":"17","status":"cancelled"}"#).unwrap();
        assert!(r.success);
        assert_eq!(r.status, "cancelled");
    }

    #[test]
    fn positions_and_orders_decode() {
        let p: Vec<PolyPosition> = serde_json::from_str(
            r#"[{"market_id":"m1","clob_token_id":"tok","size":25.0,"avg_entry_price":0.5,"current_price":0.62,"unrealized_pnl":3.0,"market_question":"Will it rain?"}]"#,
        )
        .unwrap();
        assert_eq!(p[0].size, 25.0);
        assert_eq!(p[0].avg_entry_price, Some(0.5));

        let o: Vec<PolyOrder> = serde_json::from_str(
            r#"[{"id":17,"market_id":"m1","clob_token_id":"tok","side":"buy","price":0.55,"size":10.0,"filled_size":0.0,"status":"open","created_at":"2026-07-01T12:00:00"}]"#,
        )
        .unwrap();
        assert_eq!(o[0].id, 17);
        assert_eq!(o[0].status, "open");
    }

    #[test]
    fn portfolio_decodes() {
        let p: Portfolio = serde_json::from_str(
            r#"{"cash":24994.5,"positions_value":5.5,"open_orders_value":100.0,"net_worth":25100.0}"#,
        )
        .unwrap();
        assert_eq!(p.cash, 24994.5);
        assert_eq!(p.positions_value, 5.5);
        assert_eq!(p.open_orders_value, 100.0);
        assert_eq!(p.net_worth, 25100.0);
    }

    #[test]
    fn paper_key_and_health_decode() {
        let k: PaperKey = serde_json::from_str(
            r#"{"username":"alice","role":"member","api_key":"pm_paper_abc","secret":"s","note":"store it"}"#,
        )
        .unwrap();
        assert_eq!(k.api_key, "pm_paper_abc");
        assert_eq!(k.role, "member");

        let h: Health =
            serde_json::from_str(r#"{"status":"ok","version":"0.1.0","environment":"dev"}"#)
                .unwrap();
        assert_eq!(h.version, "0.1.0");
    }

    #[test]
    fn revoked_key_decodes() {
        let r: RevokedKey =
            serde_json::from_str(r#"{"revoked":"alice","keys_disabled":2}"#).unwrap();
        assert_eq!(r.revoked, "alice");
        assert_eq!(r.keys_disabled, 2);
    }

    #[test]
    fn admin_leaderboard_row_decodes() {
        let rows: Vec<AdminLeaderboardRow> = serde_json::from_str(
            r#"[{"username":"alice","role":"member","cash":24900.0,"positions_value":50.0,"open_orders_value":50.0,"net_worth":25000.0},
                {"username":"club_admin","role":"owner","cash":25000.0,"positions_value":0.0,"open_orders_value":0.0,"net_worth":25000.0}]"#,
        )
        .unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].username, "alice");
        assert_eq!(rows[0].role, "member");
        assert_eq!(rows[0].net_worth, 25000.0);
        assert_eq!(rows[1].role, "owner");
    }

    #[test]
    fn admin_user_profile_decodes() {
        let p: UserProfile = serde_json::from_str(
            r#"{
                "username":"alice","display_name":"Alice","role":"member",
                "cash":24900.0,"positions_value":50.0,"open_orders_value":50.0,"net_worth":25000.0,
                "positions":[{"market_id":"m1","clob_token_id":"tok","size":10.0,"avg_entry_price":0.5,"current_price":0.55,"unrealized_pnl":0.5,"market_question":"Q?"}],
                "trades":[{"id":1,"market_id":"m1","clob_token_id":"tok","side":"buy","price":0.5,"size":10.0,"created_at":"2026-07-01T12:00:00"}],
                "history":[{"t":"2026-07-01T12:00:00","net_worth":25000.0,"cash":24900.0,"positions_value":50.0}]
            }"#,
        )
        .unwrap();
        assert_eq!(p.display_name.as_deref(), Some("Alice"));
        assert_eq!(p.positions.len(), 1);
        assert_eq!(p.trades[0].side, "buy");
        assert_eq!(p.history[0].t, "2026-07-01T12:00:00");
        assert_eq!(p.history[0].net_worth, 25000.0);
    }

    #[test]
    fn public_profile_subset_decodes_into_same_struct() {
        // The leaderboard proxy strips display_name and trims history points
        // to {t, net_worth}; defaults must absorb that.
        let p: UserProfile = serde_json::from_str(
            r#"{
                "username":"alice","role":"member",
                "cash":24900.0,"positions_value":50.0,"open_orders_value":50.0,"net_worth":25000.0,
                "positions":[],"trades":[],
                "history":[{"t":"2026-07-01T12:00:00","net_worth":25000.0}]
            }"#,
        )
        .unwrap();
        assert_eq!(p.display_name, None);
        assert_eq!(p.history[0].net_worth, 25000.0);
        assert_eq!(p.history[0].cash, 0.0);
    }

    #[test]
    fn role_wire_values() {
        assert_eq!(Role::Member.as_wire(), "member");
        assert_eq!(Role::Admin.as_wire(), "admin");
        assert_eq!(Role::Owner.as_wire(), "owner");
    }
}
