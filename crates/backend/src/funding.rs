//! `FUNDING_MODE` + local seed bankroll + optional Rain testnet config.

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

/// Rain sandbox credentials — loaded only when `FUNDING_MODE=testnet`.
#[derive(Debug, Clone)]
pub struct RainConfig {
    pub api_base_url: String,
    pub api_key: String,
    pub user_id: String,
    pub team_id: Option<String>,
    /// On-chain destination for ACH→USDC onramp (sandbox: Base etc.).
    pub destination_address: String,
    /// Crypto rail for onramp destination (default `base`).
    pub destination_rail: String,
}

impl RainConfig {
    /// `None` when mode is local or required `RAIN_*` missing.
    pub fn from_env_if_testnet(mode: FundingMode) -> Option<Self> {
        if mode != FundingMode::Testnet {
            return None;
        }
        let api_key = env::var("RAIN_API_KEY").ok().filter(|s| !s.is_empty())?;
        let user_id = env::var("RAIN_USER_ID").ok().filter(|s| !s.is_empty())?;
        let api_base_url = env::var("RAIN_API_BASE_URL")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "https://api-dev.raincards.xyz/v1".into());
        let destination_address = env::var("PLATFORM_SIGNER_ADDRESS")
            .ok()
            .filter(|s| !s.is_empty())
            .or_else(|| env::var("RAIN_DESTINATION_ADDRESS").ok().filter(|s| !s.is_empty()))?;
        let destination_rail = env::var("RAIN_DESTINATION_RAIL")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "base".into());
        let team_id = env::var("RAIN_TEAM_ID").ok().filter(|s| !s.is_empty());
        Some(Self {
            api_base_url,
            api_key,
            user_id,
            team_id,
            destination_address,
            destination_rail,
        })
    }
}

#[derive(Debug, Clone)]
pub struct FundingConfig {
    pub mode: FundingMode,
    /// Default seed in micro-USDC.
    pub seed_bankroll_usdc: i64,
    /// Present only for testnet when `RAIN_*` (+ destination) set.
    pub rain: Option<RainConfig>,
}

impl FundingConfig {
    pub fn from_env() -> Self {
        let mode = FundingMode::from_env();
        let whole = env::var("SEED_BANKROLL_USDC")
            .ok()
            .and_then(|v| v.parse::<i64>().ok())
            .unwrap_or(1000);
        let rain = RainConfig::from_env_if_testnet(mode);
        Self {
            mode,
            seed_bankroll_usdc: whole.saturating_mul(USDC_DECIMALS),
            rain,
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
