//! E2E 1 — Market lifecycle (`FUNDING_MODE=local`).
//!
//! 1. Emit open_requested for train ≥3 stops from target (domain-shaped fixture).
//! 2. Market on-chain + ledger; API shows ~50/50 early/late.
//! 3. Resolve with exact schedule → early (never void/on-time).
//!
//! Requires: anvil + deploy-local + up + start (backend/ledger on bus).
//! Gate: `E2E_FULL=1`.

use std::time::Duration;

use e2e::{
    assert_near_half, emit_resolve_requested, ensure_platform_funded, is_valid_resolve_outcome,
    open_market_and_wait, wait_for_condition, wait_for_http_ok, wait_for_resolved, BackendClient,
    E2eEnv, OpenFixture, WaitOpts,
};

#[tokio::test]
async fn e2e1_market_lifecycle() {
    e2e::require_e2e_full!();

    let env = E2eEnv::load();
    let client = BackendClient::new(&env.backend).expect("client");
    let opts = WaitOpts::new(env.backend.timeout);

    wait_for_http_ok(
        client.http(),
        &format!("{}/health", client.base_url()),
        opts.clone(),
    )
    .await
    .expect("backend health");

    ensure_platform_funded().expect("mint MockUSDC to platform for LMSR subsidy");

    let fix = OpenFixture::new_random();
    assert!(
        fix.remaining_stops >= 3,
        "fixture must be ≥3 stops from target"
    );

    let market_id = open_market_and_wait(&env.backend.nats_url, &fix, opts.clone())
        .await
        .expect("market appears via market.opened");

    wait_for_condition(
        WaitOpts {
            timeout: Duration::from_secs(30),
            poll: Duration::from_millis(200),
        },
        "ledger market row",
        || {
            let client = client.clone();
            let mid = market_id.clone();
            async move { Ok(client.get_market(&mid).await.is_ok()) }
        },
    )
    .await
    .expect("ledger projects market");

    let detail = client
        .get_market(&market_id)
        .await
        .expect("get market");
    assert_eq!(detail.market.outcome.as_str(), "unresolved");
    assert!(
        detail.market.chain_create_tx.is_some(),
        "on-chain create tx required"
    );
    assert_near_half(&detail.price_early_wad, &detail.price_late_wad, 100)
        .expect("~50/50 at open");

    // Exact schedule → early.
    emit_resolve_requested(
        &env.backend.nats_url,
        &fix.dedupe_key,
        fix.frozen_eta_unix,
        fix.frozen_eta_unix,
    )
    .await
    .expect("emit resolve_requested exact→early");

    let outcome = wait_for_resolved(&client, &market_id, opts)
        .await
        .expect("market resolves");
    assert!(
        is_valid_resolve_outcome(&outcome),
        "resolve must be early|late, got {outcome}"
    );
    assert_eq!(outcome, "early", "exact schedule must resolve early");

    let resolved = client.get_market(&market_id).await.expect("resolved detail");
    assert!(
        resolved.market.chain_resolve_tx.is_some(),
        "on-chain resolve tx required"
    );
    assert_ne!(resolved.market.outcome.as_str(), "unresolved");
    assert_ne!(resolved.market.outcome.as_str(), "on_time");
    assert_ne!(resolved.market.outcome.as_str(), "void");

    eprintln!("E2E 1 PASS market_id={market_id} outcome={outcome}");
}

#[tokio::test]
async fn e2e1_late_path_also_valid() {
    e2e::require_e2e_full!();

    let env = E2eEnv::load();
    let client = BackendClient::new(&env.backend).expect("client");
    let opts = WaitOpts {
        timeout: env.backend.timeout,
        poll: Duration::from_millis(250),
    };

    wait_for_http_ok(
        client.http(),
        &format!("{}/health", client.base_url()),
        opts.clone(),
    )
    .await
    .expect("backend health");

    ensure_platform_funded().expect("mint MockUSDC");

    let fix = OpenFixture::new_random();
    let market_id = open_market_and_wait(&env.backend.nats_url, &fix, opts.clone())
        .await
        .expect("market");

    // Strictly after → late.
    emit_resolve_requested(
        &env.backend.nats_url,
        &fix.dedupe_key,
        fix.frozen_eta_unix + 1,
        fix.frozen_eta_unix,
    )
    .await
    .expect("resolve late");

    let outcome = wait_for_resolved(&client, &market_id, opts)
        .await
        .expect("resolved");
    assert_eq!(outcome, "late");
    eprintln!("E2E 1 late PASS market_id={market_id}");
}
