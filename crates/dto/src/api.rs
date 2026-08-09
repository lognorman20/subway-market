use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::events::TrainPhase;

/// Placeholder HTTP API request/response types (backend lane extends these).

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HealthRequest;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
}

impl HealthResponse {
    pub fn ok() -> Self {
        Self {
            status: "ok".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketsListRequest;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketsListResponse {
    pub markets: Vec<String>,
}

impl Default for MarketsListResponse {
    fn default() -> Self {
        Self {
            markets: Vec::new(),
        }
    }
}

/// `GET /trains` query params.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrainsListQuery {
    /// Max rows (default 100, capped at 500).
    pub limit: Option<i64>,
}

/// One row from ledger `tracked_trains` projection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrackedTrain {
    pub dedupe_key: String,
    pub updated_at: DateTime<Utc>,
    pub phase: TrainPhase,
    pub route_id: String,
    pub trip_id: String,
    pub nyct_train_id: Option<String>,
    pub first_seen_at: Option<DateTime<Utc>>,
    pub start_stop_id: Option<String>,
    pub target_stop_id: Option<String>,
    pub frozen_at: Option<DateTime<Utc>>,
    pub expected_arrival_unix: Option<i64>,
    pub remaining_stops: Option<i32>,
    pub arrival_unix: Option<i64>,
    pub diff_seconds: Option<i64>,
    pub actual_source: Option<String>,
    pub action: Option<String>,
    pub side: Option<String>,
    pub reason: Option<String>,
    pub diff_id: Option<String>,
    pub freeze_event_id: Option<String>,
    pub diff_event_id: Option<String>,
}

/// `GET /trains` response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrainsListResponse {
    pub trains: Vec<TrackedTrain>,
}

impl Default for TrainsListResponse {
    fn default() -> Self {
        Self {
            trains: Vec::new(),
        }
    }
}
