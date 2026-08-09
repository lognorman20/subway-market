//! Engine: apply ingest events → freeze / score.
//!
//! `emit=false` during ledger rebuild; `emit=true` live.

use dto::{
    remaining_stops_to_target, ActualSource, BusEvent, DiffEvent, FreezeEvent, TripSnapshot,
    VehicleUpdate, WatchableRoutes, EventId,
};
use tracing::{debug, info};

use crate::diff::{dedupe_key, score_arrival};
use crate::freeze::{clear_freeze, expire_stale_freeze_if_needed, maybe_freeze};
use crate::log_fmt::{arrival_message, freeze_message};
use crate::state::TripStore;
use crate::types::TripState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmitMode {
    /// Rebuild / replay — never publish bus events.
    Silent,
    /// Live / buffer drain — may publish.
    Live,
}

/// Domain output from one `apply` call (zero or more per event).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DomainEmit {
    Freeze(FreezeEvent),
    Diff(DiffEvent),
}

impl DomainEmit {
    pub fn into_diff(self) -> Option<DiffEvent> {
        match self {
            Self::Diff(d) => Some(d),
            _ => None,
        }
    }
}

/// Apply one bus event. Returns emit payloads when `mode == Live`.
pub fn apply(store: &mut TripStore, event: &BusEvent, mode: EmitMode) -> Vec<DomainEmit> {
    match event {
        BusEvent::Watchable(w) => {
            on_watchable(store, w);
            vec![]
        }
        BusEvent::Trip(snap) => on_trip_snapshot(store, snap, mode),
        BusEvent::Vehicle(veh) => on_vehicle(store, veh, mode),
        BusEvent::Freeze(_) | BusEvent::Diff(_) => {
            vec![]
        }
    }
}

fn build_freeze_event(state: &TripState) -> FreezeEvent {
    let from_stop_id = state
        .remaining
        .first()
        .map(|s| s.stop_id.clone())
        .unwrap_or_default();
    FreezeEvent {
        event_id: EventId::new(),
        trip_id: state.trip_id.clone(),
        route_id: state.route_id.clone(),
        from_stop_id,
        target_stop_id: state.destination_stop_id.clone(),
        dedupe_key: dedupe_key(state),
        frozen_eta_unix: state.frozen_eta_unix.unwrap_or(0),
        freeze_epoch_unix: state.freeze_epoch_unix.unwrap_or(0),
        remaining_stops: remaining_stops_to_target(&state.remaining, &state.destination_stop_id),
        service_date: state.service_date.clone(),
    }
}

fn on_watchable(store: &mut TripStore, w: &WatchableRoutes) {
    store.set_watchable_routes(w.route_ids.clone());
    debug!(routes = w.route_ids.len(), "watchable routes updated");
}

