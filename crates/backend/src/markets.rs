//! Markets list/detail + trade history (DB read; prices from chain when available).

use axum::{
    extract::{Path, Query, State},
    Json,
};
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use dto::{
    AgentId, Market, MarketDetailResponse, MarketId, MarketOutcome, MarketSide, MarketSummary,
    MarketTradesResponse, MarketsListResponse, Trade, TradeAction, TradeId, TradesListQuery,
};
use db::{AgentMarketPnlRow, MarketRow, TradeRow};

use crate::error::ApiError;
use crate::state::AppState;
use crate::trading_chain::parse_market_id;

const HALF_WAD: &str = "500000000000000000";

pub async fn list_markets(
    State(state): State<AppState>,
) -> Result<Json<MarketsListResponse>, ApiError> {
    use db::schema::markets::dsl::*;

    let mut conn = state
        .pool
        .get()
        .await
        .map_err(|e| ApiError::Internal(format!("pool get: {e}")))?;

    let rows: Vec<MarketRow> = markets
        .order(opened_at.desc())
        .limit(200)
        .select(MarketRow::as_select())
        .load(&mut conn)
        .await?;

    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let (pe, pl) = prices_for(&state, &row.market_id).await;
        out.push(MarketSummary {
            market_id: row.market_id,
            route_id: row.route_id,
            trip_id: row.trip_id,
            start_stop_id: row.start_stop_id,
            start_eta_unix: row.start_eta_unix,
            stop_id: row.stop_id,
            frozen_eta_unix: row.frozen_eta_unix,
            outcome: row.outcome,
            price_early_wad: pe,
            price_late_wad: pl,
        });
    }

    Ok(Json(MarketsListResponse { markets: out }))
}

pub async fn get_market(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<MarketDetailResponse>, ApiError> {
    let row = load_market(&state, &id).await?;
    let (pe, pl) = prices_for(&state, &id).await;
    Ok(Json(MarketDetailResponse {
        market: market_row_to_dto(row),
        price_early_wad: pe,
        price_late_wad: pl,
    }))
}

pub async fn list_market_trades(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<TradesListQuery>,
) -> Result<Json<MarketTradesResponse>, ApiError> {
    use db::schema::trades::dsl::*;

    let limit = query.limit.unwrap_or(100).clamp(1, 500);
    let mut conn = state
        .pool
        .get()
        .await
        .map_err(|e| ApiError::Internal(format!("pool get: {e}")))?;

    let rows: Vec<TradeRow> = trades
        .filter(market_id.eq(&id))
        .order(filled_at.desc())
        .limit(limit)
        .select(TradeRow::as_select())
        .load(&mut conn)
        .await?;

    Ok(Json(MarketTradesResponse {
        trades: rows.into_iter().map(trade_row_to_dto).collect(),
    }))
}

pub(crate) async fn load_market(state: &AppState, id: &str) -> Result<MarketRow, ApiError> {
    use db::schema::markets::dsl::*;
    let mut conn = state
        .pool
        .get()
        .await
        .map_err(|e| ApiError::Internal(format!("pool get: {e}")))?;
    markets
        .filter(market_id.eq(id))
        .select(MarketRow::as_select())
        .first(&mut conn)
        .await
        .optional()?
        .ok_or_else(|| ApiError::NotFound(format!("market {id} not found")))
}

async fn prices_for(state: &AppState, market_id: &str) -> (String, String) {
    let Some(chain) = state.chain.as_ref() else {
        return (HALF_WAD.into(), HALF_WAD.into());
    };
    let Ok(mid) = parse_market_id(market_id) else {
        return (HALF_WAD.into(), HALF_WAD.into());
    };
    let pe = chain
        .price_early(mid)
        .await
        .unwrap_or_else(|_| HALF_WAD.into());
    let pl = chain
        .price_late(mid)
        .await
        .unwrap_or_else(|_| HALF_WAD.into());
    (pe, pl)
}

fn market_row_to_dto(row: MarketRow) -> Market {
    Market {
        market_id: MarketId(row.market_id),
        dedupe_key: row.dedupe_key,
        trip_id: row.trip_id,
        route_id: row.route_id,
        train_id: row.train_id,
        start_stop_id: row.start_stop_id,
        start_eta_unix: row.start_eta_unix,
        stop_id: row.stop_id,
        frozen_eta_unix: row.frozen_eta_unix,
        liquidity_b: row.liquidity_b,
        q_early: row.q_early,
        q_late: row.q_late,
        outcome: parse_outcome(&row.outcome),
        opened_event_id: row.opened_event_id,
        resolved_event_id: row.resolved_event_id,
        chain_create_tx: row.chain_create_tx,
        chain_resolve_tx: row.chain_resolve_tx,
        opened_at: row.opened_at,
        resolved_at: row.resolved_at,
    }
}

fn parse_outcome(raw: &str) -> MarketOutcome {
    match raw {
        "early" => MarketOutcome::Early,
        "late" => MarketOutcome::Late,
        _ => MarketOutcome::Unresolved,
    }
}

pub(crate) fn trade_row_to_dto(row: TradeRow) -> Trade {
    Trade {
        trade_id: TradeId(row.trade_id),
        event_id: row.event_id,
        market_id: MarketId(row.market_id),
        agent_id: AgentId(row.agent_id),
        side: parse_side(&row.side),
        action: parse_action(&row.action),
        shares: row.shares,
        usdc_amount: row.usdc_amount,
        price_wad: row.price_wad,
        chain_tx_hash: row.chain_tx_hash,
        filled_at: row.filled_at,
    }
}

fn parse_side(raw: &str) -> MarketSide {
    match raw {
        "late" => MarketSide::Late,
        _ => MarketSide::Early,
    }
}

fn parse_action(raw: &str) -> TradeAction {
    match raw {
        "sell" => TradeAction::Sell,
        _ => TradeAction::Buy,
    }
}

pub(crate) fn pnl_row_to_dto(row: AgentMarketPnlRow) -> dto::AgentMarketPnl {
    dto::AgentMarketPnl {
        agent_id: AgentId(row.agent_id),
        market_id: MarketId(row.market_id),
        early_shares: row.early_shares,
        late_shares: row.late_shares,
        cost_basis_usdc: row.cost_basis_usdc,
        realized_pnl_usdc: row.realized_pnl_usdc,
        unrealized_pnl_usdc: row.unrealized_pnl_usdc,
        total_pnl_usdc: row.total_pnl_usdc,
        last_mark_wad: row.last_mark_wad,
        resolved: row.resolved,
        updated_at: row.updated_at,
    }
}
