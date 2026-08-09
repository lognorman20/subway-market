//! Env + stack URLs for E2E runners.

use std::env;
use std::path::PathBuf;
use std::time::Duration;

/// Rain sandbox credentials required for E2E 3 go-time acceptance.
#[derive(Debug, Clone)]
pub struct RainCreds {
    pub api_base_url: String,
    pub team_id: String,
    pub user_id: String,
    pub api_key: String,
    pub collateral_contract_id: String,
}

impl RainCreds {
    pub fn from_env() -> Option<Self> {
        let team_id = non_empty("RAIN_TEAM_ID")?;
        let user_id = non_empty("RAIN_USER_ID")?;
        let api_key = non_empty("RAIN_API_KEY")?;
        let collateral_contract_id = non_empty("RAIN_COLLATERAL_CONTRACT_ID")?;
        let api_base_url = env::var("RAIN_API_BASE_URL")
            .unwrap_or_else(|_| "https://api-dev.raincards.xyz/v1".into());
        Some(Self {
            api_base_url,
            team_id,
            user_id,
            api_key,
            collateral_contract_id,
        })
    }
}

fn non_empty(key: &str) -> Option<String> {
    env::var(key).ok().filter(|v| !v.trim().is_empty())
}

#[derive(Debug, Clone)]
pub struct BackendConfig {
    pub base_url: String,
    pub nats_url: String,
    pub timeout: Duration,
}

impl BackendConfig {
    pub fn from_env() -> Self {
        let base_url = env::var("BACKEND_URL")
            .or_else(|_| env::var("E2E_BACKEND_URL"))
            .unwrap_or_else(|_| "http://127.0.0.1:8080".into());
        let nats_url =
            env::var("NATS_URL").unwrap_or_else(|_| "nats://127.0.0.1:4223".into());
        let timeout_secs: u64 = env::var("E2E_TIMEOUT_SECS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(120);
        Self {
            base_url,
            nats_url,
            timeout: Duration::from_secs(timeout_secs),
        }
    }
}

/// Full-stack gate: `E2E_FULL=1` (or `true`/`yes`).
pub fn e2e_full_enabled() -> bool {
    matches!(
        env::var("E2E_FULL").ok().as_deref(),
        Some("1") | Some("true") | Some("yes") | Some("TRUE") | Some("YES")
    )
}

/// Skip full-stack test unless `E2E_FULL` set.
#[macro_export]
macro_rules! require_e2e_full {
    () => {
        if !$crate::config::e2e_full_enabled() {
            eprintln!("skip: set E2E_FULL=1 for full-stack E2E (stack: anvil → deploy-local → up → start)");
            return;
        }
    };
}

#[derive(Debug, Clone)]
pub struct E2eEnv {
    pub backend: BackendConfig,
    pub funding_mode: String,
    pub rain: Option<RainCreds>,
    pub handoff_dir: PathBuf,
}

impl E2eEnv {
    pub fn load() -> Self {
        let backend = BackendConfig::from_env();
        let funding_mode =
            env::var("FUNDING_MODE").unwrap_or_else(|_| "local".into());
        let rain = RainCreds::from_env();
        let handoff_dir = env::var("E2E_HANDOFF_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                // Prefer repo-root/.e2e even when cargo runs from package dir.
                PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".into()))
                    .join("../..")
                    .join(".e2e")
            });
        Self {
            backend,
            funding_mode,
            rain,
            handoff_dir,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn e2e_full_default_off() {
        // Don't assert env; just that helper is callable.
        let _ = e2e_full_enabled();
    }

    #[test]
    fn backend_config_defaults() {
        let cfg = BackendConfig::from_env();
        assert!(cfg.base_url.contains("8080") || cfg.base_url.starts_with("http"));
        assert!(!cfg.nats_url.is_empty());
    }
}