fn on_trip_snapshot(
    store: &mut TripStore,
    snap: &TripSnapshot,
    mode: EmitMode,
) -> Vec<DomainEmit> {
    if !store.is_watchable(&snap.route_id) {
        store.remove(&snap.trip_id);
        store.clear_last_arrived(&snap.trip_id);
        return vec![];
    }

    if snap.remaining.is_empty() {
        store.clear_last_arrived(&snap.trip_id);
        return finish_trip(store, &snap.trip_id, snap.feed_header_unix, mode);
    }

    let mut state = store
        .get(&snap.trip_id)
        .cloned()
        .unwrap_or_else(|| TripState {
            trip_id: snap.trip_id.clone(),
            route_id: snap.route_id.clone(),
            service_date: snap.service_date.clone(),
            ..Default::default()
        });

    state.route_id = snap.route_id.clone();
    if snap.service_date.is_some() {
        state.service_date = snap.service_date.clone();
    }
    state.remaining = snap.remaining.clone();

    if let Some((key, frozen_eta, age)) =
        expire_stale_freeze_if_needed(&mut state, snap.feed_header_unix)
    {
        info!(
            trip_id = %state.trip_id,
            dedupe_key = %key,
            frozen_eta_unix = frozen_eta,
            age_secs = age,
            feed_header_unix = snap.feed_header_unix,
            "stale freeze expired"
        );
        store.mark_done(&key);
        store.upsert(state);
        return vec![];
    }

    if store.is_done(&dedupe_key(&state)) {
        return vec![];
    }

    let feed_ready = store.arrival_feed_ready(&snap.trip_id, &state.remaining);
    let freeze_emit = if feed_ready && maybe_freeze(&mut state, snap.feed_header_unix) {
        let key = dedupe_key(&state);
        // Expire marks done then clear_freeze wipes destination; the early
        // is_done check above then misses. Re-check after the new lock.
        if store.is_done(&key) {
            debug!(
                trip_id = %state.trip_id,
                dedupe_key = %key,
                "freeze skipped (dedupe already done)"
            );
            clear_freeze(&mut state);
            None
        } else {
            store.clear_last_arrived(&snap.trip_id);
            state.freeze_epoch_unix = Some(snap.feed_header_unix);
            let remaining =
                remaining_stops_to_target(&state.remaining, &state.destination_stop_id);
            info!(
                event_id = %snap.meta.event_id.as_ref(),
                trip_id = %state.trip_id,
                route_id = %state.route_id,
                target_stop_id = %state.destination_stop_id,
                frozen_eta_unix = state.frozen_eta_unix.unwrap_or(0),
                remaining,
                detail = %freeze_message(&state),
                "freeze"
            );
            if mode == EmitMode::Live {
                Some(DomainEmit::Freeze(build_freeze_event(&state)))
            } else {
                None
            }
        }
    } else {
        if !feed_ready {
            debug!(
                trip_id = %snap.trip_id,
                "freeze deferred (feed remaining still behind last arrival)"
            );
        }
        None
    };

    if !state.destination_stop_id.is_empty() {
        if let Some(stop) = state
            .remaining
            .iter()
            .find(|s| s.stop_id == state.destination_stop_id)
        {
            state.last_target_eta_unix = Some(stop.eta_unix);
        }
    }

    // Arrival only after a freeze target exists. Unfrozen trips (e.g. start ETA
    // still far ahead) stay in RAM until live enough to freeze.
    let frozen = state.frozen_eta_unix.is_some();
    let target_present = state
        .remaining
        .iter()
        .any(|s| s.stop_id == state.destination_stop_id);
    if frozen && !target_present {
        let (actual_unix, source) = resolve_actual_unix(&state, snap.feed_header_unix);
        return finish_with_optional_freeze(store, state, actual_unix, source, mode, freeze_emit);
    }

    store.upsert(state);
    freeze_emit.into_iter().collect()
}

fn on_vehicle(
    store: &mut TripStore,
    veh: &VehicleUpdate,
    mode: EmitMode,
) -> Vec<DomainEmit> {
    if !store.is_watchable(&veh.route_id) {
        return vec![];
    }
    let trip_id = veh.trip_id.clone();
    let at_target = store
        .get(&trip_id)
        .map(|s| veh.stop_id.as_deref() == Some(s.destination_stop_id.as_str()))
        .unwrap_or(false);
    if !at_target {
        return vec![];
    }

    let Some(state) = store.get(&trip_id) else {
        return vec![];
    };
    if state.frozen_eta_unix.is_none() {
        let ts = veh.vehicle_unix.or(Some(veh.feed_header_unix));
        store.get_mut(&trip_id).unwrap().vehicle_at_stop_unix = ts;
        return vec![];
    }

    let mut state = state.clone();
    if let Some((key, frozen_eta, age)) =
        expire_stale_freeze_if_needed(&mut state, veh.feed_header_unix)
    {
        info!(
            trip_id = %state.trip_id,
            dedupe_key = %key,
            frozen_eta_unix = frozen_eta,
            age_secs = age,
            feed_header_unix = veh.feed_header_unix,
            "stale freeze expired"
        );
        store.mark_done(&key);
        store.upsert(state);
        return vec![];
    }

    if store.is_done(&dedupe_key(&state)) {
        return vec![];
    }

    let mut state = store.remove(&trip_id).expect("trip present");
    state.vehicle_at_stop_unix = veh.vehicle_unix.or(Some(veh.feed_header_unix));
    state
        .remaining
        .retain(|s| s.stop_id != state.destination_stop_id);
    let (actual_unix, source) = resolve_actual_unix(&state, veh.feed_header_unix);
    finish_trip_with_state(store, state, actual_unix, source, mode)
}

