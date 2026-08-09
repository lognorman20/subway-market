//! `FUNDING_MODE` + local seed bankroll.

use std::env;

/// Whole-USDC seed from env → micro-USDC (6 decimals).
pub const USDC_DECIMALS: i64 = 1_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FundingMode {
    Local,
    Testnet,
}

impl FundingMode {
    pub fn from_env() -> Self {
        match env::var("FUNDING_MODE")
            .unwrap_or_else(|_| "local".into())
            .to_ascii_lowercase()
            .as_str()
        {
            "testnet" => Self::Testnet,
            _ => Self::Local,
        }
    }

    pub fn is_local(self) -> bool {
        matches!(self, Self::Local)
    }
}

#[derive(Debug, Clone)]
pub struct FundingConfig {
    pub mode: FundingMode,
    /// Default seed in micro-USDC.
    pub seed_bankroll_usdc: i64,
}

impl FundingConfig {
    pub fn from_env() -> Self {
        let mode = FundingMode::from_env();
        let whole = env::var("SEED_BANKROLL_USDC")
            .ok()
            .and_then(|v| v.parse::<i64>().ok())
            .unwrap_or(1000);
        Self {
            mode,
            seed_bankroll_usdc: whole.saturating_mul(USDC_DECIMALS),
        }
    }

    pub fn local_seed_micro(&self) -> i64 {
        if self.mode.is_local() {
            self.seed_bankroll_usdc
        } else {
            0
        }
    }
}

pub fn whole_usdc_to_micro(whole: i64) -> i64 {
    whole.saturating_mul(USDC_DECIMALS)
}
