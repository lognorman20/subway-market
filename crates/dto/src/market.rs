//! Trading domain types — agents, markets, trades, positions, PnL projections.
//!
//! **Units:** share quantities, `liquidity_b`, and `q_*` match on-chain USDC 6-decimal
//! (micro-USDC) scale. Marginal prices (`price_wad`, `last_mark_wad`) are WAD (1e18 = 100%).
//! Enums mirror `chain::Side` / `chain::Outcome` wire values without depending on `chain`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ids::{AgentId, MarketId, TradeId};

/// Binary LMSR side — matches on-chain `SubwayMarket.Side` (`Early = 0`, `Late = 1`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarketSide {
    Early,
    Late,
}

impl MarketSide {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Early => "early",
            Self::Late => "late",
        }
    }

    pub const fn as_u8(self) -> u8 {
        match self {
            Self::Early => 0,
            Self::Late => 1,
        }
    }

    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Early),
            1 => Some(Self::Late),
            _ => None,
        }
    }
}

/// Market resolution — matches on-chain `SubwayMarket.Outcome`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarketOutcome {
    Unresolved,
    Early,
    Late,
}

impl MarketOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unresolved => "unresolved",
            Self::Early => "early",
            Self::Late => "late",
        }
    }

    pub const fn as_u8(self) -> u8 {
        match self {
            Self::Unresolved => 0,
            Self::Early => 1,
            Self::Late => 2,
        }
    }

    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Unresolved),
            1 => Some(Self::Early),
            2 => Some(Self::Late),
            _ => None,
        }
    }

    pub const fn is_resolvable(self) -> bool {
        matches!(self, Self::Early | Self::Late)
    }
}

/// Buy or sell fill.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TradeAction {
    Buy,
    Sell,
}

impl TradeAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Buy => "buy",
            Self::Sell => "sell",
        }
    }
}

/// Agent row — `api_key_hash` only (never plaintext key on bus/DB reads).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Agent {
    pub agent_id: AgentId,
    pub owner_wallet: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    pub api_key_hash: String,
    pub created_at: DateTime<Utc>,
}

/// Open or resolved LMSR market projection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Market {
    pub market_id: MarketId,
    pub dedupe_key: String,
    pub trip_id: String,
    pub route_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub train_id: Option<String>,
    pub stop_id: String,
    pub frozen_eta_unix: i64,
    /// LMSR liquidity `b` in micro-USDC (6 decimals, matches on-chain `liquidityB`).
    pub liquidity_b: String,
    /// Outstanding Early share quantity in micro-USDC (matches on-chain `qEarly`).
    pub q_early: String,
    /// Outstanding Late share quantity in micro-USDC (matches on-chain `qLate`).
    pub q_late: String,
    pub outcome: MarketOutcome,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opened_event_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_event_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chain_create_tx: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chain_resolve_tx: Option<String>,
    pub opened_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_at: Option<DateTime<Utc>>,
}

/// One buy/sell fill (ledger tape row).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Trade {
    pub trade_id: TradeId,
    pub event_id: String,
    pub market_id: MarketId,
    pub agent_id: AgentId,
    pub side: MarketSide,
    pub action: TradeAction,
    /// Share quantity in micro-USDC (6 decimals, matches on-chain balance units).
    pub shares: String,
    /// USDC amount in micro-units (6 decimals).
    pub usdc_amount: i64,
    /// Marginal price after fill in WAD (1e18 = 100%; matches `priceEarly`/`priceLate`).
    pub price_wad: String,
    pub chain_tx_hash: String,
    pub filled_at: DateTime<Utc>,
}

/// Per-agent × market × side share ledger (average-cost basis).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Position {
    pub agent_id: AgentId,
    pub market_id: MarketId,
    pub side: MarketSide,
    /// Open share balance in micro-USDC (6 decimals).
    pub shares: String,
    /// Net USDC paid for open shares (micro-units, average-cost).
    pub cost_basis_usdc: i64,
    pub updated_at: DateTime<Utc>,
}

/// Ledger-owned PnL projection per agent × market (table `agent_market_pnl`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentMarketPnl {
    pub agent_id: AgentId,
    pub market_id: MarketId,
    /// Early-side share balance in micro-USDC (6 decimals).
    pub early_shares: String,
    /// Late-side share balance in micro-USDC (6 decimals).
    pub late_shares: String,
    pub cost_basis_usdc: i64,
    pub realized_pnl_usdc: i64,
    pub unrealized_pnl_usdc: i64,
    pub total_pnl_usdc: i64,
    /// Last mark price in WAD (1e18 = 100%; LMSR mid or last trade).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_mark_wad: Option<String>,
    pub resolved: bool,
    pub updated_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn market_side_matches_chain_order() {
        assert_eq!(MarketSide::Early.as_u8(), 0);
        assert_eq!(MarketSide::Late.as_u8(), 1);
        assert_eq!(MarketSide::from_u8(2), None);
    }

    #[test]
    fn market_outcome_matches_chain_order() {
        assert_eq!(MarketOutcome::Unresolved.as_u8(), 0);
        assert_eq!(MarketOutcome::Early.as_u8(), 1);
        assert!(MarketOutcome::Early.is_resolvable());
        assert!(!MarketOutcome::Unresolved.is_resolvable());
    }

    #[test]
    fn agent_market_pnl_round_trip() {
        let pnl = AgentMarketPnl {
            agent_id: AgentId::new(),
            market_id: MarketId::from_on_chain(1),
            early_shares: "1000000".into(),
            late_shares: "0".into(),
            cost_basis_usdc: 500_000,
            realized_pnl_usdc: 0,
            unrealized_pnl_usdc: 50_000,
            total_pnl_usdc: 50_000,
            last_mark_wad: Some("500000000000000000".into()),
            resolved: false,
            updated_at: Utc::now(),
        };
        let json = serde_json::to_string(&pnl).unwrap();
        let back: AgentMarketPnl = serde_json::from_str(&json).unwrap();
        assert_eq!(back.cost_basis_usdc, 500_000);
        assert!(!back.resolved);
    }
}
