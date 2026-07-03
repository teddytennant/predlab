//! Public leaderboard-site adapter (default `https://predlab.teddytennant.com`).
//!
//! No authentication: the site serves the club ranking as JSON and proxies
//! per-member profiles from the sim (excluded/staff users 404; a sim outage
//! surfaces as 502).

use serde::Deserialize;

use super::{ApiError, Http};
use crate::config::Config;
use crate::domain::polymarket::UserProfile;

/// One row of `GET /leaderboard.json`.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct LeaderRow {
    pub rank: usize,
    pub username: String,
    pub net_worth: f64,
}

/// Leaderboard-site client; construct per use from the current [`Config`].
pub struct LeaderboardClient<'a> {
    http: &'a Http,
    base: String,
}

impl<'a> LeaderboardClient<'a> {
    pub fn new(http: &'a Http, config: &Config) -> Self {
        Self {
            http,
            base: config.leaderboard_url.trim_end_matches('/').to_string(),
        }
    }

    /// `GET /leaderboard.json` — the public ranking.
    pub fn leaderboard(&self) -> Result<Vec<LeaderRow>, ApiError> {
        self.http
            .get_json(&format!("{}/leaderboard.json", self.base), &[], &vec![])
    }

    /// `GET /api/user/:username` — public member profile (404 for excluded
    /// users, 502 when the sim is unreachable).
    pub fn user_profile(&self, username: &str) -> Result<UserProfile, ApiError> {
        self.http.get_json(
            &format!("{}/api/user/{username}", self.base),
            &[],
            &vec![],
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leaderboard_rows_decode() {
        let rows: Vec<LeaderRow> = serde_json::from_str(
            r#"[{"rank":1,"username":"alice","net_worth":26100.5},
                {"rank":2,"username":"bob","net_worth":24000.0}]"#,
        )
        .unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].rank, 1);
        assert_eq!(rows[0].username, "alice");
        assert_eq!(rows[1].net_worth, 24000.0);
    }

    #[test]
    fn leaderboard_rows_tolerate_missing_rank() {
        let rows: Vec<LeaderRow> =
            serde_json::from_str(r#"[{"username":"alice","net_worth":1.0}]"#).unwrap();
        assert_eq!(rows[0].rank, 0);
    }
}
