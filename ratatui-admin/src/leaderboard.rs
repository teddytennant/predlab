//! Deterministic club leaderboard: rank members by total paper balance and
//! format money for display. Pure functions, no I/O — the TUI fetches balances
//! from the simulators and feeds them in.

/// A member's combined standing across both simulators.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Standing {
    pub username: String,
    /// Combined paper balance across Polymarket + Kalshi sims, in cents.
    pub total_cents: i64,
}

/// Rank highest balance first. Ties break alphabetically by username so the
/// ordering is stable and reproducible.
pub fn rank(mut standings: Vec<Standing>) -> Vec<Standing> {
    standings.sort_by(|a, b| {
        b.total_cents
            .cmp(&a.total_cents)
            .then_with(|| a.username.cmp(&b.username))
    });
    standings
}

/// Format a cents amount as a dollar string with thousands separators,
/// e.g. `2_500_000 -> "$25,000.00"`, `-12_345 -> "-$123.45"`.
pub fn format_cents(cents: i64) -> String {
    let sign = if cents < 0 { "-" } else { "" };
    let cents = cents.unsigned_abs();
    let dollars = cents / 100;
    let rem = cents % 100;

    let digits = dollars.to_string();
    let bytes = digits.as_bytes();
    let mut grouped = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 && (bytes.len() - i).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(*b as char);
    }
    format!("{sign}${grouped}.{rem:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(name: &str, total: i64) -> Standing {
        Standing {
            username: name.to_string(),
            total_cents: total,
        }
    }

    #[test]
    fn ranks_highest_balance_first() {
        let out = rank(vec![s("alice", 100), s("bob", 300), s("carol", 200)]);
        let order: Vec<&str> = out.iter().map(|x| x.username.as_str()).collect();
        assert_eq!(order, ["bob", "carol", "alice"]);
    }

    #[test]
    fn ties_break_alphabetically() {
        let out = rank(vec![s("bob", 100), s("alice", 100)]);
        assert_eq!(out[0].username, "alice");
        assert_eq!(out[1].username, "bob");
    }

    #[test]
    fn rank_is_idempotent() {
        let once = rank(vec![s("a", 1), s("b", 2)]);
        let twice = rank(once.clone());
        assert_eq!(once, twice);
    }

    #[test]
    fn formats_dollars_with_thousands() {
        assert_eq!(format_cents(2_500_000), "$25,000.00");
        assert_eq!(format_cents(100), "$1.00");
        assert_eq!(format_cents(5), "$0.05");
        assert_eq!(format_cents(1_234_567), "$12,345.67");
    }

    #[test]
    fn formats_negative_balances() {
        assert_eq!(format_cents(-12_345), "-$123.45");
    }
}
