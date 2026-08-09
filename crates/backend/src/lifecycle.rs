//! Market lifecycle — domain intents → chain create/resolve → bus facts.
//!
//! Consumes `subway.market.open_requested` / `subway.market.resolve_requested`.
//! On chain success emits `subway.market.opened` / `subway.market.resolved`
//! with the same mandatory-emit retry pattern as trade fills (decision #10).

use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use diesel::prelude::*;
use diesel_async::pooled_connection::deadpool::Pool;
use diesel_async::{AsyncPgConnection, RunQueryDsl};
use dto::{
    BusEvent, EventMeta, MarketId, MarketOpenRequested, MarketOpened, MarketOutcome,
    MarketResolveRequested, MarketResolved, MARKET_OPENED, MARKET_OPEN_REQUESTED,
    MARKET_RESOLVED, MARKET_RESOLVE_REQUESTED,
};
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};
use transport::{EventHandler, TransportClient, TransportError};

use crate::trading_chain::TradingChain;

/// Default LMSR `b` when domain omits liquidity (100 USDC micro).
pub const DEFAULT_LIQUIDITY_B_MICRO_USDC: &str = "100000000";

/// Bounded emit retries after chain success.
const EMIT_ATTEMPTS: u32 = 5;
const EMIT_BACKOFF_BASE_MS: u64 = 50;
const EMIT_BACKOFF_CAP_MS: u64 = 800;

/// Half WAD — fresh LMSR (q=0,q=0) prices both sides at 0.5.
pub const HALF_WAD: &str = "500000000000000000";

/// Bus subjects this handler consumes.
pub const LIFECYCLE_SUBJECTS: &[&str] = &[MARKET_OPEN_REQUESTED, MARKET_RESOLVE_REQUESTED];

/// In-memory dedupe_key → on-chain market id (populated on create).
pub type MarketRegistry = Arc<Mutex<HashMap<String, u64>>>;

pub fn new_registry() -> MarketRegistry {
    Arc::new(Mutex::new(HashMap::new()))
}

pub struct LifecycleHandler {
    pub chain: Arc<dyn TradingChain>,
    /// Serialize on-chain txs (one platform signer; open+resolve run on parallel NATS tasks).
    chain_lock: Arc<Mutex<()>>,
    /// When set, skip open if any unresolved market exists (one market at a time).
    pub pool: Option<Pool<AsyncPgConnection>>,
    pub nats: Option<TransportClient>,
    pub registry: MarketRegistry,
    /// Test-only: capture emitted facts instead of NATS.
    #[doc(hidden)]
    pub emit_recorder: Option<Arc<Mutex<Vec<(String, BusEvent)>>>>,
}

impl LifecycleHandler {
    pub fn new(
        chain: Arc<dyn TradingChain>,
        nats: TransportClient,
        pool: Option<Pool<AsyncPgConnection>>,
    ) -> Self {
        Self {
            chain,
            chain_lock: Arc::new(Mutex::new(())),
            pool,
            nats: Some(nats),
            registry: new_registry(),
            emit_recorder: None,
        }
    }

    /// Process one lifecycle intent (shared by handler + tests).
    pub async fn process(&self, subject: &str, event: BusEvent) -> Result<(), TransportError> {
        match event {
            BusEvent::MarketOpenRequested(req) => self.handle_open(req).await,
            BusEvent::MarketResolveRequested(req) => self.handle_resolve(req).await,
            other => {
                debug!(
                    subject,
                    event_type = other.event_type_name(),
                    "lifecycle ignored non-intent event"
                );
                Ok(())
            }
        }
    }

