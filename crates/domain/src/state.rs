//! In-memory trip map.
//!
//! Hot path only: live remaining + active freeze until DiffEvent or abandon.

use std::collections::{HashMap, HashSet};

use dto::StopEta;

use crate::types::TripState;

#[derive(Debug, Clone)]
struct LastArrived {
    stop_id: String,
    route_id: String,
}

#[derive(Debug, Default)]
pub struct TripStore {
    by_trip: HashMap<String, TripState>,
    /// Completed prediction dedupe keys (process-local; preload from ledger on restart).
    pub done: HashSet<String>,
    /// Routes currently watchable (from latest `WatchableRoutes` event).
    pub watchable_routes: HashSet<String>,
    /// Last scored target per trip. Survives trip removal so the next freeze
    /// waits until feed `remaining[0]` catches up (or drops that stop).
    last_arrived: HashMap<String, LastArrived>,
    /// Dedupe key of the single in-flight market (open requested, not yet resolved).
    pub active_market_dedupe: Option<String>,
    /// Dedupe keys for which we emitted `market.open_requested` (resolve only these).
    pub market_opened_dedupes: HashSet<String>,
}

impl TripStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, trip_id: &str) -> Option<&TripState> {
        self.by_trip.get(trip_id)
    }

    pub fn get_mut(&mut self, trip_id: &str) -> Option<&mut TripState> {
        self.by_trip.get_mut(trip_id)
    }

    pub fn upsert(&mut self, state: TripState) {
        self.by_trip.insert(state.trip_id.clone(), state);
    }

    pub fn remove(&mut self, trip_id: &str) -> Option<TripState> {
        self.by_trip.remove(trip_id)
    }

    pub fn mark_done(&mut self, dedupe_key: impl Into<String>) {
        self.done.insert(dedupe_key.into());
    }

    pub fn is_done(&self, dedupe_key: &str) -> bool {
        self.done.contains(dedupe_key)
    }

    /// Record that `target_stop_id` just scored for `trip_id`.
    pub fn note_arrived(
        &mut self,
        trip_id: impl Into<String>,
        route_id: impl Into<String>,
        target_stop_id: impl Into<String>,
    ) {
        self.last_arrived.insert(
            trip_id.into(),
            LastArrived {
                stop_id: target_stop_id.into(),
                route_id: route_id.into(),
            },
        );
    }

    /// True when it is safe to open a new freeze after a prior arrival.
    ///
    /// If the last arrived stop still appears in `remaining` but not at index 0,
    /// the GTFS feed has not advanced yet — defer freeze to avoid anchoring on a
    /// stale prior stop (e.g. Avenue N while Bay Pkwy already arrived).
    pub fn arrival_feed_ready(&self, trip_id: &str, remaining: &[StopEta]) -> bool {
        let Some(prev) = self.last_arrived.get(trip_id) else {
            return true;
        };
        match remaining.iter().position(|s| s.stop_id == prev.stop_id) {
            None => true,
            Some(0) => true,
            Some(_) => false,
        }
    }

    pub fn clear_last_arrived(&mut self, trip_id: &str) {
        self.last_arrived.remove(trip_id);
    }

    pub fn is_watchable(&self, route_id: &str) -> bool {
        self.watchable_routes.is_empty() || self.watchable_routes.contains(route_id)
    }

    pub fn set_watchable_routes(&mut self, route_ids: impl IntoIterator<Item = String>) {
        self.watchable_routes = route_ids.into_iter().collect();
        let routes = self.watchable_routes.clone();
        self.by_trip
            .retain(|_, state| routes.contains(&state.route_id));
        self.last_arrived
            .retain(|_, arrived| routes.contains(&arrived.route_id));
    }

    pub fn trip_ids(&self) -> impl Iterator<Item = &String> {
        self.by_trip.keys()
    }

    pub fn trips(&self) -> impl Iterator<Item = &TripState> {
        self.by_trip.values()
    }

    pub fn len(&self) -> usize {
        self.by_trip.len()
    }

    pub fn note_market_opened(&mut self, dedupe_key: impl Into<String>) {
        let key = dedupe_key.into();
        self.active_market_dedupe = Some(key.clone());
        self.market_opened_dedupes.insert(key);
    }

    /// Clear active slot when the opened market arrives / expires. Returns true if it was active.
    pub fn clear_market_slot(&mut self, dedupe_key: &str) -> bool {
        self.market_opened_dedupes.remove(dedupe_key);
        let was_active = self.active_market_dedupe.as_deref() == Some(dedupe_key);
        if was_active {
            self.active_market_dedupe = None;
        }
        was_active
    }
}
