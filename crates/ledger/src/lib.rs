//! Ledger lane — subscribe `subway.>` and persist to Postgres.
//!
//! Projects trains plus trade tape / positions / `agent_market_pnl` from bus facts only
//! (locked #10). Sole DB writer.
//!
//! **Deps:** `dto`, `transport`, `db`.

pub mod handler;
pub mod pnl;
pub mod projection;
pub mod trade_projection;
pub mod writer;
