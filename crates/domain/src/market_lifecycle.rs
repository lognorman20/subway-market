//! Market open/resolve intents from freeze + arrival (slice 7).
//!
//! Domain decides; backend executes chain. No on-time outcome — exact arrival = early.

use dto::{
    DiffEvent, EventMeta, FreezeEvent, MarketOpenRequested, MarketOutcome, MarketResolveRequested,
};

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
pub fn freeze_eligible_for_market(freeze: &FreezeEvent) -> bool {
    freeze.remaining_stops >= MIN_STOPS_FOR_MARKET
}

/// Build open intent from a freeze (caller must check [`freeze_eligible_for_market`]).
pub fn open_requested_from_freeze(
    freeze: &FreezeEvent,
    train_id: Option<String>,
) -> MarketOpenRequested {
    MarketOpenRequested {
        meta: EventMeta::new(),
        dedupe_key: freeze.dedupe_key.clone(),
        trip_id: freeze.trip_id.clone(),
        route_id: freeze.route_id.clone(),
        train_id,
        stop_id: freeze.target_stop_id.clone(),
        frozen_eta_unix: freeze.frozen_eta_unix,
        liquidity_b: Some(DEFAULT_LIQUIDITY_B_MICRO_USDC.into()),
    }
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
    use dto::{ActualSource, DiffId, EventId};

    fn sample_freeze(remaining_stops: usize) -> FreezeEvent {
        FreezeEvent {
            event_id: EventId::new(),
            trip_id: "t1".into(),
            route_id: "A".into(),
            from_stop_id: "A01".into(),
            target_stop_id: "A99".into(),
            dedupe_key: "t1:A99:20260725".into(),
            frozen_eta_unix: 1_700_000_100,
            freeze_epoch_unix: 1_700_000_000,
            remaining_stops,
            service_date: Some("20260725".into()),
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
        assert!(!freeze_eligible_for_market(&sample_freeze(2)));
        assert!(freeze_eligible_for_market(&sample_freeze(3)));
        assert!(freeze_eligible_for_market(&sample_freeze(4)));
    }

    #[test]
    fn open_requested_from_eligible_freeze() {
        let freeze = sample_freeze(3);
        let open = open_requested_from_freeze(&freeze, Some("A1234".into()));
        assert_eq!(open.dedupe_key, freeze.dedupe_key);
        assert_eq!(open.stop_id, "A99");
        assert_eq!(open.frozen_eta_unix, freeze.frozen_eta_unix);
        assert_eq!(
            open.liquidity_b.as_deref(),
            Some(DEFAULT_LIQUIDITY_B_MICRO_USDC)
        );
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
