use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::events::TrainPhase;
use crate::ids::AgentId;
use crate::market::{AgentMarketPnl, Market, MarketSide, Trade, TradeAction};

/// Placeholder HTTP API request/response types (backend lane extends these).

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HealthRequest;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
}

impl HealthResponse {
    pub fn ok() -> Self {
        Self {
            status: "ok".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketsListRequest;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketsListResponse {
    pub markets: Vec<MarketSummary>,
}

impl Default for MarketsListResponse {
    fn default() -> Self {
        Self {
            markets: Vec::new(),
        }
    }
}

/// Lightweight market row for list endpoints.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketSummary {
    pub market_id: String,
    pub route_id: String,
    pub trip_id: String,
    pub stop_id: String,
    pub outcome: String,
    /// Marginal Early price in WAD (1e18 = 100%; matches on-chain `priceEarly`).
    pub price_early_wad: String,
    /// Marginal Late price in WAD (1e18 = 100%; matches on-chain `priceLate`).
    pub price_late_wad: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketDetailRequest {
    pub market_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketDetailResponse {
    pub market: Market,
    pub price_early_wad: String,
    pub price_late_wad: String,
}

/// `POST /agents` — human session creates agent runtime credentials.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateAgentRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateAgentResponse {
    pub agent_id: AgentId,
    /// Plaintext key — returned once at creation only.
    pub api_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentsListResponse {
    pub agents: Vec<AgentSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentSummary {
    pub agent_id: AgentId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuyRequest {
    pub market_id: String,
    pub side: MarketSide,
    /// Share quantity in micro-USDC (6 decimals, matches on-chain `buy`/`sell` args).
    pub shares: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SellRequest {
    pub market_id: String,
    pub side: MarketSide,
    /// Share quantity in micro-USDC (6 decimals).
    pub shares: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TradeResponse {
    pub trade: Trade,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentPnlResponse {
    pub rows: Vec<AgentMarketPnl>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BankrollResponse {
    /// Spendable USDC in micro-units (6 decimals).
    pub available_usdc: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketTradesResponse {
    pub trades: Vec<Trade>,
}

/// Agent-facing trade tape filter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TradesListQuery {
    pub market_id: Option<String>,
    pub limit: Option<i64>,
}

/// Documented in `llms.txt` — agent HTTP error envelope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiErrorResponse {
    pub error: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

/// Buy/sell action for shared route handlers (human session or agent Bearer).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrderAction {
    Buy,
    Sell,
}

impl From<TradeAction> for OrderAction {
    fn from(value: TradeAction) -> Self {
        match value {
            TradeAction::Buy => Self::Buy,
            TradeAction::Sell => Self::Sell,
        }
    }
}

/// `GET /trains` query params.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrainsListQuery {
    /// Max rows (default 100, capped at 500).
    pub limit: Option<i64>,
}

/// One row from ledger `tracked_trains` projection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrackedTrain {
    pub dedupe_key: String,
    pub updated_at: DateTime<Utc>,
    pub phase: TrainPhase,
    pub route_id: String,
    pub trip_id: String,
    pub nyct_train_id: Option<String>,
    pub first_seen_at: Option<DateTime<Utc>>,
    pub start_stop_id: Option<String>,
    pub target_stop_id: Option<String>,
    pub frozen_at: Option<DateTime<Utc>>,
    pub expected_arrival_unix: Option<i64>,
    pub remaining_stops: Option<i32>,
    pub arrival_unix: Option<i64>,
    pub diff_seconds: Option<i64>,
    pub actual_source: Option<String>,
    pub action: Option<String>,
    pub side: Option<String>,
    pub reason: Option<String>,
    pub diff_id: Option<String>,
    pub freeze_event_id: Option<String>,
    pub diff_event_id: Option<String>,
}

/// `GET /trains` response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrainsListResponse {
    pub trains: Vec<TrackedTrain>,
}

impl Default for TrainsListResponse {
    fn default() -> Self {
        Self {
            trains: Vec::new(),
        }
    }
}
