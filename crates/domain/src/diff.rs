//! Arrival scoring → DiffEvent.

use dto::{ActualSource, DiffEvent, DiffId, EventId};

use crate::types::TripState;

/// Build dedupe key: trip + target + service_date (or freeze epoch fallback).
pub fn dedupe_key(state: &TripState) -> String {
    let date = state
        .service_date
        .clone()
        .or_else(|| state.freeze_epoch_unix.map(|t| t.to_string()))
        .unwrap_or_else(|| "unknown".into());
    format!("{}:{}:{}", state.trip_id, state.destination_stop_id, date)
}

/// If target absent and freeze present → DiffEvent. Else None.
pub fn score_arrival(
    state: &TripState,
    actual_unix: i64,
    actual_source: ActualSource,
) -> Option<DiffEvent> {
    let frozen = state.frozen_eta_unix?;
    let target = &state.destination_stop_id;
    if state.remaining.iter().any(|s| s.stop_id == *target) {
        return None;
    }
    Some(DiffEvent {
        event_id: EventId::new(),
        diff_id: DiffId::new(),
        trip_id: state.trip_id.clone(),
        route_id: state.route_id.clone(),
        target_stop_id: target.clone(),
        dedupe_key: dedupe_key(state),
        frozen_eta_unix: frozen,
        actual_unix,
        diff_seconds: actual_unix - frozen,
        actual_source,
        service_date: state.service_date.clone(),
    })
}
