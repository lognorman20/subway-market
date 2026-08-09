//! Read-only Postgres queries for the observe TUI.

use db::models::{AgentMarketPnlRow, MarketRow, TradeRow};
use db::queries::{QueryError, TrackedTrainRow};
use db::schema::{agent_market_pnl, markets, tracked_trains, trades};
use diesel::prelude::*;
use diesel_async::{AsyncPgConnection, RunQueryDsl};

const UPCOMING_PHASES: &[&str] = &["watching", "frozen"];
const HISTORY_PHASES: &[&str] = &["arrived", "acted"];

/// Trains still being watched or frozen (not yet scored).
pub async fn load_upcoming(
    conn: &mut AsyncPgConnection,
    limit: i64,
) -> Result<Vec<TrackedTrainRow>, QueryError> {
    tracked_trains::table
        .filter(tracked_trains::phase.eq_any(UPCOMING_PHASES))
        .order(tracked_trains::updated_at.desc())
        .limit(limit)
        .select(TrackedTrainRow::as_select())
        .load(conn)
        .await
        .map_err(Into::into)
}

/// Scored or terminal rows — phase `arrived`/`acted` or any row with `diff_seconds`.
pub async fn load_history(
    conn: &mut AsyncPgConnection,
    limit: i64,
) -> Result<Vec<TrackedTrainRow>, QueryError> {
    tracked_trains::table
        .filter(
            tracked_trains::phase
                .eq_any(HISTORY_PHASES)
                .or(tracked_trains::diff_seconds.is_not_null()),
        )
        .order(tracked_trains::updated_at.desc())
        .limit(limit)
        .select(TrackedTrainRow::as_select())
        .load(conn)
        .await
        .map_err(Into::into)
}

/// Recent trade tape, newest fills first.
pub async fn load_trades(
    conn: &mut AsyncPgConnection,
    limit: i64,
) -> Result<Vec<TradeRow>, QueryError> {
    trades::table
        .order(trades::filled_at.desc())
        .limit(limit)
        .select(TradeRow::as_select())
        .load(conn)
        .await
        .map_err(Into::into)
}

/// Markets, newest opens first.
pub async fn load_markets(
    conn: &mut AsyncPgConnection,
    limit: i64,
) -> Result<Vec<MarketRow>, QueryError> {
    markets::table
        .order(markets::opened_at.desc())
        .limit(limit)
        .select(MarketRow::as_select())
        .load(conn)
        .await
        .map_err(Into::into)
}

/// Agent × market PnL projections, most recently updated first.
pub async fn load_agent_market_pnl(
    conn: &mut AsyncPgConnection,
    limit: i64,
) -> Result<Vec<AgentMarketPnlRow>, QueryError> {
    agent_market_pnl::table
        .order(agent_market_pnl::updated_at.desc())
        .limit(limit)
        .select(AgentMarketPnlRow::as_select())
        .load(conn)
        .await
        .map_err(Into::into)
}
