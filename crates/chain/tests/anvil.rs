//! Anvil-backed integration tests for chain helpers.
//!
//! Spawns `anvil` via alloy node-bindings when available on PATH.
//! Skips (passes) if `anvil` is missing so `cargo test -p chain` stays green.

use std::process::Command;
use std::str::FromStr;

use alloy::primitives::{Address, U256};
use alloy::providers::{ProviderBuilder, WalletProvider};
use alloy::signers::local::PrivateKeySigner;

use chain::{
    deploy_local, mint_mock_usdc, ChainClient, Outcome, Side,
};

const ONE_USDC: u64 = 1_000_000;
const LIQUIDITY_B: u64 = 100 * ONE_USDC;
const WAD: u128 = 1_000_000_000_000_000_000;

fn anvil_on_path() -> bool {
    Command::new("anvil")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[tokio::test]
async fn anvil_create_buy_sell_resolve() {
    if !anvil_on_path() {
        eprintln!("skip anvil_create_buy_sell_resolve: anvil not on PATH");
        return;
    }

    let provider = ProviderBuilder::new().connect_anvil_with_wallet();
    let platform = provider.default_signer_address();

    let (usdc, market_addr) = deploy_local(&provider, platform)
        .await
        .expect("deploy");

    // Seed platform bankroll for subsidy + buys.
    mint_mock_usdc(
        &provider,
        usdc,
        platform,
        U256::from(1_000_000u64 * ONE_USDC),
    )
    .await
    .expect("mint");

    let client = ChainClient::new(provider, market_addr, usdc, platform);

    let version = client.version().await.expect("version");
    assert!(version.contains("lmsr"), "version={version}");

    let onchain_platform = client.platform().await.expect("platform");
    assert_eq!(onchain_platform, platform);

    let (market_id, _tx) = client
        .create_market("7", "trip-1", "stop-42", 1_700_000_000, U256::from(LIQUIDITY_B))
        .await
        .expect("create_market");
    assert_eq!(market_id, U256::ZERO);

    let state = client.market_state(market_id).await.expect("state");
    assert!(state.exists);
    assert_eq!(state.train_id, "7");
    assert_eq!(state.outcome, Outcome::Unresolved);

    let p_early = client.price_early(market_id).await.expect("price_early");
    let half = U256::from(WAD / 2);
    let tol = U256::from(WAD / 1000);
    assert!(p_early > half - tol && p_early < half + tol, "p_early={p_early}");

    let account = Address::from_str("0x00000000000000000000000000000000000A11CE").unwrap();
    let shares = U256::from(10 * ONE_USDC);

    client
        .buy(market_id, account, Side::Early, shares)
        .await
        .expect("buy");

    let bal = client
        .early_balance(market_id, account)
        .await
        .expect("early_balance");
    assert_eq!(bal, shares);

    let p_early_after = client.price_early(market_id).await.expect("price_early after");
    assert!(p_early_after > p_early, "buy Early should lift Early price");

    let sell_shares = U256::from(ONE_USDC);
    client
        .sell(market_id, account, Side::Early, sell_shares)
        .await
        .expect("sell");
    let bal_after = client
        .early_balance(market_id, account)
        .await
        .expect("early_balance after sell");
    assert_eq!(bal_after, shares - sell_shares);

    client
        .resolve_market(market_id, Outcome::Early)
        .await
        .expect("resolve");

    let state = client.market_state(market_id).await.expect("state resolved");
    assert_eq!(state.outcome, Outcome::Early);
    assert_eq!(
        client.price_early(market_id).await.unwrap(),
        U256::from(WAD)
    );
    assert_eq!(client.price_late(market_id).await.unwrap(), U256::ZERO);
}

#[tokio::test]
async fn anvil_resolve_rejects_unresolved_outcome() {
    if !anvil_on_path() {
        eprintln!("skip anvil_resolve_rejects_unresolved_outcome: anvil not on PATH");
        return;
    }

    let provider = ProviderBuilder::new().connect_anvil_with_wallet();
    let platform = provider.default_signer_address();
    let (usdc, market_addr) = deploy_local(&provider, platform).await.unwrap();
    mint_mock_usdc(
        &provider,
        usdc,
        platform,
        U256::from(1_000_000u64 * ONE_USDC),
    )
    .await
    .unwrap();

    let client = ChainClient::new(provider, market_addr, usdc, platform);
    let (market_id, _) = client
        .create_market("A", "t", "s", 1_700_000_000, U256::from(LIQUIDITY_B))
        .await
        .unwrap();

    let err = client
        .resolve_market(market_id, Outcome::Unresolved)
        .await
        .expect_err("must reject Unresolved");
    assert!(matches!(err, chain::ChainError::InvalidResolveOutcome));
}

#[tokio::test]
async fn anvil_signer_from_key_matches_anvil0() {
    if !anvil_on_path() {
        return;
    }
    // Sanity: default anvil wallet #0 key matches documented address.
    let key = "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80";
    let signer = PrivateKeySigner::from_str(key).unwrap();
    assert_eq!(
        signer.address(),
        Address::from_str("0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266").unwrap()
    );
}
