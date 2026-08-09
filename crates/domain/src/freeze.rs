//! Freeze on first sight of a stop three ahead of current (`remaining[3]`).
//!
//! `remaining[0]` is the current/start stop and is not counted as "ahead".
//! Shorter lists use the furthest remaining stop ahead of start.
//! Target must never equal the starting stop. Idempotent.
//!
//! Freeze waits until the start/next-stop ETA is near the feed clock so
//! scheduled trips still sitting far ahead of departure are not locked early.

use dto::StopEta;

use crate::diff::dedupe_key;
use crate::types::TripState;

/// Abandon a freeze when feed is this many seconds past `frozen_eta_unix`.
pub const STALE_FREEZE_SECS: i64 = 3 * 60 * 60;

/// Max seconds `remaining[0].eta_unix` may sit ahead of `feed_header_unix`
/// before the trip is treated as not yet live (terminal assign / schedule).
pub const MAX_START_ETA_AHEAD_SECS: i64 = 2 * 60;

/// True when the first remaining stop is due now or within [`MAX_START_ETA_AHEAD_SECS`].
pub fn start_is_live(remaining: &[StopEta], feed_header_unix: i64) -> bool {
    match remaining.first() {
        Some(stop) => stop.eta_unix <= feed_header_unix.saturating_add(MAX_START_ETA_AHEAD_SECS),
        None => false,
    }
}

/// Index of the freeze target in `remaining`.
///
/// Start stop is always `remaining[0]`. Target must differ from start.
///
/// - `len >= 4` → three stops ahead (`remaining[3]`)
/// - `len == 3` → two ahead (`remaining[2]`) — furthest available
/// - `len == 2` → one ahead (`remaining[1]`)
/// - `len <= 1` → cannot freeze (no target ≠ start)
pub fn freeze_target_index(remaining_len: usize) -> Option<usize> {
    match remaining_len {
        0 | 1 => None,
        2 => Some(1),
        3 => Some(2),
        _ => Some(3),
    }
}

/// Clear freeze fields so a new prediction can lock on a later segment.
pub fn clear_freeze(state: &mut TripState) {
    state.frozen_eta_unix = None;
    state.freeze_epoch_unix = None;
    state.destination_stop_id.clear();
    state.last_target_eta_unix = None;
    state.vehicle_at_stop_unix = None;
}

/// Expire a stuck freeze without scoring. Returns `(dedupe_key, frozen_eta_unix, age_secs)`.
pub fn expire_stale_freeze_if_needed(
    state: &mut TripState,
    feed_header_unix: i64,
) -> Option<(String, i64, i64)> {
    let frozen_eta = state.frozen_eta_unix?;
    if feed_header_unix < frozen_eta.saturating_add(STALE_FREEZE_SECS) {
        return None;
    }
    let key = dedupe_key(state);
    let age = feed_header_unix.saturating_sub(frozen_eta);
    clear_freeze(state);
    Some((key, frozen_eta, age))
}

/// Returns true if this call wrote a new freeze.
pub fn maybe_freeze(state: &mut TripState, feed_header_unix: i64) -> bool {
    if state.frozen_eta_unix.is_some() {
        return false;
    }
    if !start_is_live(&state.remaining, feed_header_unix) {
        return false;
    }
    let Some(idx) = freeze_target_index(state.remaining.len()) else {
        return false;
    };
    let start_stop_id = state.remaining.first().map(|s| s.stop_id.as_str());
    let stop = &state.remaining[idx];
    if start_stop_id == Some(stop.stop_id.as_str()) {
        return false;
    }
    // Target ETA must be strictly after the feed lock clock. Otherwise
    // freeze_epoch (Start time) is at/after frozen ETA — empty prediction window.
    // Also covers zombie RT entities with past-due stop times.
    if stop.eta_unix <= feed_header_unix {
        return false;
    }
    state.destination_stop_id = stop.stop_id.clone();
    state.frozen_eta_unix = Some(stop.eta_unix);
    true
}
