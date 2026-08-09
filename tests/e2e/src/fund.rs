//! Local MockUSDC mint for platform signer (deploy-local does not seed balances).

use anyhow::{bail, Context, Result};
use std::process::Command;

/// Anvil account #1 — mint from here so we never steal platform signer nonces.
const MINTER_PK: &str = "0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d";

fn rpc_url() -> String {
    std::env::var("MONAD_RPC_URL").unwrap_or_else(|_| "http://127.0.0.1:8545".into())
}

fn usdc_address() -> Result<String> {
    let usdc = std::env::var("USDC_ADDRESS").context("USDC_ADDRESS")?;
    if usdc.trim().is_empty() {
        bail!("USDC_ADDRESS empty — run just deploy-local first");
    }
    Ok(usdc)
}

fn platform_address() -> String {
    std::env::var("PLATFORM_SIGNER_ADDRESS").unwrap_or_else(|_| {
        "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266".into()
    })
}

/// Current MockUSDC balance of platform (micro-USDC).
pub fn platform_usdc_balance() -> Result<u128> {
    let usdc = usdc_address()?;
    let platform = platform_address();
    let out = Command::new("cast")
        .args([
            "call",
            &usdc,
            "balanceOf(address)(uint256)",
            &platform,
            "--rpc-url",
            &rpc_url(),
        ])
        .output()
        .context("cast call balanceOf")?;
    if !out.status.success() {
        bail!(
            "balanceOf failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let raw = String::from_utf8_lossy(&out.stdout);
    // cast may print `123 [1.23e2]` — take first token.
    let token = raw.split_whitespace().next().unwrap_or("0");
    token.parse::<u128>().with_context(|| format!("parse balance '{token}'"))
}

/// Mint MockUSDC to platform. Uses anvil #1 as minter (not platform) to avoid nonce races.
pub fn mint_platform_usdc(amount_micro: u128) -> Result<()> {
    let usdc = usdc_address()?;
    let platform = platform_address();

    let out = Command::new("cast")
        .args([
            "send",
            &usdc,
            "mint(address,uint256)",
            &platform,
            &amount_micro.to_string(),
            "--private-key",
            MINTER_PK,
            "--rpc-url",
            &rpc_url(),
        ])
        .output()
        .context("spawn cast mint")?;

    if !out.status.success() {
        bail!(
            "cast mint failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    Ok(())
}

/// Ensure platform has ≥ `min_micro` USDC for LMSR subsidies (idempotent).
pub fn ensure_platform_funded() -> Result<()> {
    const MIN: u128 = 100_000 * 1_000_000; // 100k USDC
    const TOP_UP: u128 = 1_000_000 * 1_000_000;
    match platform_usdc_balance() {
        Ok(bal) if bal >= MIN => Ok(()),
        _ => mint_platform_usdc(TOP_UP),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn minter_is_not_anvil0() {
        // anvil #0 starts with ac0974… — minter must differ.
        assert!(!super::MINTER_PK.contains("ac0974bec39a17e36ba4a6b4d238ff944bacb478"));
    }
}
