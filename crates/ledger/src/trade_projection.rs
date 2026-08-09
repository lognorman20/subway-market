//! Project market / trade / PnL facts from bus into Postgres (same tx as `bus_events`).

use chrono::Utc;
use diesel::prelude::*;
use diesel_async::{AsyncPgConnection, RunQueryDsl};
use dto::{
    MarketOpened, MarketOutcome, MarketResolved, MarketSide, TradeAction, TradeFilled,
};
use tracing::{debug, info};

use crate::pnl::{
    apply_buy, apply_resolve, apply_sell, parse_shares, parse_wad, shares_to_string, wad_to_string,
    PnlState, SidePos,
};
use crate::writer::WriterError;

fn projection_err(msg: impl Into<String>) -> WriterError {
    WriterError::Projection(msg.into())
}

/// Upsert market row from `subway.market.opened`.
pub async fn project_market_opened(
    conn: &mut AsyncPgConnection,
    opened: &MarketOpened,
) -> Result<(), WriterError> {
    use db::schema::markets::dsl::*;

    let event_id = opened.meta.event_id.as_ref();
    let mid = opened.market_id.as_ref();

    diesel::insert_into(markets)
        .values((
            market_id.eq(mid),
            dedupe_key.eq(&opened.dedupe_key),
            trip_id.eq(&opened.trip_id),
            route_id.eq(&opened.route_id),
            train_id.eq(opened.train_id.as_deref()),
            stop_id.eq(&opened.stop_id),
            frozen_eta_unix.eq(opened.frozen_eta_unix),
            liquidity_b.eq(&opened.liquidity_b),
            q_early.eq("0"),
            q_late.eq("0"),
            outcome.eq(MarketOutcome::Unresolved.as_str()),
            opened_event_id.eq(Some(event_id)),
            chain_create_tx.eq(Some(opened.chain_tx_hash.as_str())),
            opened_at.eq(opened.opened_at),
        ))
        .on_conflict(market_id)
        .do_update()
        .set((
            dedupe_key.eq(&opened.dedupe_key),
            trip_id.eq(&opened.trip_id),
            route_id.eq(&opened.route_id),
            train_id.eq(opened.train_id.as_deref()),
            stop_id.eq(&opened.stop_id),
            frozen_eta_unix.eq(opened.frozen_eta_unix),
            liquidity_b.eq(&opened.liquidity_b),
            opened_event_id.eq(Some(event_id)),
            chain_create_tx.eq(Some(opened.chain_tx_hash.as_str())),
            opened_at.eq(opened.opened_at),
        ))
        .execute(conn)
        .await?;

    info!(
        market_id = %mid,
        event_id = %event_id,
        "projected market opened"
    );
    Ok(())
}

/// Update market outcome from `subway.market.resolved` and settle agent PnL rows.
pub async fn project_market_resolved(
    conn: &mut AsyncPgConnection,
    resolved: &MarketResolved,
) -> Result<(), WriterError> {
    use db::schema::markets::dsl as m;

    if !resolved.outcome.is_resolvable() {
        return Err(projection_err(format!(
            "market resolved with non-resolvable outcome {:?}",
            resolved.outcome
        )));
    }

    let mid = resolved.market_id.as_ref();
    let event_id = resolved.meta.event_id.as_ref();

    let updated = diesel::update(m::markets.filter(m::market_id.eq(mid)))
        .set((
            m::outcome.eq(resolved.outcome.as_str()),
            m::resolved_event_id.eq(Some(event_id)),
            m::chain_resolve_tx.eq(Some(resolved.chain_tx_hash.as_str())),
            m::resolved_at.eq(Some(resolved.resolved_at)),
        ))
        .execute(conn)
        .await?;

    if updated == 0 {
        return Err(projection_err(format!(
            "market_resolved for unknown market_id {mid}"
        )));
    }

    settle_agents_for_market(conn, mid, resolved.outcome).await?;

    info!(
        market_id = %mid,
        outcome = %resolved.outcome.as_str(),
        event_id = %event_id,
        "projected market resolved"
    );
    Ok(())
}

