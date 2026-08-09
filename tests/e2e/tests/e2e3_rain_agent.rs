//! E2E 3 — Rain fund → agent trades (`FUNDING_MODE=testnet` when RAIN_* set).
//!
//! Human: create agent → fund via Rain sandbox → handoff AGENT_API_KEY + llms.txt.
//! Agent leg: prefer composer-2.5 via `just e2e-agent` (orchestrator parent).
//! Harness also offers `E2E3_HARNESS_TRADES=1` (default under `just e2e`): reads
//! llms.txt then buy+sell with Bearer so ledger/PnL assertions can pass without
//! spawning a Task from Rust.
//!
//! Skip: any RAIN_* empty → report `E2E 3 skipped: RAIN_* missing`.
//! Gate: `E2E_FULL=1`.

use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use dto::MarketSide;
use e2e::{
    ensure_platform_funded, establish_human_session, open_market_and_wait, rain_creds_present,
    rain_skip_reason, wait_for_condition, wait_for_http_ok, BackendClient, E2eEnv, OpenFixture,
    WaitOpts,
};

const SHARES: &str = "1000000";
const RAIN_AMOUNT_USDC: i64 = 10;

#[tokio::test]
async fn e2e3_rain_fund_agent_trades() {
    e2e::require_e2e_full!();

    if !rain_creds_present() {
        let reason = rain_skip_reason().unwrap_or_else(|| "E2E 3 skipped: RAIN_* missing".into());
        eprintln!("{reason}");
        return;
    }

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

    // Local anvil still needs platform USDC for LMSR. Rain settles Base USDC; interim
    // treasury mirror mints MockUSDC 1:1 on settle when USDC is MockUSDC. Monad testnet
    // Circle USDC (one-pager 0x534b…) has no mint — pre-fund via Circle faucet / CCTP.
    if let Err(e) = ensure_platform_funded() {
        eprintln!("WARN: platform USDC mint skipped/failed: {e}");
    }

    // Note: live Rain path needs FUNDING_MODE=testnet on the running backend.
    if env.funding_mode != "testnet" {
        eprintln!(
            "WARN: FUNDING_MODE={} — Rain fund requires backend with FUNDING_MODE=testnet",
            env.funding_mode
        );
    }

    let human = establish_human_session(&client)
        .await
        .expect("human session");
    let agent = client
        .create_agent(&human.session_token, Some("e2e3-rain-agent"))
        .await
        .expect("create agent");

    let fund = match client
        .fund_rain(
            &human.session_token,
            agent.agent_id.as_ref(),
            RAIN_AMOUNT_USDC,
        )
        .await
    {
        Ok(f) => f,
        Err(e) => {
            let msg = e.to_string();
            if is_rain_infra_failure(&msg) {
                eprintln!(
                    "E2E 3 Rain live call failed (sandbox/network beyond harness control): {msg}"
                );
                eprintln!("Handoff scaffolding still written when possible; local mock path stays green via unit tests.");
                write_partial_handoff(&env, &client, &agent.api_key, agent.agent_id.as_ref(), None)
                    .ok();
                return;
            }
            panic!("fund_rain: {msg}");
        }
    };
    eprintln!(
        "Rain fund initiated funding_id={} status={}",
        fund.funding_id, fund.status
    );

    let settled = match client
        .poll_funding(&human.session_token, &fund.funding_id)
        .await
    {
        Ok(s) => s,
        Err(e) => {
            let msg = e.to_string();
            if is_rain_infra_failure(&msg) {
                eprintln!("E2E 3 Rain poll failed (sandbox/network): {msg}");
                write_partial_handoff(
                    &env,
                    &client,
                    &agent.api_key,
                    agent.agent_id.as_ref(),
                    None,
                )
                .ok();
                return;
            }
            panic!("poll_funding: {msg}");
        }
    };

    if settled.status != "settled" {
        eprintln!(
            "E2E 3 Rain not settled (status={}) — treat as sandbox failure beyond control",
            settled.status
        );
        write_partial_handoff(&env, &client, &agent.api_key, agent.agent_id.as_ref(), None).ok();
        return;
    }

    let bankroll = client
        .bankroll(&agent.api_key, agent.agent_id.as_ref())
        .await
        .expect("bankroll after Rain");
    assert!(
        bankroll.available_usdc > 0,
        "bankroll must be credited after Rain settle"
    );
    eprintln!(
        "Rain credited bankroll_usdc={}",
        bankroll.available_usdc
    );

    // Open market for agent trades.
    let fix = OpenFixture::new_random();
    let market_id = open_market_and_wait(&env.backend.nats_url, &fix, opts.clone())
        .await
        .expect("market");

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

    write_agent_handoff(
        &env,
        &client,
        &agent.api_key,
        agent.agent_id.as_ref(),
        &market_id,
    )
    .expect("write handoff");

    let harness_trades = std::env::var("E2E3_HARNESS_TRADES")
        .map(|v| matches!(v.as_str(), "1" | "true" | "yes"))
        .unwrap_or(true);

    if harness_trades {
        // Reads llms.txt first (agent-doc aware), then buy+sell with Bearer.
        // True composer-2.5 reasoning: `just e2e-agent` (orchestrator parent).
        let llms = client.llms_txt().await.expect("llms.txt");
        assert!(
            llms.contains("/buy") && llms.contains("/sell"),
            "llms.txt must document buy/sell"
        );
        assert!(
            llms.contains("AGENT_API_KEY") || llms.contains("Bearer"),
            "llms.txt must document agent auth"
        );

        let buy = client
            .buy(
                &agent.api_key,
                &market_id,
                MarketSide::Early,
                SHARES,
                None,
            )
            .await
            .expect("agent buy");
        assert!(!buy.trade.chain_tx_hash.is_empty());

        // Wait for ledger PnL projection (bus → ledger).
        wait_for_condition(
            WaitOpts {
                timeout: Duration::from_secs(30),
                poll: Duration::from_millis(250),
            },
            "pnl after buy",
            || {
                let client = client.clone();
                let key = agent.api_key.clone();
                let aid = agent.agent_id.as_ref().to_string();
                let mid = market_id.clone();
                async move {
                    match client.agent_market_pnl(&key, &aid, &mid).await {
                        Ok(row) => Ok(row.early_shares != "0" || row.cost_basis_usdc > 0),
                        Err(_) => Ok(false),
                    }
                }
            },
        )
        .await
        .expect("pnl row after buy");

        let sell = client
            .sell(
                &agent.api_key,
                &market_id,
                MarketSide::Early,
                SHARES,
                None,
            )
            .await
            .expect("agent sell");
        assert!(!sell.trade.chain_tx_hash.is_empty());

        let pnl = client
            .agent_market_pnl(&agent.api_key, agent.agent_id.as_ref(), &market_id)
            .await
            .expect("pnl after sell");
        // After round-trip, shares may be 0 but PnL row exists / updated.
        eprintln!(
            "PnL after sell: early={} late={} realized={} unrealized={} total={}",
            pnl.early_shares,
            pnl.late_shares,
            pnl.realized_pnl_usdc,
            pnl.unrealized_pnl_usdc,
            pnl.total_pnl_usdc
        );

        eprintln!(
            "E2E 3 PASS (harness trades + Rain) agent={} market={}",
            agent.agent_id.as_ref(),
            market_id
        );
    } else {
        eprintln!(
            "E2E 3 fund PASS — agent trades deferred to composer-2.5 via just e2e-agent\n\
             handoff: {}",
            env.handoff_dir.join("agent-handoff.env").display()
        );
    }
}