    async fn handle_open(&self, req: MarketOpenRequested) -> Result<(), TransportError> {
        if let Some(pool) = &self.pool {
            let unresolved = count_unresolved_markets(pool).await?;
            if unresolved > 0 {
                warn!(
                    dedupe_key = %req.dedupe_key,
                    unresolved_count = unresolved,
                    "skipping market open — unresolved market already active"
                );
                return Ok(());
            }
        }

        let liquidity = req
            .liquidity_b
            .as_deref()
            .unwrap_or(DEFAULT_LIQUIDITY_B_MICRO_USDC);
        let liquidity_u128 = u128::from_str(liquidity).map_err(|e| {
            TransportError::Message(format!("bad liquidity_b {liquidity}: {e}"))
        })?;

        let train_id = req
            .train_id
            .clone()
            .unwrap_or_else(|| req.trip_id.clone());

        let _chain_guard = self.chain_lock.lock().await;
        let (market_id, tx_hash) = self
            .chain
            .create_market(
                &train_id,
                &req.trip_id,
                &req.stop_id,
                req.frozen_eta_unix,
                liquidity_u128,
            )
            .await
            .map_err(|e| TransportError::Message(e.message().to_string()))?;

        // Fresh market should be ~50/50; log when chain reports otherwise (q=0 both sides).
        if let (Ok(early), Ok(late)) = (
            self.chain.price_early(market_id).await,
            self.chain.price_late(market_id).await,
        ) {
            if early != HALF_WAD || late != HALF_WAD {
                warn!(
                    market_id,
                    price_early = %early,
                    price_late = %late,
                    "new market prices not 50/50"
                );
            }
        }

        {
            let mut reg = self.registry.lock().await;
            reg.insert(req.dedupe_key.clone(), market_id);
        }

        let opened = MarketOpened {
            meta: EventMeta::new(),
            market_id: MarketId::from_on_chain(market_id),
            dedupe_key: req.dedupe_key.clone(),
            trip_id: req.trip_id.clone(),
            route_id: req.route_id.clone(),
            train_id: req.train_id.clone(),
            start_stop_id: req.start_stop_id.clone(),
            start_eta_unix: req.start_eta_unix,
            stop_id: req.stop_id.clone(),
            frozen_eta_unix: req.frozen_eta_unix,
            liquidity_b: liquidity.to_string(),
            chain_tx_hash: tx_hash,
            opened_at: Utc::now(),
        };

        self.emit_fact(MARKET_OPENED, BusEvent::MarketOpened(opened.clone()))
            .await?;

        info!(
            market_id,
            dedupe_key = %req.dedupe_key,
            stop_id = %req.stop_id,
            "market opened"
        );
        Ok(())
    }

    async fn handle_resolve(&self, req: MarketResolveRequested) -> Result<(), TransportError> {
        if !matches!(req.outcome, MarketOutcome::Early | MarketOutcome::Late) {
            return Err(TransportError::Message(format!(
                "resolve outcome must be early or late, got {}",
                req.outcome.as_str()
            )));
        }

        let market_id = {
            let reg = self.registry.lock().await;
            reg.get(&req.dedupe_key).copied()
        };
        let Some(market_id) = market_id else {
            return Err(TransportError::Message(format!(
                "no market_id for dedupe_key {}",
                req.dedupe_key
            )));
        };

        let _chain_guard = self.chain_lock.lock().await;
        let tx_hash = self
            .chain
            .resolve_market(market_id, req.outcome)
            .await
            .map_err(|e| TransportError::Message(e.message().to_string()))?;

        let resolved = MarketResolved {
            meta: EventMeta::new(),
            market_id: MarketId::from_on_chain(market_id),
            outcome: req.outcome,
            actual_arrival_unix: req.actual_arrival_unix,
            chain_tx_hash: tx_hash,
            resolved_at: Utc::now(),
        };

        self.emit_fact(MARKET_RESOLVED, BusEvent::MarketResolved(resolved))
            .await?;

        info!(
            market_id,
            dedupe_key = %req.dedupe_key,
            outcome = %req.outcome.as_str(),
            "market resolved"
        );
        Ok(())
    }

    async fn emit_fact(&self, subject: &str, event: BusEvent) -> Result<(), TransportError> {
        if let Some(rec) = &self.emit_recorder {
            rec.lock().await.push((subject.to_string(), event));
            return Ok(());
        }

        let nats = self.nats.as_ref().ok_or_else(|| {
            TransportError::Message("nats unavailable — cannot emit market fact".into())
        })?;

        emit_with_retry(nats, subject, &event).await
    }
}

#[async_trait]
impl EventHandler for LifecycleHandler {
    async fn handle(&self, subject: &str, event: BusEvent) -> Result<(), TransportError> {
        self.process(subject, event).await
    }
}

async fn count_unresolved_markets(pool: &Pool<AsyncPgConnection>) -> Result<i64, TransportError> {
    use db::schema::markets::dsl::*;

    let mut conn = pool
        .get()
        .await
        .map_err(|e| TransportError::Message(format!("pool get: {e}")))?;
    markets
        .filter(outcome.eq("unresolved"))
        .count()
        .get_result(&mut conn)
        .await
        .map_err(|e| TransportError::Message(format!("count unresolved markets: {e}")))
}

