//! Deterministic market open/resolve via NATS lifecycle intents (fixture helpers).
//! Mimics domain: open when ≥3 stops away; resolve exact → early.

use anyhow::{Context, Result};
use chrono::Utc;
use dto::{
    BusEvent, EventMeta, MarketOpenRequested, MarketOutcome, MarketResolveRequested,
    MARKET_OPENED, MARKET_OPEN_REQUESTED, MARKET_RESOLVE_REQUESTED,
};
use futures::StreamExt;
use transport::connect;
use uuid::Uuid;

use crate::http::BackendClient;
use crate::outcome::outcome_from_arrival;
use crate::wait::{wait_for_condition, WaitOpts};

#[derive(Debug, Clone)]
pub struct OpenFixture {
    pub dedupe_key: String,
    pub trip_id: String,
    pub route_id: String,
    pub stop_id: String,
    pub train_id: String,
    pub frozen_eta_unix: i64,
    /// Documented: ≥3 stops from target (E2E 1 requirement).
    pub remaining_stops: usize,
}

impl OpenFixture {
    pub fn new_random() -> Self {
        let id = Uuid::new_v4().simple().to_string();
        let frozen = Utc::now().timestamp() + 600;
        Self {
            dedupe_key: format!("e2e:{id}:A99"),
            trip_id: format!("e2e-trip-{id}"),
            route_id: "A".into(),
            stop_id: "A99".into(),
            train_id: format!("E2E{id}"),
            frozen_eta_unix: frozen,
            remaining_stops: 3,
        }
    }
}

pub async fn emit_open_requested(nats_url: &str, fix: &OpenFixture) -> Result<()> {
    assert!(
        fix.remaining_stops >= 3,
        "E2E 1 requires train ≥3 stops from target"
    );
    let nats = connect(nats_url).await.context("nats connect")?;
    let req = MarketOpenRequested {
        meta: EventMeta::new(),
        dedupe_key: fix.dedupe_key.clone(),
        trip_id: fix.trip_id.clone(),
        route_id: fix.route_id.clone(),
        train_id: Some(fix.train_id.clone()),
        stop_id: fix.stop_id.clone(),
        frozen_eta_unix: fix.frozen_eta_unix,
        liquidity_b: Some("100000000".into()),
    };
    nats.emit(MARKET_OPEN_REQUESTED, &BusEvent::MarketOpenRequested(req))
        .await
        .context("emit open_requested")?;
    // Short-lived clients must flush or publish stays buffered and is lost on drop.
    nats.inner()
        .flush()
        .await
        .map_err(|e| anyhow::anyhow!("nats flush: {e}"))?;
    Ok(())
}

pub async fn emit_resolve_requested(
    nats_url: &str,
    dedupe_key: &str,
    actual_arrival_unix: i64,
    frozen_eta_unix: i64,
) -> Result<()> {
    let outcome_str = outcome_from_arrival(actual_arrival_unix, frozen_eta_unix);
    let outcome = match outcome_str {
        "early" => MarketOutcome::Early,
        "late" => MarketOutcome::Late,
        other => anyhow::bail!("invalid outcome {other}"),
    };
    let nats = connect(nats_url).await.context("nats connect")?;
    let req = MarketResolveRequested {
        meta: EventMeta::new(),
        dedupe_key: dedupe_key.to_string(),
        actual_arrival_unix,
        outcome,
    };
    nats.emit(
        MARKET_RESOLVE_REQUESTED,
        &BusEvent::MarketResolveRequested(req),
    )
    .await
    .context("emit resolve_requested")?;
    nats.inner()
        .flush()
        .await
        .map_err(|e| anyhow::anyhow!("nats flush: {e}"))?;
    Ok(())
}

/// Open fixture + wait for on-chain/ledger open via bus fact (preferred over /markets 200-cap).
pub async fn open_market_and_wait(
    nats_url: &str,
    fix: &OpenFixture,
    opts: WaitOpts,
) -> Result<String> {
    // Subscribe BEFORE emit so we cannot miss the fact.
    let nats = connect(nats_url).await.context("nats connect")?;
    let mut sub = nats
        .inner()
        .subscribe(MARKET_OPENED.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("subscribe opened: {e}"))?;

    emit_open_requested(nats_url, fix).await?;

    let deadline = tokio::time::Instant::now() + opts.timeout;
    loop {
        let left = deadline.saturating_duration_since(tokio::time::Instant::now());
        if left.is_zero() {
            anyhow::bail!(
                "timeout waiting for market.opened trip={} (backend may be backlogged)",
                fix.trip_id
            );
        }
        match tokio::time::timeout(left, sub.next()).await {
            Ok(Some(msg)) => {
                if let Ok(BusEvent::MarketOpened(opened)) =
                    serde_json::from_slice::<BusEvent>(&msg.payload)
                {
                    if opened.trip_id == fix.trip_id || opened.dedupe_key == fix.dedupe_key {
                        return Ok(opened.market_id.0);
                    }
                }
            }
            Ok(None) => anyhow::bail!("opened subscription ended"),
            Err(_) => anyhow::bail!(
                "timeout waiting for market.opened trip={}",
                fix.trip_id
            ),
        }
    }
}

/// Wait until ledger/API shows a market for this trip (HTTP fallback).
pub async fn wait_for_market_by_trip(
    client: &BackendClient,
    trip_id: &str,
    opts: WaitOpts,
) -> Result<String> {
    wait_for_condition(opts, &format!("market trip={trip_id}"), || {
        let client = client.clone();
        let trip_id = trip_id.to_string();
        async move {
            match client.list_markets().await {
                Ok(list) => Ok(list.markets.iter().any(|m| m.trip_id == trip_id)),
                Err(_) => Ok(false),
            }
        }
    })
    .await?;

    let list = client.list_markets().await?;
    let m = list
        .markets
        .iter()
        .find(|m| m.trip_id == trip_id)
        .context("market vanished after wait")?;
    Ok(m.market_id.clone())
}

/// Wait until market outcome is early|late (never void/on-time).
pub async fn wait_for_resolved(
    client: &BackendClient,
    market_id: &str,
    opts: WaitOpts,
) -> Result<String> {
    wait_for_condition(opts, &format!("resolve {market_id}"), || {
        let client = client.clone();
        let market_id = market_id.to_string();
        async move {
            match client.get_market(&market_id).await {
                Ok(detail) => {
                    let o = detail.market.outcome.as_str();
                    Ok(matches!(o, "early" | "late"))
                }
                Err(_) => Ok(false),
            }
        }
    })
    .await?;
    let detail = client.get_market(market_id).await?;
    Ok(detail.market.outcome.as_str().to_string())
}
