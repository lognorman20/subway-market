//! Market open/resolve intents from freeze + arrival (slice 7).
//!
//! Domain decides; backend executes chain. No on-time outcome — exact arrival = early.
//!
//! **One market at a time:** among eligible frozen trips, pick one at random when the
//! active slot is empty. Do not open every freeze.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use dto::{
    remaining_stops_to_target, DiffEvent, EventMeta, MarketOpenRequested, MarketOutcome,
    MarketResolveRequested,
};

use crate::diff::dedupe_key;
use crate::state::TripStore;
use crate::types::TripState;

/// Minimum stops ahead of current (`remaining[0]`) required to open a market.
pub const MIN_STOPS_FOR_MARKET: usize = 3;

/// Default LMSR `b` in micro-USDC when domain omits `liquidity_b` (100 USDC).
pub const DEFAULT_LIQUIDITY_B_MICRO_USDC: &str = "100000000";

/// Resolution rule: arrival ≤ frozen_eta → early; strictly after → late.
pub fn outcome_from_arrival(actual_arrival_unix: i64, frozen_eta_unix: i64) -> MarketOutcome {
    if actual_arrival_unix <= frozen_eta_unix {
        MarketOutcome::Early
    } else {
        MarketOutcome::Late
    }
}

/// True when freeze target is far enough for a market (≥3 stops away).
pub fn trip_eligible_for_market(state: &TripState) -> bool {
    if state.frozen_eta_unix.is_none() || state.destination_stop_id.is_empty() {
        return false;
    }
    remaining_stops_to_target(&state.remaining, &state.destination_stop_id) >= MIN_STOPS_FOR_MARKET
}

/// Build open intent from a frozen trip (caller must check eligibility + slot).
pub fn open_requested_from_trip(state: &TripState) -> Option<MarketOpenRequested> {
    let frozen_eta_unix = state.frozen_eta_unix?;
    if !trip_eligible_for_market(state) {
        return None;
    }
    let start_stop_id = state
        .start_stop_id
        .clone()
        .or_else(|| state.remaining.first().map(|s| s.stop_id.clone()))
        .unwrap_or_default();
    let start_eta_unix = state
        .start_eta_unix
        .or(state.freeze_epoch_unix)
        .unwrap_or(0);
    Some(MarketOpenRequested {
        meta: EventMeta::new(),
        dedupe_key: dedupe_key(state),
        trip_id: state.trip_id.clone(),
        route_id: state.route_id.clone(),
        train_id: state.train_id.clone(),
        start_stop_id,
        start_eta_unix,
        stop_id: state.destination_stop_id.clone(),
        frozen_eta_unix,
        liquidity_b: Some(DEFAULT_LIQUIDITY_B_MICRO_USDC.into()),
    })
}

/// If no active market, randomly pick one eligible frozen trip and reserve the slot.
pub fn try_select_and_open_market(store: &mut TripStore, salt: u64) -> Option<MarketOpenRequested> {
    if store.active_market_dedupe.is_some() {
        return None;
    }

    let mut candidates: Vec<MarketOpenRequested> = Vec::new();
    for state in store.trips() {
        let key = dedupe_key(state);
        if store.is_done(&key) || store.market_opened_dedupes.contains(&key) {
            continue;
        }
        if let Some(req) = open_requested_from_trip(state) {
            candidates.push(req);
        }
    }
    if candidates.is_empty() {
        return None;
    }

    let idx = {
        let mut hasher = DefaultHasher::new();
        salt.hash(&mut hasher);
        candidates.len().hash(&mut hasher);
        for c in &candidates {
            c.dedupe_key.hash(&mut hasher);
        }
        (hasher.finish() as usize) % candidates.len()
    };
    let req = candidates.swap_remove(idx);
    store.note_market_opened(req.dedupe_key.clone());
    Some(req)
}

