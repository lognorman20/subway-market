//! HTTP route handlers.

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use db::{list_tracked_trains, QueryError, TrackedTrainRow};
use dto::{HealthResponse, TrackedTrain, TrainPhase, TrainsListQuery, TrainsListResponse};
use tracing::error;

use crate::state::AppState;

const DEFAULT_TRAINS_LIMIT: i64 = 200;
const MAX_TRAINS_LIMIT: i64 = 500;

pub async fn health() -> Json<HealthResponse> {
    Json(HealthResponse::ok())
}

pub async fn list_trains(
    State(state): State<AppState>,
    Query(query): Query<TrainsListQuery>,
) -> Result<Json<TrainsListResponse>, ApiError> {
    let limit = query
        .limit
        .unwrap_or(DEFAULT_TRAINS_LIMIT)
        .clamp(1, MAX_TRAINS_LIMIT);

    let mut conn = state
        .pool
        .get()
        .await
        .map_err(|err| ApiError::Internal(format!("pool get: {err}")))?;

    let rows = list_tracked_trains(&mut conn, limit).await?;
    let trains = rows.into_iter().map(tracked_train_row_to_api).collect();

    Ok(Json(TrainsListResponse { trains }))
}

fn tracked_train_row_to_api(row: TrackedTrainRow) -> TrackedTrain {
    TrackedTrain {
        dedupe_key: row.dedupe_key,
        updated_at: row.updated_at,
        phase: phase_from_db(&row.phase),
        route_id: row.route_id,
        trip_id: row.trip_id,
        nyct_train_id: row.nyct_train_id,
        first_seen_at: row.first_seen_at,
        start_stop_id: row.start_stop_id,
        target_stop_id: row.target_stop_id,
        frozen_at: row.frozen_at,
        expected_arrival_unix: row.expected_arrival_unix,
        remaining_stops: row.remaining_stops,
        arrival_unix: row.arrival_unix,
        diff_seconds: row.diff_seconds,
        actual_source: row.actual_source,
        action: row.action,
        side: row.side,
        reason: row.reason,
        diff_id: row.diff_id,
        freeze_event_id: row.freeze_event_id,
        diff_event_id: row.diff_event_id,
    }
}

fn phase_from_db(phase: &str) -> TrainPhase {
    match phase {
        "frozen" => TrainPhase::Frozen,
        "arrived" => TrainPhase::Arrived,
        "acted" => TrainPhase::Acted,
        _ => TrainPhase::Watching,
    }
}

#[derive(Debug)]
pub enum ApiError {
    Internal(String),
}

impl From<QueryError> for ApiError {
    fn from(err: QueryError) -> Self {
        Self::Internal(err.to_string())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            Self::Internal(message) => {
                error!(error = %message, "api error");
                (StatusCode::INTERNAL_SERVER_ERROR, message)
            }
        };
        (status, message).into_response()
    }
}
