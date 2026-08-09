//! E2E 2 — Human buy/sell moves LMSR prices (`FUNDING_MODE=local`).
//!
//! Automated path: random EVM wallet → sign session challenge → create agent →
//! seed bankroll → buy/sell over HTTP. Platform signer submits chain txs (human
//! never signs LMSR). Frontend Phantom connect stubbed.
//!
//! Gate: `E2E_FULL=1`.

use std::time::Duration;

use dto::MarketSide;
use e2e::{
    assert_near_half, ensure_platform_funded, establish_human_session, open_market_and_wait,
    price_moved_down, price_moved_up, wait_for_condition, wait_for_http_ok, BackendClient, E2eEnv,
    OpenFixture, WaitOpts,
};

const SHARES: &str = "1000000"; // 1 USDC micro-share unit

#[tokio::test]
async fn e2e2_human_buy_sell_moves_prices() {
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

    ensure_platform_funded().expect("mint MockUSDC");

    // Open market fixture (or reuse existing open market).
    let fix = OpenFixture::new_random();
    let market_id = open_market_and_wait(&env.backend.nats_url, &fix, opts.clone())
        .await
        .expect("market open");

    // Bus fact can beat ledger projection — wait until GET /markets/:id succeeds.
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

    let detail0 = client.get_market(&market_id).await.expect("detail");
    assert_near_half(&detail0.price_early_wad, &detail0.price_late_wad, 100)
        .expect("open ~50/50");
    let early0 = detail0.price_early_wad.clone();
    let late0 = detail0.price_late_wad.clone();

    let human = establish_human_session(&client)
        .await
        .expect("CLI wallet session");
    let agent = client
        .create_agent(&human.session_token, Some("e2e2-human"))
        .await
        .expect("create agent");
    client
        .seed_bankroll(
            &human.session_token,
            agent.agent_id.as_ref(),
            Some(1_000),
        )
        .await
        .expect("seed bankroll (FUNDING_MODE=local)");

    // 1) Buy early → early up, late down.
    let buy_early = client
        .buy(
            &human.session_token,
            &market_id,
            MarketSide::Early,
            SHARES,
            Some(agent.agent_id.as_ref()),
        )
        .await
        .expect("buy early");
    assert!(
        !buy_early.trade.chain_tx_hash.is_empty(),
        "platform signer must land chain tx"
    );

    let after_buy_early = client.get_market(&market_id).await.expect("prices");
    assert!(
        price_moved_up(&early0, &after_buy_early.price_early_wad).unwrap(),
        "early price should rise after buy early"
    );
    assert!(
        price_moved_down(&late0, &after_buy_early.price_late_wad).unwrap(),
        "late price should fall after buy early"
    );
    let early1 = after_buy_early.price_early_wad.clone();

    // 2) Sell early → early down.
    let sell_early = client
        .sell(
            &human.session_token,
            &market_id,
            MarketSide::Early,
            SHARES,
            Some(agent.agent_id.as_ref()),
        )
        .await
        .expect("sell early");
    assert!(!sell_early.trade.chain_tx_hash.is_empty());

    let after_sell = client.get_market(&market_id).await.expect("prices");
    assert!(
        price_moved_down(&early1, &after_sell.price_early_wad).unwrap(),
        "early price should fall after sell"
    );
    let late_before_other = after_sell.price_late_wad.clone();

    // 3) Buy late → late up.
    let buy_late = client
        .buy(
            &human.session_token,
            &market_id,
            MarketSide::Late,
            SHARES,
            Some(agent.agent_id.as_ref()),
        )
        .await
        .expect("buy late");
    assert!(!buy_late.trade.chain_tx_hash.is_empty());

    let after_buy_late = client.get_market(&market_id).await.expect("prices");
    assert!(
        price_moved_up(&late_before_other, &after_buy_late.price_late_wad).unwrap(),
        "late price should rise after buy late"
    );

    eprintln!(
        "E2E 2 PASS wallet={} market_id={} agent={}",
        human.address,
        market_id,
        agent.agent_id.as_ref()
    );
}