/// Build resolve intent from a scored arrival diff.
pub fn resolve_requested_from_diff(diff: &DiffEvent) -> MarketResolveRequested {
    let outcome = outcome_from_arrival(diff.actual_unix, diff.frozen_eta_unix);
    MarketResolveRequested {
        meta: EventMeta::new(),
        dedupe_key: diff.dedupe_key.clone(),
        actual_arrival_unix: diff.actual_unix,
        outcome,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dto::{ActualSource, DiffId, EventId, StopEta};

    fn frozen_trip(trip: &str, remaining_stops: usize) -> TripState {
        let feed = 1_700_000_000_i64;
        let mut remaining = vec![StopEta {
            stop_id: "A01".into(),
            eta_unix: feed,
        }];
        for i in 0..remaining_stops {
            remaining.push(StopEta {
                stop_id: format!("A{:02}", i + 2),
                eta_unix: feed + 60 * (i as i64 + 1),
            });
        }
        let target = remaining.last().unwrap().stop_id.clone();
        let target_eta = remaining.last().unwrap().eta_unix;
        TripState {
            trip_id: trip.into(),
            route_id: "A".into(),
            service_date: Some("20260725".into()),
            train_id: Some("A1234".into()),
            destination_stop_id: target,
            remaining,
            frozen_eta_unix: Some(target_eta),
            freeze_epoch_unix: Some(feed),
            start_stop_id: Some("A01".into()),
            start_eta_unix: Some(feed),
            ..Default::default()
        }
    }

    fn sample_diff(actual: i64, frozen: i64) -> DiffEvent {
        DiffEvent {
            event_id: EventId::new(),
            diff_id: DiffId::new(),
            trip_id: "t1".into(),
            route_id: "A".into(),
            target_stop_id: "A99".into(),
            dedupe_key: "t1:A99:20260725".into(),
            frozen_eta_unix: frozen,
            actual_unix: actual,
            diff_seconds: actual - frozen,
            actual_source: ActualSource::StopTime,
            service_date: Some("20260725".into()),
        }
    }

    #[test]
    fn exact_on_time_is_early() {
        assert_eq!(
            outcome_from_arrival(1_700_000_100, 1_700_000_100),
            MarketOutcome::Early
        );
    }

    #[test]
    fn early_arrival_is_early() {
        assert_eq!(
            outcome_from_arrival(1_700_000_050, 1_700_000_100),
            MarketOutcome::Early
        );
    }

    #[test]
    fn late_arrival_is_late() {
        assert_eq!(
            outcome_from_arrival(1_700_000_101, 1_700_000_100),
            MarketOutcome::Late
        );
    }

    #[test]
    fn market_requires_three_stops() {
        assert!(!trip_eligible_for_market(&frozen_trip("t1", 2)));
        assert!(trip_eligible_for_market(&frozen_trip("t1", 3)));
        assert!(trip_eligible_for_market(&frozen_trip("t1", 4)));
    }

    #[test]
    fn open_requested_includes_start_fields() {
        let state = frozen_trip("t1", 3);
        let open = open_requested_from_trip(&state).expect("open");
        assert_eq!(open.start_stop_id, "A01");
        assert_eq!(open.start_eta_unix, 1_700_000_000);
        assert_eq!(open.stop_id, state.destination_stop_id);
        assert_eq!(
            open.liquidity_b.as_deref(),
            Some(DEFAULT_LIQUIDITY_B_MICRO_USDC)
        );
    }

    #[test]
    fn select_opens_exactly_one_among_many() {
        let mut store = TripStore::new();
        store.upsert(frozen_trip("t1", 3));
        store.upsert(frozen_trip("t2", 3));
        store.upsert(frozen_trip("t3", 3));
        let first = try_select_and_open_market(&mut store, 42).expect("pick");
        assert!(store.active_market_dedupe.is_some());
        assert!(try_select_and_open_market(&mut store, 99).is_none());
        assert_eq!(store.market_opened_dedupes.len(), 1);
        assert!(store.market_opened_dedupes.contains(&first.dedupe_key));
    }

    #[test]
    fn resolve_requested_boundary_exact_early() {
        let diff = sample_diff(100, 100);
        let req = resolve_requested_from_diff(&diff);
        assert_eq!(req.outcome, MarketOutcome::Early);
        assert_eq!(req.actual_arrival_unix, 100);
        assert_eq!(req.dedupe_key, diff.dedupe_key);
    }

    #[test]
    fn resolve_requested_late() {
        let diff = sample_diff(101, 100);
        let req = resolve_requested_from_diff(&diff);
        assert_eq!(req.outcome, MarketOutcome::Late);
    }
}
