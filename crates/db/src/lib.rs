//! Database schema and migration helpers.
//!
//! **Ownership:** the **ledger** lane is the sole writer to Postgres.
//! Other lanes may read via this crate's types/queries — never write.

pub mod migrate;
pub mod models;
pub mod queries;
pub mod schema;

pub use migrate::run_migrations;
pub use models::{
    AgentMarketPnlRow, AgentMarketPnlUpsert, AgentRow, MarketRow, NewAgentRow, NewMarketRow,
    NewTradeRow, PositionRow, PositionUpsert, TradeRow,
};
pub use queries::{
    fetch_ingest_events, ledger_has_dedupe_key, list_tracked_trains, preload_done_keys,
    QueryError, TrackedTrainRow,
};
