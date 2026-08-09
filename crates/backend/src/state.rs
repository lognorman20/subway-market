//! Shared Axum state.

use std::sync::Arc;

use diesel_async::pooled_connection::deadpool::Pool;
use diesel_async::AsyncPgConnection;
use transport::TransportClient;

use crate::auth::AuthStore;
use crate::funding::FundingConfig;
use crate::rain::{PendingFundingStore, RainApi, SettlePollConfig};
use crate::trading_chain::TradingChain;

#[derive(Clone)]
pub struct AppState {
    pub pool: Pool<AsyncPgConnection>,
    pub auth: AuthStore,
    pub funding: FundingConfig,
    /// Optional — buy/sell fail with 503 when absent.
    pub chain: Option<Arc<dyn TradingChain>>,
    /// Optional — tests may omit; emit skipped when absent.
    pub nats: Option<TransportClient>,
    /// Rain HTTP client — set when `FUNDING_MODE=testnet` + `RAIN_*` configured.
    pub rain: Option<Arc<dyn RainApi>>,
    pub pending_funding: PendingFundingStore,
    /// Override poll timings (tests); default production backoff when `None`.
    pub settle_poll: Option<SettlePollConfig>,
}
