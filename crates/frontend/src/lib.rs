//! Frontend lane — HTMX server over the backend HTTP API.
//!
//! **Deps:** `dto`, `transport` only. No cross-lane crate deps.

mod backend;
mod config;
mod markets;
mod routes;
mod session;
mod templates;
mod trains;

pub use backend::AppState;
pub use config::Config;

pub fn name() -> &'static str {
    "frontend"
}

pub fn router(state: AppState) -> axum::Router {
    routes::router(state)
}