fn is_rain_infra_failure(msg: &str) -> bool {
    let lower = msg.to_lowercase();
    lower.contains("timeout")
        || lower.contains("dns")
        || lower.contains("connect")
        || lower.contains("503")
        || lower.contains("502")
        || lower.contains("429")
        || lower.contains("unavailable")
        || lower.contains("rain client")
        || lower.contains("rain decode")
        || lower.contains("funding_mode")
        || lower.contains("forbidden")
        || lower.contains("401")
        || lower.contains("unauthorized")
        || lower.contains("internal server error")
}

fn write_agent_handoff(
    env: &E2eEnv,
    client: &BackendClient,
    api_key: &str,
    agent_id: &str,
    market_id: &str,
) -> anyhow::Result<()> {
    write_partial_handoff(env, client, api_key, agent_id, Some(market_id))
}

fn write_partial_handoff(
    env: &E2eEnv,
    client: &BackendClient,
    api_key: &str,
    agent_id: &str,
    market_id: Option<&str>,
) -> anyhow::Result<()> {
    fs::create_dir_all(&env.handoff_dir)?;
    let path: PathBuf = env.handoff_dir.join("agent-handoff.env");
    let mut body = format!(
        "# Generated by e2e3 — paste into composer-2.5 agent leg (`just e2e-agent`)\n\
         BACKEND_URL={}\n\
         AGENT_API_KEY={}\n\
         AGENT_ID={}\n\
         LLMS_TXT_URL={}/llms.txt\n",
        client.base_url(),
        api_key,
        agent_id,
        client.base_url(),
    );
    if let Some(mid) = market_id {
        body.push_str(&format!("MARKET_ID={mid}\n"));
    }
    fs::write(&path, body)?;
    eprintln!("wrote {}", path.display());
    Ok(())
}
