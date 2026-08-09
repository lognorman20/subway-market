//! Ledger EventHandler.

use std::sync::Arc;

use async_trait::async_trait;
use dto::BusEvent;
use tracing::{debug, error};
use transport::{EventHandler, TransportError};

use crate::writer::DbWriter;

pub struct LedgerHandler {
    pub writer: Arc<DbWriter>,
}

#[async_trait]
impl EventHandler for LedgerHandler {
    async fn handle(&self, subject: &str, event: BusEvent) -> Result<(), TransportError> {
        debug!(
            subject,
            event_id = %event.event_id().as_ref(),
            event_type = event.event_type_name(),
            "persist"
        );
        self.writer
            .insert(subject, &event)
            .await
            .map_err(|e| {
                error!(
                    error = %e,
                    subject,
                    event_id = %event.event_id().as_ref(),
                    event_type = event.event_type_name(),
                    "ledger persist failed"
                );
                TransportError::Message(e.to_string())
            })?;

        match &event {
            BusEvent::Freeze(freeze) => {
                debug!(
                    event_id = %freeze.event_id.as_ref(),
                    trip_id = %freeze.trip_id,
                    route_id = %freeze.route_id,
                    dedupe_key = %freeze.dedupe_key,
                    "freeze recorded"
                );
            }
            BusEvent::Diff(diff) => {
                debug!(
                    diff_id = %diff.diff_id.as_ref(),
                    event_id = %diff.event_id.as_ref(),
                    trip_id = %diff.trip_id,
                    route_id = %diff.route_id,
                    "diff recorded"
                );
            }
            BusEvent::MarketOpened(opened) => {
                debug!(
                    event_id = %opened.meta.event_id.as_ref(),
                    market_id = %opened.market_id.as_ref(),
                    "market opened recorded"
                );
            }
            BusEvent::MarketResolved(resolved) => {
                debug!(
                    event_id = %resolved.meta.event_id.as_ref(),
                    market_id = %resolved.market_id.as_ref(),
                    outcome = %resolved.outcome.as_str(),
                    "market resolved recorded"
                );
            }
            BusEvent::TradeFilled(fill) => {
                debug!(
                    event_id = %fill.meta.event_id.as_ref(),
                    trade_id = %fill.trade_id.as_ref(),
                    market_id = %fill.market_id.as_ref(),
                    agent_id = %fill.agent_id.as_ref(),
                    action = %fill.action.as_str(),
                    "trade filled recorded"
                );
            }
            _ => {}
        }

        Ok(())
    }
}
