//! Domain library — watchable filter, freeze ≤3, DiffEvent + market lifecycle intents.
//!
//! **Deps:** `dto`, `transport`, `db` (read-only). No cross-lane crate deps.

pub mod diff;
pub mod engine;
pub mod freeze;
pub mod log_fmt;
pub mod handler;
pub mod market_lifecycle;
pub mod replay;
pub mod state;
pub mod types;