async fn emit_with_retry(
    nats: &TransportClient,
    subject: &str,
    event: &BusEvent,
) -> Result<(), TransportError> {
    let mut last_err = String::new();

    for attempt in 0..EMIT_ATTEMPTS {
        match nats.emit(subject, event).await {
            Ok(()) => return Ok(()),
            Err(err) => {
                last_err = err.to_string();
                warn!(
                    attempt = attempt + 1,
                    max = EMIT_ATTEMPTS,
                    subject,
                    error = %last_err,
                    "market fact emit failed — retrying"
                );
                if attempt + 1 < EMIT_ATTEMPTS {
                    let backoff_ms = (EMIT_BACKOFF_BASE_MS << attempt).min(EMIT_BACKOFF_CAP_MS);
                    tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
                }
            }
        }
    }

    let payload = serde_json::to_string(event).unwrap_or_else(|_| "<serialize failed>".into());
    error!(
        error = %last_err,
        attempts = EMIT_ATTEMPTS,
        subject,
        event_payload = %payload,
        "CRITICAL: market fact emit exhausted after chain success — ledger will miss event; replay payload"
    );
    Err(TransportError::Message(format!(
        "emit {subject} failed after {EMIT_ATTEMPTS} attempts: {last_err}"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trading_chain::MockTradingChain;
    use dto::EventMeta;

    fn open_req(dedupe: &str) -> MarketOpenRequested {
        MarketOpenRequested {
            meta: EventMeta::new(),
            dedupe_key: dedupe.into(),
            trip_id: "t1".into(),
            route_id: "A".into(),
            train_id: Some("A1234".into()),
            start_stop_id: "A01".into(),
            start_eta_unix: 1_700_000_000,
            stop_id: "A99".into(),
            frozen_eta_unix: 1_700_000_100,
            liquidity_b: Some(DEFAULT_LIQUIDITY_B_MICRO_USDC.into()),
        }
    }

    fn resolve_req(dedupe: &str, outcome: MarketOutcome, actual: i64) -> MarketResolveRequested {
        MarketResolveRequested {
            meta: EventMeta::new(),
            dedupe_key: dedupe.into(),
            actual_arrival_unix: actual,
            outcome,
        }
    }

    #[tokio::test]
    async fn open_intent_creates_market_and_emits_opened() {
        let chain = Arc::new(MockTradingChain::balanced());
        let recorder = Arc::new(Mutex::new(Vec::new()));
        let handler = LifecycleHandler {
            chain: chain.clone(),
            chain_lock: Arc::new(Mutex::new(())),
            pool: None,
            nats: None,
            registry: new_registry(),
            emit_recorder: Some(recorder.clone()),
        };

        handler
            .process(
                MARKET_OPEN_REQUESTED,
                BusEvent::MarketOpenRequested(open_req("t1:A99:d")),
            )
            .await
            .unwrap();

        assert_eq!(chain.create_call_count(), 1);
        assert_eq!(
            *handler.registry.lock().await.get("t1:A99:d").unwrap(),
            1
        );

        let events = recorder.lock().await;
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].0, MARKET_OPENED);
        match &events[0].1 {
            BusEvent::MarketOpened(o) => {
                assert_eq!(o.market_id.0, "1");
                assert_eq!(o.dedupe_key, "t1:A99:d");
                assert_eq!(o.liquidity_b, DEFAULT_LIQUIDITY_B_MICRO_USDC);
            }
            other => panic!("expected MarketOpened, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn resolve_intent_resolves_and_emits_resolved() {
        let chain = Arc::new(MockTradingChain::balanced());
        let recorder = Arc::new(Mutex::new(Vec::new()));
        let registry = new_registry();
        registry.lock().await.insert("t1:A99:d".into(), 7);

        let handler = LifecycleHandler {
            chain: chain.clone(),
            chain_lock: Arc::new(Mutex::new(())),
            pool: None,
            nats: None,
            registry,
            emit_recorder: Some(recorder.clone()),
        };

        handler
            .process(
                MARKET_RESOLVE_REQUESTED,
                BusEvent::MarketResolveRequested(resolve_req(
                    "t1:A99:d",
                    MarketOutcome::Early,
                    1_700_000_100,
                )),
            )
            .await
            .unwrap();

        assert_eq!(chain.resolve_call_count(), 1);
        let events = recorder.lock().await;
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].0, MARKET_RESOLVED);
        match &events[0].1 {
            BusEvent::MarketResolved(r) => {
                assert_eq!(r.market_id.0, "7");
                assert_eq!(r.outcome, MarketOutcome::Early);
                assert_eq!(r.actual_arrival_unix, 1_700_000_100);
            }
            other => panic!("expected MarketResolved, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn open_then_resolve_end_to_end_via_handler() {
        let chain = Arc::new(MockTradingChain::balanced());
        let recorder = Arc::new(Mutex::new(Vec::new()));
        let handler = LifecycleHandler {
            chain: chain.clone(),
            chain_lock: Arc::new(Mutex::new(())),
            pool: None,
            nats: None,
            registry: new_registry(),
            emit_recorder: Some(recorder.clone()),
        };

        handler
            .process(
                MARKET_OPEN_REQUESTED,
                BusEvent::MarketOpenRequested(open_req("dk")),
            )
            .await
            .unwrap();
        handler
            .process(
                MARKET_RESOLVE_REQUESTED,
                BusEvent::MarketResolveRequested(resolve_req(
                    "dk",
                    MarketOutcome::Late,
                    1_700_000_200,
                )),
            )
            .await
            .unwrap();

        assert_eq!(chain.create_call_count(), 1);
        assert_eq!(chain.resolve_call_count(), 1);
        let events = recorder.lock().await;
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].0, MARKET_OPENED);
        assert_eq!(events[1].0, MARKET_RESOLVED);
    }
}
