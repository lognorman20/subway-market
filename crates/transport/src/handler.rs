use async_trait::async_trait;
use dto::BusEvent;

use crate::error::TransportError;

/// Sole in-app entry for transport **consumers** (domain, ledger, backend).
///
/// Match on [`BusEvent`] / subject → call lane functions. Do not scatter
/// subscribe loops inside business modules.
#[async_trait]
pub trait EventHandler: Send + Sync {
    async fn handle(&self, subject: &str, event: BusEvent) -> Result<(), TransportError>;
}
