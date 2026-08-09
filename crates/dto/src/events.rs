use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ids::{DiffId, EventId, EventMeta};

/// Ledger projection lifecycle for `tracked_trains.phase`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrainPhase {
    Watching,
    Frozen,
    Arrived,
    Acted,
}

impl TrainPhase {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Watching => "watching",
            Self::Frozen => "frozen",
            Self::Arrived => "arrived",
            Self::Acted => "acted",
        }
    }
}

/// Wire / ledger `bus_events.event_type` discriminator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BusEventType {
    WatchableRoutes,
    TripSnapshot,
    VehicleUpdate,
    Freeze,
    Diff,
}

impl BusEventType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::WatchableRoutes => "watchable_routes",
            Self::TripSnapshot => "trip_snapshot",
            Self::VehicleUpdate => "vehicle_update",
            Self::Freeze => "freeze",
            Self::Diff => "diff",
        }
    }
}

/// How `actual_unix` was obtained (domain DiffEvent).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActualSource {
    /// Last stop_time arrival/departure for target while still present.
    StopTime,
    /// Vehicle reported at target stop.
    VehicleAtStop,
    /// Feed header.timestamp — biased poll/generation proxy, not true arrival.
    HeaderTimestampFallback,
}

/// One remaining stop + ETA from ingest TripUpdate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StopEta {
    pub stop_id: String,
    /// Predicted arrival unix seconds (prefer arrival; else departure).
    pub eta_unix: i64,
}

/// Stops ahead of current (`remaining[0]`) through `target_stop_id`.
///
/// Equals the target's index in `remaining` (so a three-ahead target at
/// `remaining[3]` yields `3`). Returns `0` if target is missing or is the
/// current stop.
pub fn remaining_stops_to_target(remaining: &[StopEta], target_stop_id: &str) -> usize {
    remaining
        .iter()
        .position(|s| s.stop_id == target_stop_id)
        .filter(|&idx| idx > 0)
        .unwrap_or(0)
}

/// Routes currently watchable (alerts filter). Subject: `subway.ingest.watchable`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WatchableRoutes {
    #[serde(flatten)]
    pub meta: EventMeta,
    pub route_ids: Vec<String>,
    pub observed_at: DateTime<Utc>,
}

/// Trip stop-time snapshot. Subject: `subway.ingest.trip`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TripSnapshot {
    #[serde(flatten)]
    pub meta: EventMeta,
    pub trip_id: String,
    pub route_id: String,
    /// GTFS service date `YYYYMMDD` when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service_date: Option<String>,
    /// Ordered future stops including current target horizon.
    pub remaining: Vec<StopEta>,
    /// v1: trip terminus from ingest (informational; domain freeze target is `remaining[3]` when available).
    pub destination_stop_id: String,
    /// Feed header.timestamp (unix) for this message.
    pub feed_header_unix: i64,
    /// NYCT internal train id when present on the GTFS-RT trip descriptor.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nyct_train_id: Option<String>,
}

/// Vehicle position. Subject: `subway.ingest.vehicle`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VehicleUpdate {
    #[serde(flatten)]
    pub meta: EventMeta,
    pub trip_id: String,
    pub route_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_id: Option<String>,
    /// Vehicle timestamp unix when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vehicle_unix: Option<i64>,
    pub feed_header_unix: i64,
}

/// Freeze locked (≤3 stops). Subject: `subway.domain.freeze`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FreezeEvent {
    pub event_id: EventId,
    pub trip_id: String,
    pub route_id: String,
    /// Train position at freeze (`remaining[0]` when locked).
    pub from_stop_id: String,
    pub target_stop_id: String,
    /// Same key as eventual `DiffEvent` for open-freeze joins.
    pub dedupe_key: String,
    pub frozen_eta_unix: i64,
    /// Feed header unix when freeze locked.
    pub freeze_epoch_unix: i64,
    /// Stops ahead of current through target (excludes start); `0` if target not ahead.
    pub remaining_stops: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service_date: Option<String>,
}

/// Prediction vs actual. Subject: `subway.domain.diff`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffEvent {
    pub event_id: EventId,
    pub diff_id: DiffId,
    pub trip_id: String,
    pub route_id: String,
    pub target_stop_id: String,
    /// Dedupe: `trip_id + target_stop_id + service_date` (or freeze epoch fallback).
    pub dedupe_key: String,
    pub frozen_eta_unix: i64,
    pub actual_unix: i64,
    /// `actual_unix - frozen_eta_unix` (seconds).
    pub diff_seconds: i64,
    pub actual_source: ActualSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service_date: Option<String>,
}

