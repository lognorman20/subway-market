//! Diesel row types matching `dto` trading shapes.

use chrono::{DateTime, Utc};
use diesel::prelude::*;
use serde::{Deserialize, Serialize};

use crate::schema::{agent_market_pnl, agents, markets, positions, trades};

/// `agents` table row.
#[derive(Debug, Clone, Queryable, Selectable, Serialize, Deserialize)]
#[diesel(table_name = agents)]
pub struct AgentRow {
    pub agent_id: String,
    pub owner_wallet: String,
    pub display_name: Option<String>,
    pub api_key_hash: String,
    pub created_at: DateTime<Utc>,
    /// Spendable bankroll in micro-USDC (6 decimals). Backend funding/identity write.
    pub bankroll_usdc: i64,
}

/// `markets` table row.
#[derive(Debug, Clone, Queryable, Selectable, Serialize, Deserialize)]
#[diesel(table_name = markets)]
pub struct MarketRow {
    pub market_id: String,
    pub dedupe_key: String,
    pub trip_id: String,
    pub route_id: String,
    pub train_id: Option<String>,
    pub stop_id: String,
    pub frozen_eta_unix: i64,
    pub liquidity_b: String,
    pub q_early: String,
    pub q_late: String,
    pub outcome: String,
    pub opened_event_id: Option<String>,
    pub resolved_event_id: Option<String>,
    pub chain_create_tx: Option<String>,
    pub chain_resolve_tx: Option<String>,
    pub opened_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub start_stop_id: String,
    pub start_eta_unix: i64,
}

/// `trades` table row.
#[derive(Debug, Clone, Queryable, Selectable, Serialize, Deserialize)]
#[diesel(table_name = trades)]
pub struct TradeRow {
    pub trade_id: String,
    pub event_id: String,
    pub market_id: String,
    pub agent_id: String,
    pub side: String,
    pub action: String,
    pub shares: String,
    pub usdc_amount: i64,
    pub price_wad: String,
    pub chain_tx_hash: String,
    pub filled_at: DateTime<Utc>,
}

/// `positions` table row (average-cost basis per side).
#[derive(Debug, Clone, Queryable, Selectable, Serialize, Deserialize)]
#[diesel(table_name = positions)]
pub struct PositionRow {
    pub agent_id: String,
    pub market_id: String,
    pub side: String,
    pub shares: String,
    pub cost_basis_usdc: i64,
    pub updated_at: DateTime<Utc>,
}

/// `agent_market_pnl` ledger projection row.
#[derive(Debug, Clone, Queryable, Selectable, Serialize, Deserialize)]
#[diesel(table_name = agent_market_pnl)]
pub struct AgentMarketPnlRow {
    pub agent_id: String,
    pub market_id: String,
    pub early_shares: String,
    pub late_shares: String,
    pub cost_basis_usdc: i64,
    pub realized_pnl_usdc: i64,
    pub unrealized_pnl_usdc: i64,
    pub total_pnl_usdc: i64,
    pub last_mark_wad: Option<String>,
    pub resolved: bool,
    pub updated_at: DateTime<Utc>,
}

/// Insert new agent (backend funding/identity; not a trade projection).
#[derive(Debug, Clone, Insertable, Serialize, Deserialize)]
#[diesel(table_name = agents)]
pub struct NewAgentRow {
    pub agent_id: String,
    pub owner_wallet: String,
    pub display_name: Option<String>,
    pub api_key_hash: String,
    pub created_at: DateTime<Utc>,
    pub bankroll_usdc: i64,
}

/// Insert new market projection.
#[derive(Debug, Clone, Insertable, Serialize, Deserialize)]
#[diesel(table_name = markets)]
pub struct NewMarketRow {
    pub market_id: String,
    pub dedupe_key: String,
    pub trip_id: String,
    pub route_id: String,
    pub train_id: Option<String>,
    pub stop_id: String,
    pub frozen_eta_unix: i64,
    pub liquidity_b: String,
    pub q_early: String,
    pub q_late: String,
    pub outcome: String,
    pub opened_event_id: Option<String>,
    pub resolved_event_id: Option<String>,
    pub chain_create_tx: Option<String>,
    pub chain_resolve_tx: Option<String>,
    pub opened_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub start_stop_id: String,
    pub start_eta_unix: i64,
}

/// Insert trade tape row.
#[derive(Debug, Clone, Insertable, Serialize, Deserialize)]
#[diesel(table_name = trades)]
pub struct NewTradeRow {
    pub trade_id: String,
    pub event_id: String,
    pub market_id: String,
    pub agent_id: String,
    pub side: String,
    pub action: String,
    pub shares: String,
    pub usdc_amount: i64,
    pub price_wad: String,
    pub chain_tx_hash: String,
    pub filled_at: DateTime<Utc>,
}

/// Upsert position row.
#[derive(Debug, Clone, Insertable, AsChangeset, Serialize, Deserialize)]
#[diesel(table_name = positions)]
pub struct PositionUpsert {
    pub agent_id: String,
    pub market_id: String,
    pub side: String,
    pub shares: String,
    pub cost_basis_usdc: i64,
    pub updated_at: DateTime<Utc>,
}

/// Upsert agent×market PnL projection.
#[derive(Debug, Clone, Insertable, AsChangeset, Serialize, Deserialize)]
#[diesel(table_name = agent_market_pnl)]
pub struct AgentMarketPnlUpsert {
    pub agent_id: String,
    pub market_id: String,
    pub early_shares: String,
    pub late_shares: String,
    pub cost_basis_usdc: i64,
    pub realized_pnl_usdc: i64,
    pub unrealized_pnl_usdc: i64,
    pub total_pnl_usdc: i64,
    pub last_mark_wad: Option<String>,
    pub resolved: bool,
    pub updated_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_market_pnl_row_matches_dto_field_names() {
        let row = AgentMarketPnlRow {
            agent_id: "a1".into(),
            market_id: "1".into(),
            early_shares: "0".into(),
            late_shares: "0".into(),
            cost_basis_usdc: 0,
            realized_pnl_usdc: 0,
            unrealized_pnl_usdc: 0,
            total_pnl_usdc: 0,
            last_mark_wad: None,
            resolved: false,
            updated_at: Utc::now(),
        };
        let json = serde_json::to_value(&row).unwrap();
        assert!(json.get("cost_basis_usdc").is_some());
        assert!(json.get("unrealized_pnl_usdc").is_some());
        assert!(json.get("resolved").is_some());
    }
}
