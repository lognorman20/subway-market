//! Domain binary — EventHandler + replay wiring.

use std::sync::Arc;

use anyhow::Context;
use dto::BusEvent;
use domain::engine::EmitMode;
use domain::handler::DomainHandler;
use domain::replay::{connect_pool, rebuild_from_ledger};
use domain::state::TripStore;
use futures::StreamExt;
use tokio::sync::{mpsc, Mutex};
use tracing::{debug, error, info, warn};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    transport::init_tracing_stdout("domain");
    info!("boot");

    let nats_url = std::env::var("NATS_URL").unwrap_or_else(|_| "nats://127.0.0.1:4223".into());
    debug!(%nats_url, "config");
    let bus_client = transport::connect(&nats_url).await.context("connect NATS")?;

    let subjects = [
        dto::INGEST_WATCHABLE,
        dto::INGEST_TRIP,
        dto::INGEST_VEHICLE,
    ];

    let store = Arc::new(Mutex::new(TripStore::new()));
    let mode = Arc::new(Mutex::new(EmitMode::Silent));

    let database_url = std::env::var("DATABASE_URL").ok();
    let db = if let Some(ref url) = database_url {
        match connect_pool(url).await {
            Ok(pool) => Some(pool),
            Err(err) => {
                error!(error = %err, "postgres connect failed");
                return Err(err.into());
            }
        }
    } else {
        warn!("DATABASE_URL unset — ledger replay skipped");
        None
    };

    let handler = Arc::new(DomainHandler {
        store: store.clone(),
        bus: Some(bus_client.clone()),
        mode: mode.clone(),
        db,
        emit_recorder: None,
        freeze_recorder: None,
        test_ledger_has_key: None,
    });

    // 1. Subscribe + buffer live events before rebuild.
    let (tx, mut rx) = mpsc::unbounded_channel::<(String, BusEvent)>();

    for subject in &subjects {
        let subject = (*subject).to_string();
        let sub = bus_client
            .inner()
            .subscribe(subject.clone())
            .await
            .context("subscribe")?;
        info!(%subject, "subscribed");
        let tx = tx.clone();
        tokio::spawn(async move {
            let mut sub = sub;
            while let Some(msg) = sub.next().await {
                match serde_json::from_slice::<BusEvent>(&msg.payload) {
                    Ok(event) => {
                        if tx.send((msg.subject.to_string(), event)).is_err() {
                            break;
                        }
                    }
                    Err(err) => {
                        error!(error = %err, subject = %msg.subject, "bad bus payload");
                    }
                }
            }
            warn!(%subject, "subscription ended");
        });
    }
    drop(tx);

    transport::mark_ready();
    info!("subscribed; rebuilding from ledger");

    // 2–3. Preload done keys + replay ingest with emit=false (fail-fast when DATABASE_URL set).
    if let Some(url) = database_url {
        let mut locked = store.lock().await;
        if let Err(err) = rebuild_from_ledger(&url, &mut locked).await {
            error!(error = %err, "ledger replay rebuild failed");
            return Err(err.into());
        }
        info!(
            trips = locked.len(),
            done = locked.done.len(),
            "replay complete"
        );
    }

    // 4. Drain buffer with emit=true.
    {
        let mut locked = mode.lock().await;
        *locked = EmitMode::Live;
    }

    while let Ok((subject, event)) = rx.try_recv() {
        let event_id = event.event_id().as_ref().to_string();
        if let Err(err) = handler.process(&subject, event).await {
            error!(
                error = %err,
                %subject,
                event_id = %event_id,
                "buffer drain process failed"
            );
            return Err(err.into());
        }
    }
    info!("buffer drained; live");

    // 5. Continue live.
    while let Some((subject, event)) = rx.recv().await {
        let event_id = event.event_id().as_ref().to_string();
        if let Err(err) = handler.process(&subject, event).await {
            error!(
                error = %err,
                %subject,
                event_id = %event_id,
                "live process failed"
            );
            return Err(err.into());
        }
    }

    Ok(())
}
