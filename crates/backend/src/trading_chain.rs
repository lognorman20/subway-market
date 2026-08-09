//! Chain adapter for buy/sell/create/resolve + price reads (platform signer).

use std::str::FromStr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};

use alloy::primitives::{Address, B256, U256};
use async_trait::async_trait;
use chain::{mint_mock_usdc, ChainClient, ChainConfig, Outcome, Side};
use dto::{MarketOutcome, MarketSide};
use tokio::sync::Mutex;

use crate::error::ApiError;

/// Result of interim treasury mirror after Rain ACH settle (Path C — not a real bridge).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TreasuryMirrorResult {
    /// MockUSDC minted 1:1 to platform signer on Monad/anvil.
    Minted { tx_hash: String },
    /// Real USDC / no mint — platform must already hold spendable USDC.
    Skipped { reason: String },
}

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

    /// Create LMSR market; returns `(market_id, tx_hash)`.
    async fn create_market(
        &self,
        train_id: &str,
        trip_id: &str,
        stop_id: &str,
        frozen_eta_unix: i64,
        liquidity_b: u128,
    ) -> Result<(u64, String), ApiError>;

    /// Resolve market to Early or Late.
    async fn resolve_market(
        &self,
        market_id: u64,
        outcome: MarketOutcome,
    ) -> Result<String, ApiError>;

    /// Interim treasury mirror: credit Monad/anvil USDC to platform 1:1 with Rain settle.
    ///
    /// Rain sandbox settles USDC on **Base** (not Monad). Until a real bridge exists,
    /// local/anvil mints MockUSDC so LMSR spends track funded amount. Not a bridge.
    async fn treasury_mirror_usdc(&self, amount_micro: u128) -> Result<TreasuryMirrorResult, ApiError>;
}

fn side_to_chain(side: MarketSide) -> Side {
    match side {
        MarketSide::Early => Side::Early,
        MarketSide::Late => Side::Late,
    }
}

fn outcome_to_chain(outcome: MarketOutcome) -> Result<Outcome, ApiError> {
    match outcome {
        MarketOutcome::Early => Ok(Outcome::Early),
        MarketOutcome::Late => Ok(Outcome::Late),
        MarketOutcome::Unresolved => Err(ApiError::BadRequest(
            "outcome must be early or late".into(),
        )),
    }
}

fn u256_to_u128(v: U256) -> Result<u128, ApiError> {
    u128::try_from(v).map_err(|_| ApiError::Internal("u256 exceeds u128".into()))
}

