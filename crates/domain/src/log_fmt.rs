//! Human-readable INFO log details (stable event name stays on the tracing message).

use dto::{remaining_stops_to_target, ActualSource, DiffEvent};

use crate::types::TripState;

pub(crate) fn direction_word(stop_id: &str) -> Option<&'static str> {
    match stop_id.chars().last()? {
        'N' | 'n' => Some("northbound"),
        'S' | 's' => Some("southbound"),
        _ => None,
    }
}

pub(crate) fn format_unix_rfc3339(unix: i64) -> String {
    chrono::DateTime::from_timestamp(unix, 0)
        .map(|dt| dt.to_rfc3339())
        .unwrap_or_else(|| unix.to_string())
}

fn lateness_label(diff_seconds: i64) -> &'static str {
    if diff_seconds > 0 {
        "late"
    } else if diff_seconds < 0 {
        "early"
    } else {
        "on-time"
    }
}

fn stop_label(stop_id: &str) -> String {
    stop_id.to_string()
}

pub(crate) fn freeze_message(state: &TripState) -> String {
    let frozen = state.frozen_eta_unix.unwrap_or(0);
    let when = format_unix_rfc3339(frozen);
    let remaining = remaining_stops_to_target(&state.remaining, &state.destination_stop_id);
    let stop = stop_label(&state.destination_stop_id);
    match direction_word(&state.destination_stop_id) {
        Some(dir) => format!(
            "froze {dir} {} train {} to arrive at {stop} by {when}; remaining={remaining}",
            state.route_id, state.trip_id,
        ),
        None => format!(
            "froze {} train {} to arrive at {stop} by {when}; remaining={remaining}",
            state.route_id, state.trip_id,
        ),
    }
}

pub(crate) fn arrival_message(diff: &DiffEvent, _actual_source: ActualSource) -> String {
    let predicted = format_unix_rfc3339(diff.frozen_eta_unix);
    let actual = format_unix_rfc3339(diff.actual_unix);
    let timing = lateness_label(diff.diff_seconds);
    let dir_prefix = direction_word(&diff.target_stop_id)
        .map(|d| format!("{d} "))
        .unwrap_or_default();
    let stop = stop_label(&diff.target_stop_id);
    format!(
        "{dir_prefix}{} train {} arrived at {stop}; predicted {predicted}, actual {actual}, diff {}s ({timing})",
        diff.route_id,
        diff.trip_id,
        diff.diff_seconds,
    )
}
