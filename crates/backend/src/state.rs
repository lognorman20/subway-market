//! Shared Axum state.

use std::sync::Arc;

use diesel_async::pooled_connection::deadpool::Pool;
use diesel_async::AsyncPgConnection;
use transport::TransportClient;

use crate::auth::AuthStore;
use crate::funding::FundingConfig;
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
}
