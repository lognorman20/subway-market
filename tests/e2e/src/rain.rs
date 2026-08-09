//! Rain credential gate for E2E 3.

use crate::config::RainCreds;

pub fn rain_creds_present() -> bool {
    RainCreds::from_env().is_some()
}

/// Human-readable skip reason when any RAIN_* is missing.
pub fn rain_skip_reason() -> Option<String> {
    if RainCreds::from_env().is_some() {
        return None;
    }
    let missing: Vec<&str> = [
        "RAIN_TEAM_ID",
        "RAIN_USER_ID",
        "RAIN_API_KEY",
        "RAIN_COLLATERAL_CONTRACT_ID",
    ]
    .into_iter()
    .filter(|k| std::env::var(k).ok().filter(|v| !v.trim().is_empty()).is_none())
    .collect();
    Some(format!(
        "E2E 3 skipped: RAIN_* missing ({})",
        missing.join(", ")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skip_reason_shape() {
        // With or without creds — helper must not panic.
        let _ = rain_creds_present();
        if let Some(msg) = rain_skip_reason() {
            assert!(msg.contains("E2E 3 skipped"));
        }
    }
}
