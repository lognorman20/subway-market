//! Chain adapter for buy/sell + price reads (platform signer).

use std::str::FromStr;
use std::sync::Arc;

use alloy::primitives::{Address, B256, U256};
use async_trait::async_trait;
use chain::{ChainClient, ChainConfig, Side};
use dto::MarketSide;
use tokio::sync::Mutex;

use crate::error::ApiError;

/// Backend-facing chain surface (mockable in tests).
#[async_trait]
pub trait TradingChain: Send + Sync {
    async fn quote_buy(&self, market_id: u64, side: MarketSide, shares: u128) -> Result<u128, ApiError>;
    async fn quote_sell(&self, market_id: u64, side: MarketSide, shares: u128) -> Result<u128, ApiError>;
    async fn buy(
        &self,
        market_id: u64,
        account: Address,
        side: MarketSide,
        shares: u128,
    ) -> Result<String, ApiError>;
    async fn sell(
        &self,
        market_id: u64,
        account: Address,
        side: MarketSide,
        shares: u128,
    ) -> Result<String, ApiError>;
    async fn price_early(&self, market_id: u64) -> Result<String, ApiError>;
    async fn price_late(&self, market_id: u64) -> Result<String, ApiError>;
}

fn side_to_chain(side: MarketSide) -> Side {
    match side {
        MarketSide::Early => Side::Early,
        MarketSide::Late => Side::Late,
    }
}

fn u256_to_u128(v: U256) -> Result<u128, ApiError> {
    u128::try_from(v).map_err(|_| ApiError::Internal("u256 exceeds u128".into()))
}

fn map_chain_err(e: chain::ChainError) -> ApiError {
    ApiError::Internal(format!("chain: {e}"))
}

/// Live `ChainClient` behind a mutex (provider not Sync across all impls).
pub struct LiveTradingChain<P> {
    inner: Mutex<ChainClient<P>>,
}

impl<P> LiveTradingChain<P> {
    pub fn new(client: ChainClient<P>) -> Self {
        Self {
            inner: Mutex::new(client),
        }
    }
}

#[async_trait]
impl<P> TradingChain for LiveTradingChain<P>
where
    P: alloy::providers::Provider + Clone + Send + Sync + 'static,
{
    async fn quote_buy(&self, market_id: u64, side: MarketSide, shares: u128) -> Result<u128, ApiError> {
        let client = self.inner.lock().await;
        let cost = client
            .quote_buy(U256::from(market_id), side_to_chain(side), U256::from(shares))
            .await
            .map_err(map_chain_err)?;
        u256_to_u128(cost)
    }

    async fn quote_sell(&self, market_id: u64, side: MarketSide, shares: u128) -> Result<u128, ApiError> {
        let client = self.inner.lock().await;
        let proceeds = client
            .quote_sell(U256::from(market_id), side_to_chain(side), U256::from(shares))
            .await
            .map_err(map_chain_err)?;
        u256_to_u128(proceeds)
    }

    async fn buy(
        &self,
        market_id: u64,
        account: Address,
        side: MarketSide,
        shares: u128,
    ) -> Result<String, ApiError> {
        let client = self.inner.lock().await;
        let tx = client
            .buy(
                U256::from(market_id),
                account,
                side_to_chain(side),
                U256::from(shares),
            )
            .await
            .map_err(map_chain_err)?;
        Ok(format!("{tx:#x}"))
    }

    async fn sell(
        &self,
        market_id: u64,
        account: Address,
        side: MarketSide,
        shares: u128,
    ) -> Result<String, ApiError> {
        let client = self.inner.lock().await;
        let tx = client
            .sell(
                U256::from(market_id),
                account,
                side_to_chain(side),
                U256::from(shares),
            )
            .await
            .map_err(map_chain_err)?;
        Ok(format!("{tx:#x}"))
    }

    async fn price_early(&self, market_id: u64) -> Result<String, ApiError> {
        let client = self.inner.lock().await;
        let p = client
            .price_early(U256::from(market_id))
            .await
            .map_err(map_chain_err)?;
        Ok(p.to_string())
    }

    async fn price_late(&self, market_id: u64) -> Result<String, ApiError> {
        let client = self.inner.lock().await;
        let p = client
            .price_late(U256::from(market_id))
            .await
            .map_err(map_chain_err)?;
        Ok(p.to_string())
    }
}

