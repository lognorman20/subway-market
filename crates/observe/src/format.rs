//! Small format helpers for the observe TUI.

use chrono::{DateTime, Utc};
use chrono_tz::America::New_York;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_diff_signed() {
        assert_eq!(format_diff(Some(90)), "+1m30s");
        assert_eq!(format_diff(Some(-45)), "-45s");
        assert_eq!(format_diff(None), "—");
    }
}
