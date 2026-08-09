//! Buy/sell — validate bankroll, chain tx (platform signer), emit `subway.trade.filled`.

use axum::{extract::State, Json};
use chrono::Utc;
use dto::{
    AgentId, BusEvent, BuyRequest, EventMeta, MarketId, MarketOutcome, SellRequest, Trade,
    TradeAction, TradeFilled, TradeId, TradeResponse, TRADE_FILLED,
};
use tracing::{info, warn};

use crate::agents::{
    adjust_bankroll, ensure_human_owns_agent, load_bankroll, resolve_trading_agent,
};
use crate::auth::{AuthIdentity, AuthUser};
use crate::error::ApiError;
use crate::markets::load_market;
use crate::state::AppState;
use crate::trading_chain::{agent_custody_address, parse_market_id, parse_shares};

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
            let cost_i = i64::try_from(cost)
                .map_err(|_| ApiError::Internal("cost exceeds i64".into()))?;
            let (bankroll, _) = load_bankroll(&state, &agent_id).await?;
            if bankroll < cost_i {
                return Err(ApiError::InsufficientBankroll);
            }
            cost_i
        }
        TradeAction::Sell => {
            let proceeds = chain.quote_sell(mid, side, shares).await?;
            i64::try_from(proceeds).map_err(|_| ApiError::Internal("proceeds exceed i64".into()))?
        }
    };

    let tx_hash = match action {
        TradeAction::Buy => chain.buy(mid, account, side, shares).await?,
        TradeAction::Sell => chain.sell(mid, account, side, shares).await?,
    };

    // Debit on buy, credit on sell.
    let delta = match action {
        TradeAction::Buy => -usdc_amount,
        TradeAction::Sell => usdc_amount,
    };
    adjust_bankroll(&state, &agent_id, delta).await?;

    let price_wad = match side {
        dto::MarketSide::Early => chain.price_early(mid).await.unwrap_or_else(|_| {
            "500000000000000000".into()
        }),
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

    if let Some(nats) = &state.nats {
        let event = BusEvent::TradeFilled(fill.clone());
        if let Err(err) = nats.emit(TRADE_FILLED, &event).await {
            warn!(error = %err, "failed to emit trade.filled");
            return Err(ApiError::Internal(format!("emit trade.filled: {err}")));
        }
    } else {
        info!("nats absent — skipping trade.filled emit");
    }

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
