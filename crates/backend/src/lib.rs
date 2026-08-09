//! Backend lane — HTTP API + optional transport fan-out.
//!
//! **Deps:** `dto`, `transport`, `db` (read-only). No cross-lane crate deps.

mod handlers;
mod router;
mod state;

pub use router::router;
pub use state::AppState;

pub fn name() -> &'static str {
    "backend"
}