/// Append trade tape + update positions / agent_market_pnl from `subway.trade.filled`.
pub async fn project_trade_filled(
    conn: &mut AsyncPgConnection,
    fill: &TradeFilled,
) -> Result<(), WriterError> {
    use db::schema::trades::dsl as t;

    let tid = fill.trade_id.as_ref();
    let eid = fill.meta.event_id.as_ref();
    let mid = fill.market_id.as_ref();
    let aid = fill.agent_id.as_ref();

    ensure_market_exists(conn, mid).await?;
    ensure_agent_exists(conn, aid).await?;

    let shares = parse_shares(&fill.shares).map_err(projection_err)?;
    if shares < 0 {
        return Err(projection_err(format!("negative shares on fill: {shares}")));
    }
    let price_wad = parse_wad(&fill.price_wad).map_err(projection_err)?;

    let inserted = diesel::insert_into(t::trades)
        .values((
            t::trade_id.eq(tid),
            t::event_id.eq(eid),
            t::market_id.eq(mid),
            t::agent_id.eq(aid),
            t::side.eq(fill.side.as_str()),
            t::action.eq(fill.action.as_str()),
            t::shares.eq(&fill.shares),
            t::usdc_amount.eq(fill.usdc_amount),
            t::price_wad.eq(&fill.price_wad),
            t::chain_tx_hash.eq(&fill.chain_tx_hash),
            t::filled_at.eq(fill.filled_at),
        ))
        .on_conflict(t::trade_id)
        .do_nothing()
        .execute(conn)
        .await?;

    if inserted == 0 {
        debug!(
            trade_id = %tid,
            event_id = %eid,
            "trade already projected; skipping PnL"
        );
        return Ok(());
    }

    let mut state = load_pnl_state(conn, aid, mid).await?;
    if state.resolved {
        return Err(projection_err(format!(
            "trade fill on resolved market {mid} agent {aid}"
        )));
    }

    match fill.action {
        TradeAction::Buy => {
            apply_buy(&mut state, fill.side, shares, fill.usdc_amount, price_wad);
        }
        TradeAction::Sell => {
            apply_sell(&mut state, fill.side, shares, fill.usdc_amount, price_wad);
        }
    }

    persist_pnl_state(conn, aid, mid, &state).await?;
    adjust_market_q(conn, mid, fill.side, fill.action, shares).await?;

    info!(
        trade_id = %tid,
        market_id = %mid,
        agent_id = %aid,
        side = %fill.side.as_str(),
        action = %fill.action.as_str(),
        realized = state.realized_pnl_usdc,
        unrealized = state.unrealized_pnl_usdc(),
        "projected trade filled"
    );
    Ok(())
}

async fn ensure_market_exists(
    conn: &mut AsyncPgConnection,
    mid: &str,
) -> Result<(), WriterError> {
    use db::schema::markets::dsl::*;

    let exists: bool = diesel::select(diesel::dsl::exists(
        markets.filter(market_id.eq(mid)),
    ))
    .get_result(conn)
    .await?;

    if !exists {
        return Err(projection_err(format!(
            "trade fill for unknown market_id {mid}"
        )));
    }
    Ok(())
}

async fn ensure_agent_exists(conn: &mut AsyncPgConnection, aid: &str) -> Result<(), WriterError> {
    use db::schema::agents::dsl::*;

    let exists: bool =
        diesel::select(diesel::dsl::exists(agents.filter(agent_id.eq(aid))))
            .get_result(conn)
            .await?;

    if !exists {
        return Err(projection_err(format!(
            "trade fill for unknown agent_id {aid} (agents must be inserted before fills)"
        )));
    }
    Ok(())
}

async fn load_pnl_state(
    conn: &mut AsyncPgConnection,
    aid: &str,
    mid: &str,
) -> Result<PnlState, WriterError> {
    use db::schema::{agent_market_pnl, positions};

    let early_row: Option<(String, i64)> = positions::table
        .select((positions::shares, positions::cost_basis_usdc))
        .filter(positions::agent_id.eq(aid))
        .filter(positions::market_id.eq(mid))
        .filter(positions::side.eq(MarketSide::Early.as_str()))
        .first(conn)
        .await
        .optional()?;

    let late_row: Option<(String, i64)> = positions::table
        .select((positions::shares, positions::cost_basis_usdc))
        .filter(positions::agent_id.eq(aid))
        .filter(positions::market_id.eq(mid))
        .filter(positions::side.eq(MarketSide::Late.as_str()))
        .first(conn)
        .await
        .optional()?;

    let pnl_row: Option<(i64, Option<String>, bool)> = agent_market_pnl::table
        .select((
            agent_market_pnl::realized_pnl_usdc,
            agent_market_pnl::last_mark_wad,
            agent_market_pnl::resolved,
        ))
        .filter(agent_market_pnl::agent_id.eq(aid))
        .filter(agent_market_pnl::market_id.eq(mid))
        .first(conn)
        .await
        .optional()?;

    let early = match early_row {
        Some((shares_s, cost)) => SidePos {
            shares: parse_shares(&shares_s).map_err(projection_err)?,
            cost_basis_usdc: cost,
        },
        None => SidePos::default(),
    };
    let late = match late_row {
        Some((shares_s, cost)) => SidePos {
            shares: parse_shares(&shares_s).map_err(projection_err)?,
            cost_basis_usdc: cost,
        },
        None => SidePos::default(),
    };

    let (realized, last_mark_early_wad, resolved) = match pnl_row {
        Some((realized, mark_s, resolved)) => {
            let mark = match mark_s {
                Some(s) => Some(parse_wad(&s).map_err(projection_err)?),
                None => None,
            };
            (realized, mark, resolved)
        }
        None => (0, None, false),
    };

    Ok(PnlState {
        early,
        late,
        realized_pnl_usdc: realized,
        last_mark_early_wad,
        resolved,
    })
}

