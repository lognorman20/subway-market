//! Rain sandbox funding (ACH→USDC) — `FUNDING_MODE=testnet` only.
//!
//! Credits Postgres `agents.bankroll_usdc` on transfer settlement (webhook or poll).
//! Rain→Monad bridge out of scope; agents still trade via backend HTTP.

mod client;
mod handlers;
mod pending;
mod poll;
mod types;

pub use client::{HttpRainClient, RainApi, RainClientError};
pub use handlers::{
    fund_rain, get_funding, poll_funding, rain_webhook, FundRainRequest, FundRainResponse,
    FundingStatusResponse,
};
pub use pending::{FundingStatus, PendingFunding, PendingFundingStore};
pub use poll::{poll_until_settled, SettlePollConfig};
pub use types::{
    major_decimal_to_micro, CreatePaymentRouteRequest, PaymentRoute, RainTransaction,
    SimulatePaymentRouteRequest, SimulatePaymentRouteResponse, TransferStatus,
};

use std::sync::Arc;

use crate::funding::RainConfig;

/// Build HTTP Rain client from config (testnet).
pub fn client_from_config(cfg: &RainConfig) -> Arc<dyn RainApi> {
    Arc::new(HttpRainClient::new(cfg.clone()))
}
