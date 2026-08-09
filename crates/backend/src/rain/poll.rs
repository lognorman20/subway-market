//! Bounded backoff poll for Rain transfer settlement.

use std::time::Duration;

use tokio::time::{sleep, Instant};

use super::client::{RainApi, RainClientError};
use super::pending::{FundingStatus, PendingFunding, PendingFundingStore};
use super::types::{major_decimal_to_micro, RainTransaction, TransferStatus};

#[derive(Debug, Clone)]
pub struct SettlePollConfig {
    pub initial_backoff: Duration,
    pub max_backoff: Duration,
    pub max_attempts: u32,
    pub overall_timeout: Duration,
}

impl Default for SettlePollConfig {
    fn default() -> Self {
        Self {
            initial_backoff: Duration::from_millis(200),
            max_backoff: Duration::from_secs(2),
            max_attempts: 40,
            overall_timeout: Duration::from_secs(30),
        }
    }
}

impl SettlePollConfig {
    /// Fast intervals for unit tests.
    pub fn for_tests() -> Self {
        Self {
            initial_backoff: Duration::from_millis(1),
            max_backoff: Duration::from_millis(5),
            max_attempts: 50,
            overall_timeout: Duration::from_secs(5),
        }
    }
}

#[derive(Debug)]
pub enum PollOutcome {
    Settled { tx: RainTransaction },
    Failed { tx: RainTransaction },
    TimedOut,
}

/// Poll Rain until transfer settles/fails or timeout.
///
/// Uses bounded exponential backoff between attempts (not a raw unbounded sleep).
pub async fn poll_until_settled(
    client: &dyn RainApi,
    store: &PendingFundingStore,
    funding: &PendingFunding,
    cfg: &SettlePollConfig,
) -> Result<PollOutcome, RainClientError> {
    let deadline = Instant::now() + cfg.overall_timeout;
    let mut backoff = cfg.initial_backoff;
    let mut attempts = 0u32;

    loop {
        if attempts >= cfg.max_attempts || Instant::now() >= deadline {
            return Ok(PollOutcome::TimedOut);
        }
        attempts += 1;

        if let Some(outcome) = check_once(client, store, funding).await? {
            return Ok(outcome);
        }

        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Ok(PollOutcome::TimedOut);
        }
        let wait = backoff.min(remaining).min(cfg.max_backoff);
        sleep(wait).await;
        backoff = (backoff * 2).min(cfg.max_backoff);
    }
}

async fn check_once(
    client: &dyn RainApi,
    store: &PendingFundingStore,
    funding: &PendingFunding,
) -> Result<Option<PollOutcome>, RainClientError> {
    // Prefer known tx id.
    if let Some(tx_id) = funding.rain_tx_id.as_deref() {
        let tx = client.get_transaction(tx_id).await?;
        return Ok(classify(&tx));
    }

    // Fresh copy may have rain_tx_id from concurrent webhook.
    if let Some(latest) = store.get(&funding.id).await {
        if let Some(tx_id) = latest.rain_tx_id.as_deref() {
            let tx = client.get_transaction(tx_id).await?;
            return Ok(classify(&tx));
        }
        if latest.status == FundingStatus::Settled {
            // Synthesize — already settled elsewhere.
            return Ok(Some(PollOutcome::Settled {
                tx: RainTransaction {
                    id: latest
                        .rain_tx_id
                        .clone()
                        .unwrap_or_else(|| "local-settled".into()),
                    tx_type: "transfer".into(),
                    transfer: None,
                },
            }));
        }
    }

    let txs = client.list_transfer_transactions(20).await?;
    for tx in txs {
        if tx.tx_type != "transfer" {
            continue;
        }
        let Some(status) = tx.transfer_status() else {
            continue;
        };
        let Some(amount_str) = tx.amount_major_str() else {
            continue;
        };
        let Some(micro) = major_decimal_to_micro(amount_str) else {
            continue;
        };
        if micro != funding.amount_micro {
            continue;
        }
        if store.already_credited(&tx.id).await {
            continue;
        }
        if let Some(outcome) = classify_status(status, tx) {
            return Ok(Some(outcome));
        }
    }
    Ok(None)
}

fn classify(tx: &RainTransaction) -> Option<PollOutcome> {
    let status = tx.transfer_status()?;
    classify_status(status, tx.clone())
}

fn classify_status(status: TransferStatus, tx: RainTransaction) -> Option<PollOutcome> {
    if status.is_terminal_success() {
        Some(PollOutcome::Settled { tx })
    } else if status.is_terminal_failure() {
        Some(PollOutcome::Failed { tx })
    } else {
        None
    }
}