async fn persist_pnl_state(
    conn: &mut AsyncPgConnection,
    aid: &str,
    mid: &str,
    state: &PnlState,
) -> Result<(), WriterError> {
    use db::schema::{agent_market_pnl, positions};

    let now = Utc::now();

    for (side, pos) in [
        (MarketSide::Early, state.early),
        (MarketSide::Late, state.late),
    ] {
        let shares_s = shares_to_string(pos.shares);
        diesel::insert_into(positions::table)
            .values((
                positions::agent_id.eq(aid),
                positions::market_id.eq(mid),
                positions::side.eq(side.as_str()),
                positions::shares.eq(&shares_s),
                positions::cost_basis_usdc.eq(pos.cost_basis_usdc),
                positions::updated_at.eq(now),
            ))
            .on_conflict((
                positions::agent_id,
                positions::market_id,
                positions::side,
            ))
            .do_update()
            .set((
                positions::shares.eq(&shares_s),
                positions::cost_basis_usdc.eq(pos.cost_basis_usdc),
                positions::updated_at.eq(now),
            ))
            .execute(conn)
            .await?;
    }

    let early_s = shares_to_string(state.early_shares());
    let late_s = shares_to_string(state.late_shares());
    let mark_s = state.last_mark_early_wad.map(wad_to_string);
    let unrealized = state.unrealized_pnl_usdc();
    let total = state.total_pnl_usdc();
    let cost = state.cost_basis_usdc();

    diesel::insert_into(agent_market_pnl::table)
        .values((
            agent_market_pnl::agent_id.eq(aid),
            agent_market_pnl::market_id.eq(mid),
            agent_market_pnl::early_shares.eq(&early_s),
            agent_market_pnl::late_shares.eq(&late_s),
            agent_market_pnl::cost_basis_usdc.eq(cost),
            agent_market_pnl::realized_pnl_usdc.eq(state.realized_pnl_usdc),
            agent_market_pnl::unrealized_pnl_usdc.eq(unrealized),
            agent_market_pnl::total_pnl_usdc.eq(total),
            agent_market_pnl::last_mark_wad.eq(mark_s.as_deref()),
            agent_market_pnl::resolved.eq(state.resolved),
            agent_market_pnl::updated_at.eq(now),
        ))
        .on_conflict((agent_market_pnl::agent_id, agent_market_pnl::market_id))
        .do_update()
        .set((
            agent_market_pnl::early_shares.eq(&early_s),
            agent_market_pnl::late_shares.eq(&late_s),
            agent_market_pnl::cost_basis_usdc.eq(cost),
            agent_market_pnl::realized_pnl_usdc.eq(state.realized_pnl_usdc),
            agent_market_pnl::unrealized_pnl_usdc.eq(unrealized),
            agent_market_pnl::total_pnl_usdc.eq(total),
            agent_market_pnl::last_mark_wad.eq(mark_s.as_deref()),
            agent_market_pnl::resolved.eq(state.resolved),
            agent_market_pnl::updated_at.eq(now),
        ))
        .execute(conn)
        .await?;

    Ok(())
}

async fn settle_agents_for_market(
    conn: &mut AsyncPgConnection,
    mid: &str,
    outcome: MarketOutcome,
) -> Result<(), WriterError> {
    use db::schema::{agent_market_pnl, positions};

    let agent_ids: Vec<String> = agent_market_pnl::table
        .select(agent_market_pnl::agent_id)
        .filter(agent_market_pnl::market_id.eq(mid))
        .filter(agent_market_pnl::resolved.eq(false))
        .load(conn)
        .await?;

    // Also settle agents that only have positions (no pnl row yet).
    let pos_agents: Vec<String> = positions::table
        .select(positions::agent_id)
        .filter(positions::market_id.eq(mid))
        .distinct()
        .load(conn)
        .await?;

    let mut ids = agent_ids;
    for a in pos_agents {
        if !ids.contains(&a) {
            ids.push(a);
        }
    }

    for aid in ids {
        let mut state = load_pnl_state(conn, &aid, mid).await?;
        if state.resolved {
            continue;
        }
        apply_resolve(&mut state, outcome);
        persist_pnl_state(conn, &aid, mid, &state).await?;
    }

    Ok(())
}

async fn adjust_market_q(
    conn: &mut AsyncPgConnection,
    mid: &str,
    side: MarketSide,
    action: TradeAction,
    shares: i64,
) -> Result<(), WriterError> {
    use db::schema::markets::dsl::*;

    let (q_early_s, q_late_s): (String, String) = markets
        .select((q_early, q_late))
        .filter(market_id.eq(mid))
        .first(conn)
        .await?;

    let mut q_e = parse_shares(&q_early_s).map_err(projection_err)?;
    let mut q_l = parse_shares(&q_late_s).map_err(projection_err)?;

    let delta = match action {
        TradeAction::Buy => shares,
        TradeAction::Sell => -shares,
    };
    match side {
        MarketSide::Early => q_e = q_e.saturating_add(delta),
        MarketSide::Late => q_l = q_l.saturating_add(delta),
    }

    diesel::update(markets.filter(market_id.eq(mid)))
        .set((
            q_early.eq(shares_to_string(q_e)),
            q_late.eq(shares_to_string(q_l)),
        ))
        .execute(conn)
        .await?;

    Ok(())
}
