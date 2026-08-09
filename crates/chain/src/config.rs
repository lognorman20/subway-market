//! Env config for Monad RPC + platform signer + deployed addresses.
//!
//! Locked env names (decision #8): `MONAD_RPC_URL`, `SUBWAY_MARKET_ADDRESS`,
//! `USDC_ADDRESS`, `PLATFORM_SIGNER_KEY`, `PLATFORM_SIGNER_ADDRESS`.

use std::env;
use std::str::FromStr;

use alloy::primitives::Address;
use alloy::signers::local::PrivateKeySigner;

use crate::error::{ChainError, Result};

/// On-chain connection + one platform signer (decision #3).
#[derive(Clone, Debug)]
pub struct ChainConfig {
    pub rpc_url: String,
    pub subway_market: Address,
    pub usdc: Address,
    /// Hex private key (with or without `0x`).
    pub signer_key: String,
    pub signer_address: Address,
}

impl ChainConfig {
    /// Load from process env. Fails if any required var missing/invalid.
    pub fn from_env() -> Result<Self> {
        let rpc_url = require_env("MONAD_RPC_URL")?;
        let subway_market = parse_address("SUBWAY_MARKET_ADDRESS")?;
        let usdc = parse_address("USDC_ADDRESS")?;
        let signer_key = require_env("PLATFORM_SIGNER_KEY")?;
        let signer_address = parse_address("PLATFORM_SIGNER_ADDRESS")?;

        let cfg = Self {
            rpc_url,
            subway_market,
            usdc,
            signer_key,
            signer_address,
        };
        cfg.validate_signer()?;
        Ok(cfg)
    }

    /// Build a local signer from `PLATFORM_SIGNER_KEY`.
    pub fn signer(&self) -> Result<PrivateKeySigner> {
        PrivateKeySigner::from_str(self.signer_key.trim())
            .map_err(|e| ChainError::InvalidSignerKey(e.to_string()))
    }

    /// Ensure configured address matches key (one platform signer).
    pub fn validate_signer(&self) -> Result<()> {
        let derived = self.signer()?.address();
        if derived != self.signer_address {
            return Err(ChainError::SignerAddressMismatch {
                configured: self.signer_address,
                derived,
            });
        }
        Ok(())
    }
}

fn require_env(key: &'static str) -> Result<String> {
    env::var(key).map_err(|_| ChainError::MissingEnv(key))
}

fn parse_address(key: &'static str) -> Result<Address> {
    let raw = require_env(key)?;
    Address::from_str(raw.trim()).map_err(|source| ChainError::InvalidAddress { name: key, source })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Anvil account #0 (matches `.env.example` defaults).
    const ANVIL0_KEY: &str = "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80";
    const ANVIL0_ADDR: &str = "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266";

    #[test]
    fn validate_signer_ok_for_anvil0() {
        let cfg = ChainConfig {
            rpc_url: "http://127.0.0.1:8545".into(),
            subway_market: Address::ZERO,
            usdc: Address::ZERO,
            signer_key: ANVIL0_KEY.into(),
            signer_address: Address::from_str(ANVIL0_ADDR).unwrap(),
        };
        cfg.validate_signer().unwrap();
    }

    #[test]
    fn validate_signer_rejects_mismatch() {
        let cfg = ChainConfig {
            rpc_url: "http://127.0.0.1:8545".into(),
            subway_market: Address::ZERO,
            usdc: Address::ZERO,
            signer_key: ANVIL0_KEY.into(),
            signer_address: Address::ZERO,
        };
        assert!(matches!(
            cfg.validate_signer(),
            Err(ChainError::SignerAddressMismatch { .. })
        ));
    }

    #[test]
    fn from_env_reads_required_vars() {
        // Isolate: set then clear within this test only.
        env::set_var("MONAD_RPC_URL", "http://127.0.0.1:8545");
        env::set_var("SUBWAY_MARKET_ADDRESS", ANVIL0_ADDR);
        env::set_var("USDC_ADDRESS", ANVIL0_ADDR);
        env::set_var("PLATFORM_SIGNER_KEY", ANVIL0_KEY);
        env::set_var("PLATFORM_SIGNER_ADDRESS", ANVIL0_ADDR);

        let cfg = ChainConfig::from_env().unwrap();
        assert_eq!(cfg.rpc_url, "http://127.0.0.1:8545");
        assert_eq!(cfg.signer_address, Address::from_str(ANVIL0_ADDR).unwrap());

        for k in [
            "MONAD_RPC_URL",
            "SUBWAY_MARKET_ADDRESS",
            "USDC_ADDRESS",
            "PLATFORM_SIGNER_KEY",
            "PLATFORM_SIGNER_ADDRESS",
        ] {
            env::remove_var(k);
        }
    }
}
