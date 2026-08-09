//! Backend lane — HTTP API + optional transport fan-out.
//!
//! **Deps:** `dto`, `transport`, `db`, `chain`. No cross-lane crate deps.
//!
//! Market create/resolve from domain lifecycle intents: [`lifecycle`].

mod agents;
mod auth;
mod error;
mod funding;
mod handlers;
pub mod lifecycle;
mod llms;
mod markets;
mod pnl;
pub mod rain;
mod router;
mod state;
mod trade;
pub mod trading_chain;

pub use auth::AuthStore;
pub use error::ApiError;
pub use funding::{
    FundingConfig, FundingMode, RainConfig, MONAD_TESTNET_CHAIN_ID, MONAD_TESTNET_USDC,
    RAIN_SANDBOX_COMPLIANT_DEST_EXAMPLE,
};
pub use lifecycle::{LifecycleHandler, LIFECYCLE_SUBJECTS};
pub use rain::{
    client_from_config, major_decimal_to_micro, HttpRainClient, PendingFundingStore, RainApi,
    RainClientError, SettlePollConfig,
};
pub use router::router;
pub use state::AppState;
pub use trading_chain::{
    try_connect_from_env as trading_chain_connect, MockTradingChain, TradingChain,
    TreasuryMirrorResult,
};

pub fn name() -> &'static str {
    "backend"
}
