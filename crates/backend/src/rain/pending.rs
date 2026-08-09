//! In-memory pending Rain fundings (agent ↔ transfer correlation).

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use tokio::sync::RwLock;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FundingStatus {
    Pending,
    Settled,
    Failed,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PendingFunding {
    pub id: String,
    pub agent_id: String,
    pub owner_wallet: String,
    /// Whole USDC requested.
    pub amount_usdc: i64,
    pub amount_micro: i64,
    pub payment_route_id: String,
    pub simulation_id: Option<String>,
    pub rain_tx_id: Option<String>,
    pub status: FundingStatus,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Default)]
pub struct PendingFundingStore {
    inner: Arc<RwLock<HashMap<String, PendingFunding>>>,
    /// Rain tx ids already credited (idempotent webhook/poll).
    credited_tx: Arc<RwLock<HashMap<String, String>>>,
}

impl PendingFundingStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn insert(&self, funding: PendingFunding) {
        self.inner.write().await.insert(funding.id.clone(), funding);
    }

    pub async fn get(&self, id: &str) -> Option<PendingFunding> {
        self.inner.read().await.get(id).cloned()
    }

    pub async fn update<F>(&self, id: &str, f: F) -> Option<PendingFunding>
    where
        F: FnOnce(&mut PendingFunding),
    {
        let mut guard = self.inner.write().await;
        let entry = guard.get_mut(id)?;
        f(entry);
        Some(entry.clone())
    }

    /// Find pending funding matching amount_micro (FIFO by created_at).
    pub async fn find_pending_by_amount(&self, amount_micro: i64) -> Option<PendingFunding> {
        let guard = self.inner.read().await;
        let mut matches: Vec<_> = guard
            .values()
            .filter(|f| f.status == FundingStatus::Pending && f.amount_micro == amount_micro)
            .cloned()
            .collect();
        matches.sort_by_key(|f| f.created_at);
        matches.into_iter().next()
    }

    pub async fn find_pending_by_simulation(&self, simulation_id: &str) -> Option<PendingFunding> {
        let guard = self.inner.read().await;
        guard
            .values()
            .find(|f| {
                f.status == FundingStatus::Pending
                    && f.simulation_id.as_deref() == Some(simulation_id)
            })
            .cloned()
    }

    /// Returns true if this rain_tx_id was already credited.
    pub async fn already_credited(&self, rain_tx_id: &str) -> bool {
        self.credited_tx.read().await.contains_key(rain_tx_id)
    }

    pub async fn mark_credited(&self, rain_tx_id: &str, funding_id: &str) {
        self.credited_tx
            .write()
            .await
            .insert(rain_tx_id.to_string(), funding_id.to_string());
    }

    pub fn new_id() -> String {
        format!("fund_{}", Uuid::new_v4())
    }
}
