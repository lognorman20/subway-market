//! Domain EventHandler.

use std::sync::Arc;

use async_trait::async_trait;
use diesel_async::pooled_connection::deadpool::Pool;
use diesel_async::AsyncPgConnection;
use dto::{BusEvent, DiffEvent, FreezeEvent};
use tokio::sync::Mutex;
use tracing::{debug, error, warn};
use transport::{EventHandler, TransportError};

use crate::engine::{apply, DomainEmit, EmitMode};
use crate::replay::ledger_has_dedupe_key;
use crate::state::TripStore;

pub struct DomainHandler {
    pub store: Arc<Mutex<TripStore>>,
    pub bus: Option<transport::TransportClient>,
    pub mode: Arc<Mutex<EmitMode>>,
    pub db: Option<Pool<AsyncPgConnection>>,
    /// Test-only: capture emitted diffs instead of NATS publish.
    #[doc(hidden)]
    pub emit_recorder: Option<Arc<Mutex<Vec<DiffEvent>>>>,
    /// Test-only: capture emitted freezes.
    #[doc(hidden)]
    pub freeze_recorder: Option<Arc<Mutex<Vec<FreezeEvent>>>>,
    /// Test-only: simulate ledger dedupe lookup (`true` → skip emit).
    #[doc(hidden)]
    pub test_ledger_has_key: Option<Arc<dyn Fn(&str) -> bool + Send + Sync>>,
}

impl DomainHandler {
    /// Process one event (shared by handler + tests).
    pub async fn process(&self, subject: &str, event: BusEvent) -> Result<(), TransportError> {
        debug!(
            subject,
            event_id = %event.event_id().as_ref(),
            event_type = event.event_type_name(),
            "on_event"
        );

        let mode = *self.mode.lock().await;
        let emits = {
            let mut store = self.store.lock().await;
            apply(&mut store, &event, mode)
        };

        for emit in emits {
            match emit {
                DomainEmit::Freeze(freeze) => {
                    if mode == EmitMode::Live {
                        self.publish_freeze(&freeze).await?;
                        debug!(
                            event_id = %freeze.event_id.as_ref(),
                            trip_id = %freeze.trip_id,
                            dedupe_key = %freeze.dedupe_key,
                            frozen_eta_unix = freeze.frozen_eta_unix,
                            "freeze emitted"
                        );
                    }
                }
                DomainEmit::Diff(diff) => {
                    if mode == EmitMode::Live {
                        let key = diff.dedupe_key.clone();
                        {
                            let store = self.store.lock().await;
                            if store.is_done(&key) {
                                debug!(dedupe_key = %key, "emit skipped (local done)");
                                continue;
                            }
                        }

                        if self.ledger_has_dedupe_key(&key).await {
                            let mut store = self.store.lock().await;
                            store.mark_done(&key);
                            debug!(dedupe_key = %key, "emit skipped (ledger hit)");
                            continue;
                        }

                        self.publish_diff(&diff).await?;

                        let mut store = self.store.lock().await;
                        store.mark_done(&key);
                        debug!(
                            diff_id = %diff.diff_id.as_ref(),
                            trip_id = %diff.trip_id,
                            dedupe_key = %diff.dedupe_key,
                            diff_seconds = diff.diff_seconds,
                            "diff emitted"
                        );
                    }
                }
            }
        }
        Ok(())
    }

    async fn ledger_has_dedupe_key(&self, key: &str) -> bool {
        if let Some(check) = &self.test_ledger_has_key {
            return check(key);
        }
        if let Some(pool) = &self.db {
            match ledger_has_dedupe_key(pool, key).await {
                Ok(true) => return true,
                Ok(false) => {}
                Err(err) => {
                    warn!(error = %err, dedupe_key = %key, "ledger dedupe check failed; may re-emit");
                }
            }
        }
        false
    }

    async fn publish_freeze(&self, freeze: &FreezeEvent) -> Result<(), TransportError> {
        if let Some(rec) = &self.freeze_recorder {
            rec.lock().await.push(freeze.clone());
            return Ok(());
        }
        let bus = self
            .bus
            .as_ref()
            .expect("bus client required when freeze_recorder unset");
        bus.emit(dto::DOMAIN_FREEZE, &BusEvent::Freeze(freeze.clone()))
            .await
            .map_err(|err| {
                error!(
                    error = %err,
                    event_id = %freeze.event_id.as_ref(),
                    trip_id = %freeze.trip_id,
                    subject = dto::DOMAIN_FREEZE,
                    "freeze NATS emit failed"
                );
                err
            })
    }

    async fn publish_diff(&self, diff: &DiffEvent) -> Result<(), TransportError> {
        if let Some(rec) = &self.emit_recorder {
            rec.lock().await.push(diff.clone());
            return Ok(());
        }
        let bus = self
            .bus
            .as_ref()
            .expect("bus client required when emit_recorder unset");
        bus.emit(dto::DOMAIN_DIFF, &BusEvent::Diff(diff.clone()))
            .await
            .map_err(|err| {
                error!(
                    error = %err,
                    diff_id = %diff.diff_id.as_ref(),
                    event_id = %diff.event_id.as_ref(),
                    trip_id = %diff.trip_id,
                    subject = dto::DOMAIN_DIFF,
                    "diff NATS emit failed"
                );
                err
            })
    }
}

#[async_trait]
impl EventHandler for DomainHandler {
    async fn handle(&self, subject: &str, event: BusEvent) -> Result<(), TransportError> {
        self.process(subject, event).await
    }
}
