//! Buy/sell — debit bankroll first, chain tx, mandatory `subway.trade.filled` emit.

use std::time::Duration;

use axum::{extract::State, Json};
use chrono::Utc;
use dto::{
    AgentId, BusEvent, BuyRequest, EventMeta, MarketId, MarketOutcome, SellRequest, Trade,
    TradeAction, TradeFilled, TradeId, TradeResponse, TRADE_FILLED,
};
use tracing::{error, warn};
use transport::TransportClient;

use crate::agents::{
    credit_bankroll_locked, debit_bankroll_locked, ensure_human_owns_agent, resolve_trading_agent,
};
use crate::auth::{AuthIdentity, AuthUser};
use crate::error::ApiError;
use crate::markets::load_market;
use crate::state::AppState;
use crate::trading_chain::{agent_custody_address, parse_market_id, parse_shares};

/// Bounded emit retries after chain success (decision #10 — ledger must see fill).
const EMIT_ATTEMPTS: u32 = 5;
const EMIT_BACKOFF_BASE_MS: u64 = 50;
const EMIT_BACKOFF_CAP_MS: u64 = 800;

pub async fn buy(
    State(state): State<AppState>,
    AuthUser(identity): AuthUser,
    Json(req): Json<BuyRequest>,
) -> Result<Json<TradeResponse>, ApiError> {
    execute_trade(state, identity, req.market_id, req.side, req.shares, req.agent_id, TradeAction::Buy)
        .await
}

pub async fn sell(
    State(state): State<AppState>,
    AuthUser(identity): AuthUser,
    Json(req): Json<SellRequest>,
) -> Result<Json<TradeResponse>, ApiError> {
    execute_trade(
        state,
        identity,
        req.market_id,
        req.side,
        req.shares,
        req.agent_id,
        TradeAction::Sell,
    )
    .await
}

async fn execute_trade(
    state: AppState,
    identity: AuthIdentity,
    market_id_raw: String,
    side: dto::MarketSide,
    shares_raw: String,
    requested_agent: Option<AgentId>,
    action: TradeAction,
) -> Result<Json<TradeResponse>, ApiError> {
    let nats = state.nats.as_ref().ok_or_else(|| {
        ApiError::Unavailable("nats unavailable — trading disabled until bus connects".into())
    })?;
    let chain = state
        .chain
        .as_ref()
        .ok_or_else(|| ApiError::Unavailable("chain not configured".into()))?;

    let agent_id = resolve_trading_agent(&identity, requested_agent.as_ref())?;
    if let AuthIdentity::Human { wallet } = &identity {
        ensure_human_owns_agent(&state, wallet, &agent_id).await?;
    }

    let market = load_market(&state, &market_id_raw).await?;
    if market.outcome != MarketOutcome::Unresolved.as_str() {
        return Err(ApiError::BadRequest("market is resolved".into()));
    }

    let mid = parse_market_id(&market_id_raw)?;
    let shares = parse_shares(&shares_raw)?;
    if shares == 0 {
        return Err(ApiError::BadRequest("shares must be > 0".into()));
    }

    let account = agent_custody_address(&agent_id);

    let usdc_amount = match action {
        TradeAction::Buy => {
            let cost = chain.quote_buy(mid, side, shares).await?;
            i64::try_from(cost).map_err(|_| ApiError::Internal("cost exceeds i64".into()))?
        }
        TradeAction::Sell => {
            let proceeds = chain.quote_sell(mid, side, shares).await?;
            i64::try_from(proceeds).map_err(|_| ApiError::Internal("proceeds exceed i64".into()))?
        }
    };

    // Buy: reserve/debit BEFORE chain (FOR UPDATE). Sell: credit AFTER chain success.
    if matches!(action, TradeAction::Buy) {
        debit_bankroll_locked(&state, &agent_id, usdc_amount).await?;
    }

    let tx_hash = match action {
        TradeAction::Buy => match chain.buy(mid, account, side, shares).await {
            Ok(h) => h,
            Err(err) => {
                if let Err(restore_err) =
                    credit_bankroll_locked(&state, &agent_id, usdc_amount).await
                {
                    error!(
                        agent_id = %agent_id,
                        usdc_amount,
                        error = %restore_err.message(),
                        "CRITICAL: chain buy failed and bankroll credit-back failed"
                    );
                }
                return Err(err);
            }
        },
        TradeAction::Sell => chain.sell(mid, account, side, shares).await?,
    };

    if matches!(action, TradeAction::Sell) {
        credit_bankroll_locked(&state, &agent_id, usdc_amount).await?;
    }

    let price_wad = match side {
        dto::MarketSide::Early => chain
            .price_early(mid)
            .await
            .unwrap_or_else(|_| "500000000000000000".into()),
        dto::MarketSide::Late => chain
            .price_late(mid)
            .await
            .unwrap_or_else(|_| "500000000000000000".into()),
    };

    let trade_id = TradeId::new();
    let meta = EventMeta::new();
    let filled_at = Utc::now();
    let fill = TradeFilled {
        meta: meta.clone(),
        trade_id: trade_id.clone(),
        market_id: MarketId(market_id_raw.clone()),
        agent_id: AgentId(agent_id.clone()),
        side,
        action,
        shares: shares_raw.clone(),
        usdc_amount,
        price_wad: price_wad.clone(),
        chain_tx_hash: tx_hash.clone(),
        filled_at,
    };

    emit_trade_filled(nats, &fill).await?;

    let trade = Trade {
        trade_id,
        event_id: meta.event_id.0,
        market_id: MarketId(market_id_raw),
        agent_id: AgentId(agent_id),
        side,
        action,
        shares: shares_raw,
        usdc_amount,
        price_wad,
        chain_tx_hash: tx_hash,
        filled_at,
    };

    Ok(Json(TradeResponse { trade }))
}

async fn emit_trade_filled(nats: &TransportClient, fill: &TradeFilled) -> Result<(), ApiError> {
    let event = BusEvent::TradeFilled(fill.clone());
    let mut last_err = String::new();

    for attempt in 0..EMIT_ATTEMPTS {
        match nats.emit(TRADE_FILLED, &event).await {
            Ok(()) => return Ok(()),
            Err(err) => {
                last_err = err.to_string();
                warn!(
                    attempt = attempt + 1,
                    max = EMIT_ATTEMPTS,
                    error = %last_err,
                    "trade.filled emit failed — retrying"
                );
                if attempt + 1 < EMIT_ATTEMPTS {
                    let backoff_ms = (EMIT_BACKOFF_BASE_MS << attempt).min(EMIT_BACKOFF_CAP_MS);
                    tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
                }
            }
        }
    }

    let payload = serde_json::to_string(&event).unwrap_or_else(|_| "<serialize failed>".into());
    error!(
        error = %last_err,
        attempts = EMIT_ATTEMPTS,
        subject = TRADE_FILLED,
        event_payload = %payload,
        "CRITICAL: trade.filled emit exhausted after chain success — ledger will miss fill; replay payload"
    );
    Err(ApiError::Internal(format!(
        "emit trade.filled failed after {EMIT_ATTEMPTS} attempts: {last_err}"
    )))
}
