//! HTTP route handlers (health, trains, auth).

use axum::{extract::State, Json};
use db::{list_tracked_trains, TrackedTrainRow};
use dto::{
    AuthChallengeRequest, AuthChallengeResponse, AuthVerifyRequest, AuthVerifyResponse,
    HealthResponse, TrackedTrain, TrainPhase, TrainsListQuery, TrainsListResponse,
};
use axum::extract::Query;

use crate::error::ApiError;
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

pub async fn auth_challenge(
    State(state): State<AppState>,
    Json(req): Json<AuthChallengeRequest>,
) -> Result<Json<AuthChallengeResponse>, ApiError> {
    let (challenge_id, message, expires_at) =
        state.auth.issue_challenge(&req.address).await?;
    Ok(Json(AuthChallengeResponse {
        challenge_id,
        message,
        expires_at,
    }))
}

pub async fn auth_verify(
    State(state): State<AppState>,
    Json(req): Json<AuthVerifyRequest>,
) -> Result<Json<AuthVerifyResponse>, ApiError> {
    let (session_token, wallet, expires_at) = state
        .auth
        .verify_challenge(&req.challenge_id, &req.address, &req.signature)
        .await?;
    Ok(Json(AuthVerifyResponse {
        session_token,
        wallet,
        expires_at,
    }))
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
