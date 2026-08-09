use std::sync::Arc;

use async_nats::Client;
use bytes::Bytes;
use dto::BusEvent;
use futures::StreamExt;
use tracing::{debug, error, info, warn};

use crate::error::TransportError;
use crate::handler::EventHandler;
use crate::logging::{log_healthy, mark_ready};

/// Connected NATS client wrapper.
#[derive(Clone)]
pub struct TransportClient {
    client: Client,
}

impl TransportClient {
    pub fn inner(&self) -> &Client {
        &self.client
    }

    /// Publish a typed event to `subject` as JSON.
    pub async fn emit(&self, subject: &str, event: &BusEvent) -> Result<(), TransportError> {
        let payload = serde_json::to_vec(event)?;
        self.client
            .publish(subject.to_string(), Bytes::from(payload))
            .await
            .map_err(|e| TransportError::Message(e.to_string()))?;
        debug!(
            subject,
            event_id = %event.event_id().as_ref(),
            event_type = event.event_type_name(),
            "emitted"
        );
        Ok(())
    }
}

/// Connect to NATS (`NATS_URL` or `nats://127.0.0.1:4223`).
pub async fn connect(url: &str) -> Result<TransportClient, TransportError> {
    let client = async_nats::connect(url)
        .await
        .map_err(|e| TransportError::Message(e.to_string()))?;
    info!(%url, "nats connected");
    Ok(TransportClient { client })
}

/// Subscribe to `subjects` and deliver each message to `handler`.
///
/// Establishes **all** subscriptions first, then [`mark_ready`] + [`log_healthy`],
/// then runs the read loops until streams end / process stops.
pub async fn run_handler(
    transport: TransportClient,
    subjects: &[&str],
    handler: Arc<dyn EventHandler>,
) -> Result<(), TransportError> {
    let mut subs = Vec::with_capacity(subjects.len());
    for subject in subjects {
        let subject = (*subject).to_string();
        let sub = transport
            .client
            .subscribe(subject.clone())
            .await
            .map_err(|e| TransportError::Message(e.to_string()))?;
        info!(%subject, "subscribed");
        subs.push((subject, sub));
    }

    mark_ready();
    log_healthy("subscribed; handling events");

    let mut join_set = tokio::task::JoinSet::new();
    for (subject, mut sub) in subs {
        let handler = Arc::clone(&handler);
        join_set.spawn(async move {
            while let Some(msg) = sub.next().await {
                match serde_json::from_slice::<BusEvent>(&msg.payload) {
                    Ok(event) => {
                        let event_id = event.event_id().as_ref().to_string();
                        let event_type = event.event_type_name();
                        debug!(
                            subject = %msg.subject,
                            event_id = %event_id,
                            event_type = %event_type,
                            "transport message"
                        );
                        if let Err(err) = handler.handle(&msg.subject, event).await {
                            error!(
                                error = %err,
                                subject = %msg.subject,
                                event_id = %event_id,
                                event_type = %event_type,
                                "handler failed"
                            );
                        }
                    }
                    Err(err) => {
                        error!(error = %err, subject = %msg.subject, "bad transport payload");
                    }
                }
            }
            warn!(%subject, "subscription ended");
            Ok::<(), TransportError>(())
        });
    }

    while let Some(res) = join_set.join_next().await {
        match res {
            Ok(Ok(())) => {}
            Ok(Err(e)) => return Err(e),
            Err(e) => return Err(TransportError::Message(e.to_string())),
        }
    }
    Ok(())
}
