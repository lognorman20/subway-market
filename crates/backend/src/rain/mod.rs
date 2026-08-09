//! Rain sandbox funding (ACH→USDC on Base/Eth/…) — `FUNDING_MODE=testnet` only.
//!
//! # Funding path (authoritative Monad sheet + Rain sandbox)
//!
//! Source of truth for Monad side: [`docs/raingentic-monad-builder-one-pager.pdf`](../../../../docs/raingentic-monad-builder-one-pager.pdf)
//! — Circle USDC testnet `0x534b2f3A21130d7a60830c2Df862319e593943A3`, faucet.circle.com,
//! testnet-rpc chain 10143. Rain section = agent cards/sandbox (not Monad ACH).
//!
//! Rain OpenAPI crypto rails: ethereum|polygon|optimism|arbitrum|avalanche|base|celo|solana
//! — **no Monad**. ACH→USDC lands on **Base** (default) at `RAIN_DESTINATION_ADDRESS`.
//!
//! **Path choice:** A impossible (no Monad rail). B = Base USDC → Circle CCTP → Monad USDC
//! (real bridge; thin go-time follow-up). **C (shipped):** ACH→Base + Postgres bankroll +
//! interim on-chain backing:
//! - anvil/MockUSDC → mint 1:1 to platform (labeled interim, not a bridge)
//! - Monad testnet Circle USDC → skip mint; top platform via Circle faucet or CCTP 1:1
//!
//! Agents trade only via backend HTTP (`llms.txt`); never call Rain directly.

mod client;
mod handlers;
mod pending;
mod poll;
mod types;

pub use client::{
    ensure_ach_usdc_onramp, route_destination_address, HttpRainClient, RainApi, RainClientError,
};
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
