use axum::{
    extract::State,
    routing::get,
    Router,
};

use crate::backend::AppState;
use crate::templates::{TrainsPageTemplate, TrainsRowsTemplate, TemplateResponse};
use crate::trains::TrainsPresentation;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/trains", get(trains_page))
        .route("/trains/rows", get(trains_rows))
}

async fn trains_page(State(state): State<AppState>) -> TemplateResponse<TrainsPageTemplate> {
    let presentation = TrainsPresentation::from_fetch(state.fetch_trains().await);
    TemplateResponse(TrainsPageTemplate::from(presentation))
}

async fn trains_rows(State(state): State<AppState>) -> TemplateResponse<TrainsRowsTemplate> {
    let presentation = TrainsPresentation::from_fetch(state.fetch_trains().await);
    TemplateResponse(TrainsRowsTemplate::from(presentation))
}
