mod home;
mod trains;

use axum::Router;
use tower_http::trace::TraceLayer;

use crate::backend::AppState;

pub fn router(state: AppState) -> Router {
    Router::new()
        .merge(home::routes())
        .merge(trains::routes())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
