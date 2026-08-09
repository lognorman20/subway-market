//! Market lifecycle + trade bus payloads (domain intents, backend facts).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ids::{AgentId, EventMeta, MarketId, TradeId};
use crate::market::{MarketOutcome, MarketSide, TradeAction};

/// Domain intent: open a market for a frozen train. Subject: `subway.market.open_requested`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketOpenRequested {
    #[serde(flatten)]
    pub meta: EventMeta,
    /// Joins freeze / `tracked_trains.dedupe_key`.
    pub dedupe_key: String,
    pub trip_id: String,
    pub route_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub train_id: Option<String>,
    /// Start stop at freeze (`remaining[0]`). Empty on legacy events.
    #[serde(default)]
    pub start_stop_id: String,
    /// Start-stop ETA unix at freeze. `0` on legacy events.
    #[serde(default)]
    pub start_eta_unix: i64,
    pub stop_id: String,
    pub frozen_eta_unix: i64,
    /// LMSR liquidity `b` in micro-USDC (6 decimals); backend may default when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub liquidity_b: Option<String>,
}

/// Domain intent: resolve an open market. Subject: `subway.market.resolve_requested`.
///
/// Joins via `dedupe_key` (domain never sees on-chain `market_id` — backend maps it).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketResolveRequested {
    #[serde(flatten)]
    pub meta: EventMeta,
    /// Same key as freeze / `MarketOpenRequested` / `MarketOpened`.
    pub dedupe_key: String,
    pub actual_arrival_unix: i64,
    /// Domain decision: early (arrival ≤ frozen_eta) or late (strictly after). Never unresolved.
    pub outcome: MarketOutcome,
}

/// Backend fact: market created on-chain. Subject: `subway.market.opened`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketOpened {
    #[serde(flatten)]
    pub meta: EventMeta,
    pub market_id: MarketId,
    pub dedupe_key: String,
    pub trip_id: String,
    pub route_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub train_id: Option<String>,
    /// Start stop at freeze. Empty on legacy events.
    #[serde(default)]
    pub start_stop_id: String,
    /// Start-stop ETA unix at freeze. `0` on legacy events.
    #[serde(default)]
    pub start_eta_unix: i64,
    pub stop_id: String,
    pub frozen_eta_unix: i64,
    /// LMSR liquidity `b` in micro-USDC (6 decimals, matches on-chain `liquidityB`).
    pub liquidity_b: String,
    pub chain_tx_hash: String,
    pub opened_at: DateTime<Utc>,
}

/// Backend fact: market resolved on-chain. Subject: `subway.market.resolved`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketResolved {
    #[serde(flatten)]
    pub meta: EventMeta,
    pub market_id: MarketId,
    pub outcome: MarketOutcome,
    pub actual_arrival_unix: i64,
    pub chain_tx_hash: String,
    pub resolved_at: DateTime<Utc>,
}

/// Backend fact: buy/sell executed on-chain. Subject: `subway.trade.filled`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TradeFilled {
    #[serde(flatten)]
    pub meta: EventMeta,
    pub trade_id: TradeId,
    pub market_id: MarketId,
    pub agent_id: AgentId,
    pub side: MarketSide,
    pub action: TradeAction,
    /// Share quantity in micro-USDC (6 decimals).
    pub shares: String,
    pub usdc_amount: i64,
    /// Marginal price after fill in WAD (1e18 = 100%).
    pub price_wad: String,
    pub chain_tx_hash: String,
    pub filled_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::BusEvent;

    #[test]
    fn trade_filled_bus_round_trip() {
        let fill = TradeFilled {
            meta: EventMeta::new(),
            trade_id: TradeId::new(),
            market_id: MarketId::from_on_chain(42),
            agent_id: AgentId::new(),
            side: MarketSide::Early,
            action: TradeAction::Buy,
            shares: "1000000".into(),
            usdc_amount: 500_000,
            price_wad: "500000000000000000".into(),
            chain_tx_hash: "0xabc".into(),
            filled_at: Utc::now(),
        };
        let bus = BusEvent::TradeFilled(fill);
        let json = serde_json::to_string(&bus).unwrap();
        let back: BusEvent = serde_json::from_str(&json).unwrap();
        match back {
            BusEvent::TradeFilled(f) => assert_eq!(f.usdc_amount, 500_000),
            _ => panic!("expected trade_filled"),
        }
    }

    #[test]
    fn market_open_requested_round_trip() {
        let req = MarketOpenRequested {
            meta: EventMeta::new(),
            dedupe_key: "t1:A99:20260725".into(),
            trip_id: "t1".into(),
            route_id: "A".into(),
            train_id: Some("A1234".into()),
            start_stop_id: "A01".into(),
            start_eta_unix: 1_699_999_900,
            stop_id: "A99".into(),
            frozen_eta_unix: 1_700_000_000,
            liquidity_b: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        let back: MarketOpenRequested = serde_json::from_str(&json).unwrap();
        assert_eq!(back.dedupe_key, "t1:A99:20260725");
        assert!(!back.meta.event_id.0.is_empty());
    }

    #[test]
    fn market_resolve_requested_round_trip() {
        let req = MarketResolveRequested {
            meta: EventMeta::new(),
            dedupe_key: "t1:A99:20260725".into(),
            actual_arrival_unix: 1_700_000_000,
            outcome: MarketOutcome::Early,
        };
        let json = serde_json::to_string(&req).unwrap();
        let back: MarketResolveRequested = serde_json::from_str(&json).unwrap();
        assert_eq!(back.dedupe_key, "t1:A99:20260725");
        assert_eq!(back.outcome, MarketOutcome::Early);
    }
}
