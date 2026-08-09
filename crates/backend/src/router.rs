//! Axum router for the backend HTTP API.

use axum::{
    routing::{get, post},
    Router,
};
use tower_http::trace::TraceLayer;

use crate::agents;
use crate::handlers;
use crate::llms;
use crate::markets;
use crate::pnl;
use crate::state::AppState;
use crate::trade;

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(handlers::health))
        .route("/llms.txt", get(llms::llms_txt))
        .route("/trains", get(handlers::list_trains))
        .route("/auth/challenge", post(handlers::auth_challenge))
        .route("/auth/verify", post(handlers::auth_verify))
        .route("/agents", post(agents::create_agent).get(agents::list_agents))
        .route(
            "/agents/{id}",
            get(agents::get_agent).delete(agents::delete_agent),
        )
        .route("/agents/{id}/bankroll", get(agents::get_bankroll))
        .route("/bankroll/seed", post(agents::seed_bankroll))
        .route("/markets", get(markets::list_markets))
        .route("/markets/{id}", get(markets::get_market))
        .route("/markets/{id}/trades", get(markets::list_market_trades))
        .route("/buy", post(trade::buy))
        .route("/sell", post(trade::sell))
        .route("/pnl", get(pnl::list_pnl))
        .route("/pnl/{agent_id}/{market_id}", get(pnl::get_agent_market_pnl))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
