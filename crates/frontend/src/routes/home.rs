use axum::{routing::get, Router};

use crate::backend::AppState;
use crate::templates::{HomeTemplate, TemplateResponse};

pub fn routes() -> Router<AppState> {
    Router::new().route("/", get(home))
}

async fn home() -> TemplateResponse<HomeTemplate> {
    TemplateResponse(HomeTemplate)
}
