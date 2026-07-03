//! The shared [`Snapshot`] the engine writes and the UI reads, plus the
//! normalized order-book display model.
//!
//! The engine holds the `Mutex` only long enough to overwrite one section;
//! the UI clones what it needs each frame.

use chrono::{DateTime, Utc};

use crate::domain::leaderboard::LeaderRow;
use crate::domain::polymarket::{
    AdminLeaderboardRow, PolyBook, PolyMarket, PolyOrder, PolyPosition, Portfolio, UserProfile,
};

/// One normalized price level. `price`/`size` are parsed for math and
/// sorting; the `_raw` strings preserve the sim's exact decimal text for
/// display.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BookLevel {
    pub price: f64,
    pub size: f64,
    pub price_raw: String,
    pub size_raw: String,
}

/// A `/book` response normalized for display: bids sorted best (highest)
/// first, asks sorted best (lowest) first.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct OrderBook {
    pub bids: Vec<BookLevel>,
    pub asks: Vec<BookLevel>,
}

impl OrderBook {
    /// Normalize a `/book` response.
    pub fn from_poly(book: &PolyBook) -> Self {
        let mut bids: Vec<BookLevel> = book.bids.iter().map(|l| level(&l.price, &l.size)).collect();
        let mut asks: Vec<BookLevel> = book.asks.iter().map(|l| level(&l.price, &l.size)).collect();
        bids.sort_by(|a, b| b.price.total_cmp(&a.price));
        asks.sort_by(|a, b| a.price.total_cmp(&b.price));
        Self { bids, asks }
    }
}

fn parse(s: &str) -> f64 {
    s.trim().parse().unwrap_or(0.0)
}

fn level(price: &str, size: &str) -> BookLevel {
    BookLevel {
        price: parse(price),
        size: parse(size),
        price_raw: price.to_string(),
        size_raw: size.to_string(),
    }
}

/// Market context (midpoint / spread) for the selected token.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SelectedQuotes {
    pub midpoint: String,
    pub spread: String,
}

/// A profile loaded for the Leaderboard or Admin detail pane, tagged with
/// where it came from so stale panes don't show the wrong user.
#[derive(Debug, Clone, PartialEq)]
pub struct LoadedProfile {
    pub username: String,
    pub profile: UserProfile,
}

/// Last error per snapshot section (`None` = the last refresh succeeded).
#[derive(Debug, Clone, Default)]
pub struct SectionErrors {
    pub markets: Option<String>,
    pub book: Option<String>,
    pub portfolio: Option<String>,
    pub leaderboard: Option<String>,
    pub roster: Option<String>,
    pub profile: Option<String>,
}

/// Everything the UI needs to draw one frame. The engine overwrites
/// individual sections as polls and commands complete.
#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    pub markets: Vec<PolyMarket>,
    /// Book for the currently selected outcome token, if any.
    pub selected_book: Option<OrderBook>,
    /// Midpoint / spread for the currently selected token.
    pub selected_quotes: Option<SelectedQuotes>,
    /// `/portfolio` summary (cash, positions, escrow, net worth).
    pub portfolio: Option<Portfolio>,
    pub positions: Vec<PolyPosition>,
    pub orders: Vec<PolyOrder>,
    /// Public standings from the leaderboard site.
    pub leaderboard: Vec<LeaderRow>,
    /// Server-side club roster (`GET /admin/leaderboard`); admin only.
    pub roster: Vec<AdminLeaderboardRow>,
    /// Whether the current credentials have admin rank. `None` = unknown
    /// (not yet probed).
    pub admin_ok: Option<bool>,
    /// Profile loaded for a detail pane (leaderboard row click or admin
    /// "view" action).
    pub profile: Option<LoadedProfile>,
    /// When any section last refreshed successfully.
    pub last_updated: Option<DateTime<Utc>>,
    pub errors: SectionErrors,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::polymarket::PolyBookLevel;

    fn poly_book() -> PolyBook {
        PolyBook {
            bids: vec![
                PolyBookLevel { price: "0.5900".into(), size: "5".into() },
                PolyBookLevel { price: "0.6100".into(), size: "100".into() },
            ],
            asks: vec![
                PolyBookLevel { price: "0.6500".into(), size: "20".into() },
                PolyBookLevel { price: "0.6300".into(), size: "50".into() },
            ],
            asset_id: Some("tok".into()),
            timestamp: None,
        }
    }

    #[test]
    fn poly_book_normalizes_and_sorts() {
        let book = OrderBook::from_poly(&poly_book());
        assert_eq!(book.bids[0].price, 0.61, "best bid first");
        assert_eq!(book.bids[0].price_raw, "0.6100", "raw string preserved");
        assert_eq!(book.asks[0].price, 0.63, "best ask first");
        assert_eq!(book.asks[1].size, 20.0);
    }

    #[test]
    fn unparsable_price_becomes_zero_but_keeps_raw() {
        let mut pb = poly_book();
        pb.bids[0].price = "garbage".into();
        let book = OrderBook::from_poly(&pb);
        let bad = book.bids.iter().find(|l| l.price_raw == "garbage").unwrap();
        assert_eq!(bad.price, 0.0);
    }
}
