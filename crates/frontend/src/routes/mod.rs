mod auth;
mod home;
mod markets;
mod trains;

use axum::Router;
use tower_http::trace::TraceLayer;

use crate::backend::AppState;

pub fn router(state: AppState) -> Router {
    Router::new()
        .merge(home::routes())
        .merge(trains::routes())
        .merge(markets::routes())
        .merge(auth::routes())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
