//! `FUNDING_MODE` + local seed bankroll + optional Rain testnet config.

use std::env;

/// Whole-USDC seed from env → micro-USDC (6 decimals).
pub const USDC_DECIMALS: i64 = 1_000_000;

/// Rain sandbox docs example destination — passes compliance; anvil #0 does **not**.
/// Prefer a real user-controlled Base address in production; this is the sandbox tip.
pub const RAIN_SANDBOX_COMPLIANT_DEST_EXAMPLE: &str =
    "0x1234567890abcdef1234567890abcdef12345678";

/// Circle USDC on Monad **testnet** — from `docs/raingentic-monad-builder-one-pager.pdf`.
pub const MONAD_TESTNET_USDC: &str = "0x534b2f3A21130d7a60830c2Df862319e593943A3";

/// Monad testnet chain id (one-pager: chain 10143).
pub const MONAD_TESTNET_CHAIN_ID: u64 = 10143;

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
    /// On-chain destination for ACH→USDC onramp (Rain rails: Base/Eth/… — **not** Monad).
    /// Must pass Rain compliance (do not use anvil #0 / PLATFORM_SIGNER_ADDRESS).
    pub destination_address: String,
    /// Crypto rail for onramp destination (default `base`).
    pub destination_rail: String,
}

/// True when Rain API base looks like sandbox/dev (not production).
pub fn is_rain_sandbox_api_url(api_base_url: &str) -> bool {
    let lower = api_base_url.to_ascii_lowercase();
    lower.contains("api-dev.raincards")
        || lower.contains("sandbox")
        || lower.contains("rain-sandbox")
}