/// Connect from env when contract addresses are set; else `None`.
pub fn try_connect_from_env() -> Option<Arc<dyn TradingChain>> {
    let market = std::env::var("SUBWAY_MARKET_ADDRESS").ok()?;
    let usdc = std::env::var("USDC_ADDRESS").ok()?;
    if market.trim().is_empty() || usdc.trim().is_empty() {
        return None;
    }
    let config = ChainConfig::from_env().ok()?;
    let client = ChainClient::connect(&config).ok()?;
    Some(Arc::new(LiveTradingChain::new(client)))
}

/// Deterministic EVM custody address for an agent (no agent key).
pub fn agent_custody_address(agent_id: &str) -> Address {
    use alloy::primitives::keccak256;
    let hash = keccak256(agent_id.as_bytes());
    Address::from_slice(&hash.as_slice()[12..])
}

/// In-memory mock for HTTP unit tests.
#[derive(Debug, Default)]
pub struct MockTradingChain {
    pub buy_cost: u128,
    pub sell_proceeds: u128,
    pub price_early_wad: String,
    pub price_late_wad: String,
    /// Fail `buy`/`sell` txs (quotes still succeed).
    pub fail_tx: std::sync::atomic::AtomicBool,
    pub buy_calls: std::sync::atomic::AtomicUsize,
    pub sell_calls: std::sync::atomic::AtomicUsize,
}

impl MockTradingChain {
    pub fn balanced() -> Self {
        Self {
            buy_cost: 500_000,
            sell_proceeds: 500_000,
            price_early_wad: "500000000000000000".into(),
            price_late_wad: "500000000000000000".into(),
            fail_tx: std::sync::atomic::AtomicBool::new(false),
            buy_calls: std::sync::atomic::AtomicUsize::new(0),
            sell_calls: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    pub fn buy_call_count(&self) -> usize {
        self.buy_calls.load(std::sync::atomic::Ordering::SeqCst)
    }

    pub fn sell_call_count(&self) -> usize {
        self.sell_calls.load(std::sync::atomic::Ordering::SeqCst)
    }
}

#[async_trait]
impl TradingChain for MockTradingChain {
    async fn quote_buy(
        &self,
        _market_id: u64,
        _side: MarketSide,
        _shares: u128,
    ) -> Result<u128, ApiError> {
        Ok(self.buy_cost)
    }

    async fn quote_sell(
        &self,
        _market_id: u64,
        _side: MarketSide,
        _shares: u128,
    ) -> Result<u128, ApiError> {
        Ok(self.sell_proceeds)
    }

    async fn buy(
        &self,
        _market_id: u64,
        _account: Address,
        _side: MarketSide,
        _shares: u128,
    ) -> Result<String, ApiError> {
        self.buy_calls
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if self.fail_tx.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(ApiError::Internal("mock chain fail".into()));
        }
        Ok(format!("0x{}", hex::encode(B256::repeat_byte(0xab))))
    }

    async fn sell(
        &self,
        _market_id: u64,
        _account: Address,
        _side: MarketSide,
        _shares: u128,
    ) -> Result<String, ApiError> {
        self.sell_calls
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if self.fail_tx.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(ApiError::Internal("mock chain fail".into()));
        }
        Ok(format!("0x{}", hex::encode(B256::repeat_byte(0xcd))))
    }

    async fn price_early(&self, _market_id: u64) -> Result<String, ApiError> {
        Ok(self.price_early_wad.clone())
    }

    async fn price_late(&self, _market_id: u64) -> Result<String, ApiError> {
        Ok(self.price_late_wad.clone())
    }
}

pub fn parse_market_id(raw: &str) -> Result<u64, ApiError> {
    u64::from_str(raw.trim()).map_err(|_| ApiError::BadRequest(format!("bad market_id: {raw}")))
}

pub fn parse_shares(raw: &str) -> Result<u128, ApiError> {
    u128::from_str(raw.trim()).map_err(|_| ApiError::BadRequest(format!("bad shares: {raw}")))
}
