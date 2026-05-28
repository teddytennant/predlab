//! Thin HTTP client for the PredLab simulator + leaderboard server.
//!
//! Everything the TUI needs is one of:
//!   * **Public** — markets list, public leaderboard JSON. No key required.
//!   * **Authenticated** — your portfolio, positions, trades, open orders.
//!     Sent with the `POLY_API_KEY` header you got from the club admin.
//!
//! All endpoints map 1:1 to the routes the website uses, so the TUI shows the
//! exact same data the leaderboard page shows.

use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use reqwest::Client;
use serde::Deserialize;

/// Default Polymarket simulator base — the club's hosted instance.
pub const DEFAULT_POLY_BASE: &str = "https://poly.teddytennant.com";
/// Default leaderboard server base — serves the public ranking JSON.
pub const DEFAULT_LEADERBOARD_BASE: &str = "https://predlab.teddytennant.com";

#[derive(Clone)]
pub struct Api {
    pub poly_base: String,
    pub leaderboard_base: String,
    pub api_key: Option<String>,
    client: Client,
}

impl Api {
    pub fn from_env() -> Self {
        let env = |k: &str, d: &str| std::env::var(k).unwrap_or_else(|_| d.into());
        let key = std::env::var("POLY_API_KEY")
            .or_else(|_| std::env::var("POLY_KEY"))
            .ok()
            .filter(|s| !s.trim().is_empty());
        let client = Client::builder()
            .timeout(Duration::from_secs(15))
            .user_agent(concat!("predlab-tui/", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("build reqwest client");
        Self {
            poly_base: env("POLY_BASE", DEFAULT_POLY_BASE)
                .trim_end_matches('/')
                .to_string(),
            leaderboard_base: env("LEADERBOARD_BASE", DEFAULT_LEADERBOARD_BASE)
                .trim_end_matches('/')
                .to_string(),
            api_key: key,
            client,
        }
    }

    pub fn has_key(&self) -> bool {
        self.api_key.is_some()
    }

    fn auth(&self, req: reqwest::RequestBuilder) -> Result<reqwest::RequestBuilder> {
        match &self.api_key {
            Some(k) => Ok(req.header("POLY_API_KEY", k)),
            None => Err(anyhow!(
                "no API key — set POLY_API_KEY (your `pm_paper_…` key) to use this view"
            )),
        }
    }

    /// Public ranking from the leaderboard server. No key required.
    pub async fn leaderboard(&self) -> Result<Vec<LeaderRow>> {
        let url = format!("{}/leaderboard.json", self.leaderboard_base);
        let rows: Vec<LeaderRow> = self
            .client
            .get(&url)
            .send()
            .await
            .with_context(|| format!("GET {url}"))?
            .error_for_status()
            .with_context(|| format!("{url} returned error"))?
            .json()
            .await
            .with_context(|| format!("parsing {url}"))?;
        Ok(rows)
    }

    /// Public market list. `limit` caps the result count server-side.
    pub async fn markets(&self, limit: usize) -> Result<Vec<Market>> {
        let url = format!("{}/markets", self.poly_base);
        let raw: serde_json::Value = self
            .client
            .get(&url)
            .query(&[("limit", limit.to_string())])
            .send()
            .await
            .with_context(|| format!("GET {url}"))?
            .error_for_status()
            .with_context(|| format!("{url} returned error"))?
            .json()
            .await
            .with_context(|| format!("parsing {url}"))?;
        let arr = raw.as_array().ok_or_else(|| anyhow!("{url} not a JSON array"))?;
        let mut out = Vec::with_capacity(arr.len());
        for v in arr {
            // serde_json::from_value is tolerant on missing fields thanks to
            // `#[serde(default)]` on Market.
            match serde_json::from_value::<Market>(v.clone()) {
                Ok(m) => out.push(m),
                // Skip unparseable entries instead of failing the whole list —
                // the sim is permissive and may include experimental fields.
                Err(_) => continue,
            }
        }
        Ok(out)
    }

    /// Member portfolio snapshot. Requires an API key.
    pub async fn portfolio(&self) -> Result<Portfolio> {
        let url = format!("{}/portfolio", self.poly_base);
        self.auth(self.client.get(&url))?
            .send()
            .await
            .with_context(|| format!("GET {url}"))?
            .error_for_status()
            .with_context(|| format!("{url} returned error (check POLY_API_KEY)"))?
            .json()
            .await
            .with_context(|| format!("parsing {url}"))
    }

    /// Member positions with current marks + unrealized P&L. Requires a key.
    pub async fn positions(&self) -> Result<Vec<Position>> {
        let url = format!("{}/positions", self.poly_base);
        self.auth(self.client.get(&url))?
            .send()
            .await
            .with_context(|| format!("GET {url}"))?
            .error_for_status()
            .with_context(|| format!("{url} returned error"))?
            .json()
            .await
            .with_context(|| format!("parsing {url}"))
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct LeaderRow {
    pub username: String,
    pub net_worth: f64,
    /// Optional pre-computed rank from the server. Currently unused (the TUI
    /// derives rank from position) but parsed so the field is documented.
    #[serde(default)]
    #[allow(dead_code)]
    pub rank: Option<usize>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Market {
    /// The market id — accepted as either string or number for compatibility
    /// with both the sim and real Polymarket payloads. Kept for future use
    /// (e.g., opening an order-book overlay).
    #[serde(default)]
    #[allow(dead_code)]
    pub id: serde_json::Value,
    #[serde(default)]
    pub question: String,
    #[serde(default, rename = "bestBid")]
    pub best_bid: Option<f64>,
    #[serde(default, rename = "bestAsk")]
    pub best_ask: Option<f64>,
    /// Outcome tokens [YES, NO]. Kept around so a future order-entry view can
    /// reach them without re-fetching.
    #[serde(default, rename = "clobTokenIds")]
    #[allow(dead_code)]
    pub clob_token_ids: Vec<String>,
    #[serde(default)]
    pub volume: Option<f64>,
    #[serde(default)]
    pub category: Option<String>,
}

impl Market {
    /// "Mid" price (¢) of the YES outcome, used as the at-a-glance market estimate.
    pub fn mid(&self) -> Option<f64> {
        match (self.best_bid, self.best_ask) {
            (Some(b), Some(a)) => Some((b + a) / 2.0),
            (Some(b), None) => Some(b),
            (None, Some(a)) => Some(a),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Portfolio {
    #[serde(default)]
    pub cash: f64,
    #[serde(default)]
    pub positions_value: f64,
    #[serde(default)]
    pub open_orders_value: f64,
    #[serde(default)]
    pub net_worth: f64,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Position {
    #[serde(default)]
    pub market_id: String,
    #[serde(default)]
    pub market_question: Option<String>,
    #[serde(default)]
    pub size: f64,
    #[serde(default)]
    pub avg_entry_price: Option<f64>,
    #[serde(default)]
    pub current_price: f64,
    #[serde(default)]
    pub unrealized_pnl: f64,
}
