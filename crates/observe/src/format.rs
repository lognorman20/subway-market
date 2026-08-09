//! Small format helpers for the observe TUI.

use chrono::{DateTime, Utc};
use chrono_tz::America::New_York;

/// Micro-USDC scale (6 decimals).
pub const MICRO_USDC: i64 = 1_000_000;

/// WAD fixed-point scale (1e18 = 100% / $1 face).
pub const WAD: u128 = 1_000_000_000_000_000_000;

pub fn format_ny_short(unix: Option<i64>) -> String {
    match unix {
        Some(ts) => DateTime::from_timestamp(ts, 0)
            .map(|dt| dt.with_timezone(&New_York).format("%H:%M").to_string())
            .unwrap_or_else(|| ts.to_string()),
        None => "—".into(),
    }
}

pub fn format_train(route_id: &str, nyct_train_id: Option<&str>) -> String {
    match nyct_train_id.filter(|id| !id.is_empty()) {
        Some(run) => format!("{route_id} {run}"),
        None => route_id.to_string(),
    }
}

pub fn format_diff(diff_seconds: Option<i64>) -> String {
    match diff_seconds {
        Some(s) => {
            let abs = s.abs();
            let sign = if s >= 0 { "+" } else { "-" };
            if abs >= 60 {
                format!("{sign}{}m{}s", abs / 60, abs % 60)
            } else {
                format!("{sign}{abs}s")
            }
        }
        None => "—".into(),
    }
}

pub fn format_age(updated_at: DateTime<Utc>) -> String {
    let secs = (Utc::now() - updated_at).num_seconds();
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else {
        format!("{}h", secs / 3600)
    }
}

/// Shorten long IDs for table columns (keep tail if longer than `max`).
pub fn format_id(id: &str, max: usize) -> String {
    if id.len() <= max {
        id.to_string()
    } else if max <= 1 {
        id[id.len().saturating_sub(max)..].to_string()
    } else {
        format!("…{}", &id[id.len() - (max - 1)..])
    }
}

/// Format micro-USDC integer as human USDC (`±X.XXXXXX`).
pub fn format_usdc_micro(micro: i64) -> String {
    let sign = if micro < 0 { "-" } else { "" };
    let abs = micro.unsigned_abs();
    let whole = abs / (MICRO_USDC as u64);
    let frac = abs % (MICRO_USDC as u64);
    format!("{sign}{whole}.{frac:06}")
}

/// Format share quantity string (micro-USDC) as human USDC.
pub fn format_shares(shares: &str) -> String {
    match shares.parse::<i64>() {
        Ok(n) => format_usdc_micro(n),
        Err(_) => shares.to_string(),
    }
}

/// Format WAD price/mark string as probability decimal (`0.XXXX`, 4 dp).
pub fn format_wad(wad: &str) -> String {
    match wad.parse::<u128>() {
        Ok(n) => format_wad_u128(n),
        Err(_) => "—".into(),
    }
}

pub fn format_wad_u128(wad: u128) -> String {
    // Four decimal places: (wad * 10_000) / 1e18
    let scaled = wad.saturating_mul(10_000) / WAD;
    let whole = scaled / 10_000;
    let frac = scaled % 10_000;
    format!("{whole}.{frac:04}")
}

/// Optional WAD mark → display or em dash.
pub fn format_wad_opt(wad: Option<&str>) -> String {
    match wad {
        Some(s) if !s.is_empty() => format_wad(s),
        _ => "—".into(),
    }
}

/// Clock time in America/New_York for a timestamptz.
pub fn format_ny_clock(dt: DateTime<Utc>) -> String {
    dt.with_timezone(&New_York).format("%H:%M:%S").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_diff_signed() {
        assert_eq!(format_diff(Some(90)), "+1m30s");
        assert_eq!(format_diff(Some(-45)), "-45s");
        assert_eq!(format_diff(None), "—");
    }

    #[test]
    fn format_usdc_micro_human() {
        assert_eq!(format_usdc_micro(500_000), "0.500000");
        assert_eq!(format_usdc_micro(1_000_000), "1.000000");
        assert_eq!(format_usdc_micro(0), "0.000000");
        assert_eq!(format_usdc_micro(-1_250_000), "-1.250000");
        assert_eq!(format_usdc_micro(42), "0.000042");
    }

    #[test]
    fn format_shares_from_string() {
        assert_eq!(format_shares("1000000"), "1.000000");
        assert_eq!(format_shares("not-a-number"), "not-a-number");
    }

    #[test]
    fn format_wad_probability() {
        assert_eq!(format_wad("500000000000000000"), "0.5000");
        assert_eq!(format_wad("1000000000000000000"), "1.0000");
        assert_eq!(format_wad("0"), "0.0000");
        assert_eq!(format_wad("250000000000000000"), "0.2500");
        assert_eq!(format_wad("bogus"), "—");
        assert_eq!(format_wad_opt(None), "—");
        assert_eq!(format_wad_opt(Some("750000000000000000")), "0.7500");
    }

    #[test]
    fn format_id_truncates_tail() {
        assert_eq!(format_id("abc", 8), "abc");
        // 8 chars total: ellipsis + 7-char tail
        assert_eq!(format_id("0123456789abcdef", 8), "…9abcdef");
        assert_eq!(format_id("abcdefghij", 8), "…defghij");
    }
}
