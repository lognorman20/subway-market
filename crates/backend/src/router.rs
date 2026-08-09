//! Axum router for the backend HTTP API.

use axum::{routing::get, Router};
use tower_http::trace::TraceLayer;

use crate::handlers;
use crate::state::AppState;

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(handlers::health))
        .route("/trains", get(handlers::list_trains))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
