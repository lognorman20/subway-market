//! Backend lane — HTTP API + optional transport fan-out.
//!
//! **Deps:** `dto`, `transport`, `db`, `chain`. No cross-lane crate deps.
//!
//! Slice 7 seam: market create/resolve from domain intents lives in
//! [`lifecycle`] (stubs only here).

mod agents;
mod auth;
mod error;
mod funding;
mod handlers;
mod lifecycle;
mod llms;
mod markets;
mod pnl;
mod router;
mod state;
mod trade;
pub mod trading_chain;

pub use auth::AuthStore;
pub use error::ApiError;
pub use funding::{FundingConfig, FundingMode};
pub use router::router;
pub use state::AppState;
pub use trading_chain::{
    try_connect_from_env as trading_chain_connect, MockTradingChain, TradingChain,
};

pub fn name() -> &'static str {
    "backend"
}
