//! Ledger lane — subscribe `subway.>` and persist to Postgres (trains only).
//!
//! **Deps:** `dto`, `transport`, `db`. Sole writer to migrations/schema.

pub mod handler;
pub mod projection;
pub mod writer;
