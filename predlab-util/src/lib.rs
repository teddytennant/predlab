//! Small formatting helpers shared by the PredLab Rust front-ends
//! (`ratatui-admin` and `predlab-tui`). Kept dependency-free so any tool can
//! pull it in cheaply.

/// Format a USD amount with thousands separators: `25000.0` → `"$25,000.00"`,
/// `-42.1` → `"-$42.10"`.
pub fn fmt_money(v: f64) -> String {
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

/// Truncate `s` to at most `max` characters, appending `…` when shortened.
/// Operates on `char` boundaries so it never splits a multi-byte UTF-8 scalar.
pub fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut t: String = s.chars().take(max.saturating_sub(1)).collect();
        t.push('…');
        t
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn money_groups_thousands() {
        assert_eq!(fmt_money(25_000.0), "$25,000.00");
        assert_eq!(fmt_money(1_234_567.5), "$1,234,567.50");
        assert_eq!(fmt_money(0.0), "$0.00");
        assert_eq!(fmt_money(-42.10), "-$42.10");
    }

    #[test]
    fn truncate_respects_char_boundaries() {
        assert_eq!(truncate("hello", 10), "hello");
        assert_eq!(truncate("hello world", 5), "hell…");
        // Multi-byte scalars must not be split mid-byte.
        assert_eq!(truncate("héllo wörld", 5), "héll…");
        assert_eq!(truncate("日本語テスト", 3), "日本…");
    }
}