/// Wire enum for bus payloads (serde tagged).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event_type", content = "data")]
pub enum BusEvent {
    #[serde(rename = "watchable_routes")]
    Watchable(WatchableRoutes),
    #[serde(rename = "trip_snapshot")]
    Trip(TripSnapshot),
    #[serde(rename = "vehicle_update")]
    Vehicle(VehicleUpdate),
    #[serde(rename = "freeze")]
    Freeze(FreezeEvent),
    #[serde(rename = "diff")]
    Diff(DiffEvent),
}

impl BusEvent {
    pub fn event_id(&self) -> &EventId {
        match self {
            Self::Watchable(e) => &e.meta.event_id,
            Self::Trip(e) => &e.meta.event_id,
            Self::Vehicle(e) => &e.meta.event_id,
            Self::Freeze(e) => &e.event_id,
            Self::Diff(e) => &e.event_id,
        }
    }

    pub fn diff_id(&self) -> Option<&DiffId> {
        match self {
            Self::Diff(e) => Some(&e.diff_id),
            _ => None,
        }
    }

    pub fn event_type_name(&self) -> &'static str {
        self.event_type().as_str()
    }

    pub fn event_type(&self) -> BusEventType {
        match self {
            Self::Watchable(_) => BusEventType::WatchableRoutes,
            Self::Trip(_) => BusEventType::TripSnapshot,
            Self::Vehicle(_) => BusEventType::VehicleUpdate,
            Self::Freeze(_) => BusEventType::Freeze,
            Self::Diff(_) => BusEventType::Diff,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::EventMeta;

    #[test]
    fn trip_snapshot_round_trip() {
        let snap = TripSnapshot {
            meta: EventMeta::new(),
            trip_id: "t1".into(),
            route_id: "A".into(),
            service_date: Some("20260725".into()),
            remaining: vec![StopEta {
                stop_id: "A01".into(),
                eta_unix: 1_700_000_000,
            }],
            destination_stop_id: "A99".into(),
            feed_header_unix: 1_700_000_000,
            nyct_train_id: Some("A1234".into()),
        };
        let json = serde_json::to_string(&snap).unwrap();
        let back: TripSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(back.trip_id, "t1");
        assert_eq!(back.nyct_train_id.as_deref(), Some("A1234"));
        assert!(!back.meta.event_id.0.is_empty());
    }

    #[test]
    fn freeze_event_round_trip() {
        let freeze = FreezeEvent {
            event_id: EventId::new(),
            trip_id: "t1".into(),
            route_id: "A".into(),
            from_stop_id: "A01".into(),
            target_stop_id: "A99".into(),
            dedupe_key: "t1:A99:20260725".into(),
            frozen_eta_unix: 1_700_000_000,
            freeze_epoch_unix: 1_699_999_900,
            remaining_stops: 3,
            service_date: Some("20260725".into()),
        };
        let bus = BusEvent::Freeze(freeze.clone());
        let json = serde_json::to_string(&bus).unwrap();
        let back: BusEvent = serde_json::from_str(&json).unwrap();
        match back {
            BusEvent::Freeze(f) => {
                assert_eq!(f.trip_id, freeze.trip_id);
                assert_eq!(f.dedupe_key, freeze.dedupe_key);
            }
            _ => panic!("expected freeze variant"),
        }
    }

    #[test]
    fn remaining_stops_returns_zero_when_target_missing() {
        let stops = vec![
            StopEta {
                stop_id: "A01".into(),
                eta_unix: 1_700_000_000,
            },
            StopEta {
                stop_id: "A02".into(),
                eta_unix: 1_700_000_100,
            },
        ];
        assert_eq!(remaining_stops_to_target(&stops, "A99"), 0);
    }

    #[test]
    fn remaining_stops_returns_index_when_ahead() {
        let stops = vec![
            StopEta {
                stop_id: "A01".into(),
                eta_unix: 1_700_000_000,
            },
            StopEta {
                stop_id: "A02".into(),
                eta_unix: 1_700_000_100,
            },
            StopEta {
                stop_id: "A03".into(),
                eta_unix: 1_700_000_200,
            },
        ];
        assert_eq!(remaining_stops_to_target(&stops, "A03"), 2);
    }
}