fn u256_to_u64(v: U256) -> Result<u64, ApiError> {
    u64::try_from(v).map_err(|_| ApiError::Internal("u256 exceeds u64".into()))
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

    async fn create_market(
        &self,
        train_id: &str,
        trip_id: &str,
        stop_id: &str,
        frozen_eta_unix: i64,
        liquidity_b: u128,
    ) -> Result<(u64, String), ApiError> {
        let frozen = u64::try_from(frozen_eta_unix)
            .map_err(|_| ApiError::BadRequest("frozen_eta_unix must be >= 0".into()))?;
        let client = self.inner.lock().await;
        let (id, tx) = client
            .create_market(
                train_id,
                trip_id,
                stop_id,
                frozen,
                U256::from(liquidity_b),
            )
            .await
            .map_err(map_chain_err)?;
        Ok((u256_to_u64(id)?, format!("{tx:#x}")))
    }

    async fn resolve_market(
        &self,
        market_id: u64,
        outcome: MarketOutcome,
    ) -> Result<String, ApiError> {
        let chain_outcome = outcome_to_chain(outcome)?;
        let client = self.inner.lock().await;
        let tx = client
            .resolve_market(U256::from(market_id), chain_outcome)
            .await
            .map_err(map_chain_err)?;
        Ok(format!("{tx:#x}"))
    }

    async fn treasury_mirror_usdc(&self, amount_micro: u128) -> Result<TreasuryMirrorResult, ApiError> {
        if amount_micro == 0 {
            return Ok(TreasuryMirrorResult::Skipped {
                reason: "zero amount".into(),
            });
        }
        let client = self.inner.lock().await;
        let to = client.signer_address();
        let usdc = client.usdc_address();
        let usdc_hex = format!("{usdc:#x}");
        // Circle native USDC on Monad testnet (one-pager) — no mint; use faucet or CCTP.
        if usdc_hex.eq_ignore_ascii_case(crate::funding::MONAD_TESTNET_USDC) {
            return Ok(TreasuryMirrorResult::Skipped {
                reason: format!(
                    "USDC_ADDRESS is Circle Monad testnet USDC; interim mint N/A — \
                     top platform via https://faucet.circle.com/ or Circle CCTP from Base \
                     Rain dest (amount_micro={amount_micro})"
                ),
            });
        }
        match mint_mock_usdc(client.provider(), usdc, to, U256::from(amount_micro)).await {
            Ok(tx) => Ok(TreasuryMirrorResult::Minted {
                tx_hash: format!("{tx:#x}"),
            }),
            Err(e) => {
                // Real USDC has no mint — expected on Monad without MockUSDC.
                tracing::warn!(
                    error = %e,
                    amount_micro,
                    "interim treasury mirror mint failed (not a real Rain→Monad bridge); \
                     platform wallet must already hold USDC for LMSR"
                );
                Ok(TreasuryMirrorResult::Skipped {
                    reason: format!("mint unavailable: {e}"),
                })
            }
        }
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
    pub fail_tx: AtomicBool,
    pub buy_calls: AtomicUsize,
    pub sell_calls: AtomicUsize,
    pub create_calls: AtomicUsize,
    pub resolve_calls: AtomicUsize,
    pub next_market_id: AtomicU64,
    pub treasury_mirror_calls: AtomicUsize,
    pub treasury_mirror_micro: AtomicU64,
}

impl MockTradingChain {
    pub fn balanced() -> Self {
        Self {
            buy_cost: 500_000,
            sell_proceeds: 500_000,
            price_early_wad: "500000000000000000".into(),
            price_late_wad: "500000000000000000".into(),
            fail_tx: AtomicBool::new(false),
            buy_calls: AtomicUsize::new(0),
            sell_calls: AtomicUsize::new(0),
            create_calls: AtomicUsize::new(0),
            resolve_calls: AtomicUsize::new(0),
            next_market_id: AtomicU64::new(1),
            treasury_mirror_calls: AtomicUsize::new(0),
            treasury_mirror_micro: AtomicU64::new(0),
        }
    }

    pub fn buy_call_count(&self) -> usize {
        self.buy_calls.load(Ordering::SeqCst)
    }

    pub fn sell_call_count(&self) -> usize {
        self.sell_calls.load(Ordering::SeqCst)
    }

    pub fn create_call_count(&self) -> usize {
        self.create_calls.load(Ordering::SeqCst)
    }

    pub fn resolve_call_count(&self) -> usize {
        self.resolve_calls.load(Ordering::SeqCst)
    }

    pub fn treasury_mirror_call_count(&self) -> usize {
        self.treasury_mirror_calls.load(Ordering::SeqCst)
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
        self.buy_calls.fetch_add(1, Ordering::SeqCst);
        if self.fail_tx.load(Ordering::SeqCst) {
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
        self.sell_calls.fetch_add(1, Ordering::SeqCst);
        if self.fail_tx.load(Ordering::SeqCst) {
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

    async fn create_market(
        &self,
        _train_id: &str,
        _trip_id: &str,
        _stop_id: &str,
        _frozen_eta_unix: i64,
        _liquidity_b: u128,
    ) -> Result<(u64, String), ApiError> {
        self.create_calls.fetch_add(1, Ordering::SeqCst);
        if self.fail_tx.load(Ordering::SeqCst) {
            return Err(ApiError::Internal("mock chain fail".into()));
        }
        let id = self.next_market_id.fetch_add(1, Ordering::SeqCst);
        Ok((
            id,
            format!("0x{}", hex::encode(B256::repeat_byte(0x11))),
        ))
    }

    async fn resolve_market(
        &self,
        _market_id: u64,
        outcome: MarketOutcome,
    ) -> Result<String, ApiError> {
        self.resolve_calls.fetch_add(1, Ordering::SeqCst);
        if !matches!(outcome, MarketOutcome::Early | MarketOutcome::Late) {
            return Err(ApiError::BadRequest(
                "outcome must be early or late".into(),
            ));
        }
        if self.fail_tx.load(Ordering::SeqCst) {
            return Err(ApiError::Internal("mock chain fail".into()));
        }
        Ok(format!("0x{}", hex::encode(B256::repeat_byte(0x22))))
    }

    async fn treasury_mirror_usdc(&self, amount_micro: u128) -> Result<TreasuryMirrorResult, ApiError> {
        self.treasury_mirror_calls.fetch_add(1, Ordering::SeqCst);
        let capped = u64::try_from(amount_micro).unwrap_or(u64::MAX);
        self.treasury_mirror_micro
            .fetch_add(capped, Ordering::SeqCst);
        Ok(TreasuryMirrorResult::Minted {
            tx_hash: format!("0x{}", hex::encode(B256::repeat_byte(0xee))),
        })
    }
}

pub fn parse_market_id(raw: &str) -> Result<u64, ApiError> {
    u64::from_str(raw.trim()).map_err(|_| ApiError::BadRequest(format!("bad market_id: {raw}")))
}

pub fn parse_shares(raw: &str) -> Result<u128, ApiError> {
    u128::from_str(raw.trim()).map_err(|_| ApiError::BadRequest(format!("bad shares: {raw}")))
}