/// Resolve sandbox credit-on-simulate:
/// - `RAIN_SANDBOX_CREDIT_ON_SIMULATE=0|false|no` → off
/// - `=1|true|yes` → on
/// - unset → on when `api_base_url` is sandbox/dev, else off
pub fn resolve_sandbox_credit_on_simulate(api_base_url: &str) -> bool {
    match env::var("RAIN_SANDBOX_CREDIT_ON_SIMULATE") {
        Ok(v) => {
            let v = v.trim().to_ascii_lowercase();
            matches!(v.as_str(), "1" | "true" | "yes" | "on")
        }
        Err(_) => is_rain_sandbox_api_url(api_base_url),
    }
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
        let destination_address = resolve_rain_destination_address()?;
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

/// Prefer `RAIN_DESTINATION_ADDRESS`. Never use `PLATFORM_SIGNER_ADDRESS` (anvil fails compliance).
///
/// Optional deferred discovery: when unset, `ensure_ach_usdc_onramp` may fill from an existing
/// Rain route; config load still needs an address so Rain client can boot — use env or leave
/// unset and require discovery at ensure time via [`RainConfig::destination_address`] mutation.
fn resolve_rain_destination_address() -> Option<String> {
    if let Some(addr) = env::var("RAIN_DESTINATION_ADDRESS")
        .ok()
        .filter(|s| !s.is_empty())
    {
        if is_anvil_default_address(&addr) {
            tracing::warn!(
                %addr,
                "RAIN_DESTINATION_ADDRESS looks like anvil #0 — Rain compliance will reject it; use a Base wallet that passes Rain checks (sandbox tip: {})",
                RAIN_SANDBOX_COMPLIANT_DEST_EXAMPLE
            );
        }
        return Some(addr);
    }
    // Explicit opt-in only — never silently use PLATFORM_SIGNER_ADDRESS.
    None
}

/// Anvil account #0 (Foundry default) — Rain sandbox rejects for onramp destination.
pub fn is_anvil_default_address(addr: &str) -> bool {
    addr.eq_ignore_ascii_case("0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266")
}

#[derive(Debug, Clone)]
pub struct FundingConfig {
    pub mode: FundingMode,
    /// Default seed in micro-USDC.
    pub seed_bankroll_usdc: i64,
    /// Present only for testnet when `RAIN_*` (+ destination) set.
    pub rain: Option<RainConfig>,
    /// Sandbox-only: credit bankroll immediately after Rain ACH simulate succeeds
    /// (interim — not real ACH finality). Later webhook/poll must not double-credit.
    pub sandbox_credit_on_simulate: bool,
}

impl FundingConfig {
    pub fn from_env() -> Self {
        let mode = FundingMode::from_env();
        let whole = env::var("SEED_BANKROLL_USDC")
            .ok()
            .and_then(|v| v.parse::<i64>().ok())
            .unwrap_or(1000);
        let rain = RainConfig::from_env_if_testnet(mode);
        let sandbox_credit_on_simulate = rain
            .as_ref()
            .map(|r| resolve_sandbox_credit_on_simulate(&r.api_base_url))
            .unwrap_or(false);
        Self {
            mode,
            seed_bankroll_usdc: whole.saturating_mul(USDC_DECIMALS),
            rain,
            sandbox_credit_on_simulate,
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn anvil0_detected() {
        assert!(is_anvil_default_address(
            "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266"
        ));
        assert!(is_anvil_default_address(
            "0xF39FD6E51AAD88F6F4CE6AB8827279CFFFB92266"
        ));
        assert!(!is_anvil_default_address(
            RAIN_SANDBOX_COMPLIANT_DEST_EXAMPLE
        ));
    }

    #[test]
    fn destination_prefers_rain_env_not_platform() {
        let _g = ENV_LOCK.lock().unwrap();
        env::set_var("FUNDING_MODE", "testnet");
        env::set_var("RAIN_API_KEY", "k");
        env::set_var("RAIN_USER_ID", "u");
        env::set_var(
            "RAIN_DESTINATION_ADDRESS",
            RAIN_SANDBOX_COMPLIANT_DEST_EXAMPLE,
        );
        env::set_var(
            "PLATFORM_SIGNER_ADDRESS",
            "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266",
        );
        let cfg = RainConfig::from_env_if_testnet(FundingMode::Testnet).expect("rain");
        assert_eq!(
            cfg.destination_address.to_ascii_lowercase(),
            RAIN_SANDBOX_COMPLIANT_DEST_EXAMPLE.to_ascii_lowercase()
        );
        env::remove_var("RAIN_DESTINATION_ADDRESS");
        env::remove_var("RAIN_API_KEY");
        env::remove_var("RAIN_USER_ID");
        env::remove_var("PLATFORM_SIGNER_ADDRESS");
        env::remove_var("FUNDING_MODE");
    }

    #[test]
    fn destination_absent_without_rain_dest_even_if_platform_set() {
        let _g = ENV_LOCK.lock().unwrap();
        env::set_var("FUNDING_MODE", "testnet");
        env::set_var("RAIN_API_KEY", "k");
        env::set_var("RAIN_USER_ID", "u");
        env::remove_var("RAIN_DESTINATION_ADDRESS");
        env::set_var(
            "PLATFORM_SIGNER_ADDRESS",
            "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266",
        );
        assert!(RainConfig::from_env_if_testnet(FundingMode::Testnet).is_none());
        env::remove_var("RAIN_API_KEY");
        env::remove_var("RAIN_USER_ID");
        env::remove_var("PLATFORM_SIGNER_ADDRESS");
        env::remove_var("FUNDING_MODE");
    }

    #[test]
    fn sandbox_url_detected() {
        assert!(is_rain_sandbox_api_url("https://api-dev.raincards.xyz/v1"));
        assert!(!is_rain_sandbox_api_url("https://api.raincards.xyz/v1"));
    }

    #[test]
    fn credit_on_simulate_env_overrides() {
        let _g = ENV_LOCK.lock().unwrap();
        env::remove_var("RAIN_SANDBOX_CREDIT_ON_SIMULATE");
        assert!(resolve_sandbox_credit_on_simulate(
            "https://api-dev.raincards.xyz/v1"
        ));
        assert!(!resolve_sandbox_credit_on_simulate(
            "https://api.raincards.xyz/v1"
        ));
        env::set_var("RAIN_SANDBOX_CREDIT_ON_SIMULATE", "0");
        assert!(!resolve_sandbox_credit_on_simulate(
            "https://api-dev.raincards.xyz/v1"
        ));
        env::set_var("RAIN_SANDBOX_CREDIT_ON_SIMULATE", "1");
        assert!(resolve_sandbox_credit_on_simulate(
            "https://api.raincards.xyz/v1"
        ));
        env::remove_var("RAIN_SANDBOX_CREDIT_ON_SIMULATE");
    }
}