fn finish_trip(
    store: &mut TripStore,
    trip_id: &str,
    feed_header_unix: i64,
    mode: EmitMode,
) -> Vec<DomainEmit> {
    let Some(mut state) = store.remove(trip_id) else {
        return vec![];
    };
    if store.is_done(&dedupe_key(&state)) {
        return vec![];
    }
    state.remaining.retain(|s| s.stop_id != state.destination_stop_id);
    let (actual_unix, source) = resolve_actual_unix(&state, feed_header_unix);
    finish_trip_with_state(store, state, actual_unix, source, mode)
}

fn finish_with_optional_freeze(
    store: &mut TripStore,
    state: TripState,
    actual_unix: i64,
    actual_source: ActualSource,
    mode: EmitMode,
    freeze_emit: Option<DomainEmit>,
) -> Vec<DomainEmit> {
    let mut out: Vec<DomainEmit> = freeze_emit.into_iter().collect();
    if let Some(diff) = score_and_finish(store, state, actual_unix, actual_source, mode) {
        out.push(diff);
    }
    out
}

fn finish_trip_with_state(
    store: &mut TripStore,
    state: TripState,
    actual_unix: i64,
    actual_source: ActualSource,
    mode: EmitMode,
) -> Vec<DomainEmit> {
    score_and_finish(store, state, actual_unix, actual_source, mode)
        .into_iter()
        .collect()
}

fn resolve_actual_unix(state: &TripState, feed_header_unix: i64) -> (i64, ActualSource) {
    if let Some(ts) = state.last_target_eta_unix {
        return (ts, ActualSource::StopTime);
    }
    if let Some(ts) = state.vehicle_at_stop_unix {
        return (ts, ActualSource::VehicleAtStop);
    }
    (
        feed_header_unix,
        ActualSource::HeaderTimestampFallback,
    )
}

fn score_and_finish(
    store: &mut TripStore,
    state: TripState,
    actual_unix: i64,
    actual_source: ActualSource,
    mode: EmitMode,
) -> Option<DomainEmit> {
    let trip_id = state.trip_id.clone();
    let key = dedupe_key(&state);

    if store.is_done(&key) {
        store.remove(&trip_id);
        debug!(trip_id = %trip_id, dedupe_key = %key, "arrival skipped (done)");
        return None;
    }

    let Some(diff) = score_arrival(&state, actual_unix, actual_source) else {
        store.remove(&trip_id);
        return None;
    };

    store.note_arrived(&trip_id, &diff.route_id, &diff.target_stop_id);
    store.remove(&trip_id);

    if mode == EmitMode::Silent {
        debug!(
            trip_id = %trip_id,
            diff_id = %diff.diff_id.as_ref(),
            "diff suppressed (emit=false rebuild)"
        );
        return None;
    }

    info!(
        event_id = %diff.event_id.as_ref(),
        diff_id = %diff.diff_id.as_ref(),
        trip_id = %diff.trip_id,
        route_id = %diff.route_id,
        target_stop_id = %diff.target_stop_id,
        dedupe_key = %diff.dedupe_key,
        diff_seconds = diff.diff_seconds,
        actual_source = ?actual_source,
        detail = %arrival_message(&diff, actual_source),
        "diff"
    );
    Some(DomainEmit::Diff(diff))
}
