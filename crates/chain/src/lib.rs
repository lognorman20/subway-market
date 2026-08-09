//! Monad / alloy bindings for SubwayMarket LMSR.
//!
//! Bindings compile from vendored ABIs in `crates/chain/abi/` — no forge
//! artifacts needed. After contract changes: `cd apps/contracts && forge build`,
//! then refresh `abi/` (see README).

pub mod bindings;
pub mod client;
pub mod config;
pub mod error;
pub mod types;

pub use client::{
    deploy_local, http_provider_with_signer, mint_mock_usdc, ChainClient, MarketState,
};
pub use config::ChainConfig;
pub use error::{ChainError, Result};
pub use types::{Outcome, Side};

/// Lane name for diagnostics.
pub fn name() -> &'static str {
    "chain"
}

#[cfg(test)]
mod tests {
    #[test]
    fn lane_name() {
        assert_eq!(super::name(), "chain");
    }
}
