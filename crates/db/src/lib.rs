//! Database schema and migration helpers.
//!
//! **Ownership:** the **ledger** lane is the sole writer for trade/PnL projections
//! (markets/trades/positions/`agent_market_pnl` from bus). Backend may write
//! agents + bankroll (funding/identity). Other lanes read via this crate.

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
