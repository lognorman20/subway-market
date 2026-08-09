//! Tx + read helpers for `SubwayMarket` using one platform signer.

use alloy::network::ReceiptResponse;
use alloy::primitives::{Address, B256, U256};
use alloy::providers::{Provider, ProviderBuilder};
use alloy::signers::local::PrivateKeySigner;

use crate::bindings::{IERC20, MockUSDC, SubwayMarket};
use crate::config::ChainConfig;
use crate::error::{ChainError, Result};
use crate::types::{Outcome, Side};

/// Snapshot of on-chain market storage fields useful to backend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketState {
    pub train_id: String,
    pub trip_id: String,
    pub stop_id: String,
    pub frozen_eta_unix: u64,
    pub liquidity_b: U256,
    pub q_early: U256,
    pub q_late: U256,
    pub outcome: Outcome,
    pub exists: bool,
}

/// Convenience client: platform signer owns all LMSR txs (decision #3).
#[derive(Clone, Debug)]
pub struct ChainClient<P> {
    provider: P,
    market: Address,
    usdc: Address,
    signer_address: Address,
}

impl ChainClient<()> {
    /// Connect HTTP RPC with platform signer from config.
    pub fn connect(config: &ChainConfig) -> Result<ChainClient<impl Provider + Clone>> {
        let signer = config.signer()?;
        let provider = http_provider_with_signer(&config.rpc_url, signer)?;
        Ok(ChainClient {
            provider,
            market: config.subway_market,
            usdc: config.usdc,
            signer_address: config.signer_address,
        })
    }
}

impl<P> ChainClient<P>
where
    P: Provider + Clone,
{
    /// Wrap an existing provider (tests / custom wallets).
    pub fn new(provider: P, market: Address, usdc: Address, signer_address: Address) -> Self {
        Self {
            provider,
            market,
            usdc,
            signer_address,
        }
    }

    pub fn provider(&self) -> &P {
        &self.provider
    }

    pub fn market_address(&self) -> Address {
        self.market
    }

    pub fn usdc_address(&self) -> Address {
        self.usdc
    }

    pub fn signer_address(&self) -> Address {
        self.signer_address
    }

    fn market_contract(&self) -> SubwayMarket::SubwayMarketInstance<&P> {
        SubwayMarket::new(self.market, &self.provider)
    }

    fn usdc_contract(&self) -> IERC20::IERC20Instance<&P> {
        IERC20::new(self.usdc, &self.provider)
    }

    /// Approve USDC spend for the market contract (platform payer).
    pub async fn approve_usdc(&self, amount: U256) -> Result<B256> {
        let pending = self
            .usdc_contract()
            .approve(self.market, amount)
            .send()
            .await?;
        let receipt = pending.get_receipt().await?;
        Ok(receipt.transaction_hash())
    }

    /// Open a market; returns new `marketId`. Pulls LMSR subsidy from platform.
    pub async fn create_market(
        &self,
        train_id: &str,
        trip_id: &str,
        stop_id: &str,
        frozen_eta_unix: u64,
        liquidity_b: U256,
    ) -> Result<(U256, B256)> {
        let market = self.market_contract();
        let market_id = market.nextMarketId().call().await?;

        // Approve max so subsidy + later buys work without re-approving each time.
        self.approve_usdc(U256::MAX).await?;

        let pending = market
            .createMarket(
                train_id.to_string(),
                trip_id.to_string(),
                stop_id.to_string(),
                frozen_eta_unix,
                liquidity_b,
            )
            .send()
            .await?;
        let receipt = pending.get_receipt().await?;
        Ok((market_id, receipt.transaction_hash()))
    }

    /// Buy shares for `account` (platform pays USDC).
    pub async fn buy(
        &self,
        market_id: U256,
        account: Address,
        side: Side,
        shares: U256,
    ) -> Result<B256> {
        // Keep allowance topped up (idempotent when already MAX).
        self.approve_usdc(U256::MAX).await?;

        let pending = self
            .market_contract()
            .buy(market_id, account, side.as_u8(), shares)
            .send()
            .await?;
        let receipt = pending.get_receipt().await?;
        Ok(receipt.transaction_hash())
    }

    /// Sell shares for `account` (platform receives USDC).
    pub async fn sell(
        &self,
        market_id: U256,
        account: Address,
        side: Side,
        shares: U256,
    ) -> Result<B256> {
        let pending = self
            .market_contract()
            .sell(market_id, account, side.as_u8(), shares)
            .send()
            .await?;
        let receipt = pending.get_receipt().await?;
        Ok(receipt.transaction_hash())
    }

    /// Resolve to Early or Late only.
    pub async fn resolve_market(&self, market_id: U256, outcome: Outcome) -> Result<B256> {
        if !outcome.is_resolvable() {
            return Err(ChainError::InvalidResolveOutcome);
        }
        let pending = self
            .market_contract()
            .resolveMarket(market_id, outcome.as_u8())
            .send()
            .await?;
        let receipt = pending.get_receipt().await?;
        Ok(receipt.transaction_hash())
    }

    /// Resolve from actual arrival vs frozen ETA.
    pub async fn resolve_market_from_arrival(
        &self,
        market_id: U256,
        actual_arrival_unix: u64,
    ) -> Result<B256> {
        let pending = self
            .market_contract()
            .resolveMarketFromArrival(market_id, actual_arrival_unix)
            .send()
            .await?;
        let receipt = pending.get_receipt().await?;
        Ok(receipt.transaction_hash())
    }

    /// Marginal Early price in WAD (1e18 = 100%).
    pub async fn price_early(&self, market_id: U256) -> Result<U256> {
        Ok(self.market_contract().priceEarly(market_id).call().await?)
    }

    /// Marginal Late price in WAD.
    pub async fn price_late(&self, market_id: U256) -> Result<U256> {
        Ok(self.market_contract().priceLate(market_id).call().await?)
    }

    pub async fn quote_buy(&self, market_id: U256, side: Side, shares: U256) -> Result<U256> {
        Ok(self
            .market_contract()
            .quoteBuy(market_id, side.as_u8(), shares)
            .call()
            .await?)
    }

    pub async fn quote_sell(&self, market_id: U256, side: Side, shares: U256) -> Result<U256> {
        Ok(self
            .market_contract()
            .quoteSell(market_id, side.as_u8(), shares)
            .call()
            .await?)
    }

    pub async fn early_balance(&self, market_id: U256, account: Address) -> Result<U256> {
        Ok(self
            .market_contract()
            .earlyBalance(market_id, account)
            .call()
            .await?)
    }

    pub async fn late_balance(&self, market_id: U256, account: Address) -> Result<U256> {
        Ok(self
            .market_contract()
            .lateBalance(market_id, account)
            .call()
            .await?)
    }

    /// Read full market storage row.
    pub async fn market_state(&self, market_id: U256) -> Result<MarketState> {
        let m = self.market_contract().markets(market_id).call().await?;
        let outcome =
            Outcome::from_u8(m.outcome).ok_or(ChainError::UnknownOutcome(m.outcome))?;
        Ok(MarketState {
            train_id: m.trainId,
            trip_id: m.tripId,
            stop_id: m.stopId,
            frozen_eta_unix: m.frozenEtaUnix,
            liquidity_b: m.liquidityB,
            q_early: m.qEarly,
            q_late: m.qLate,
            outcome,
            exists: m.exists,
        })
    }

    pub async fn next_market_id(&self) -> Result<U256> {
        Ok(self.market_contract().nextMarketId().call().await?)
    }

    pub async fn platform(&self) -> Result<Address> {
        Ok(self.market_contract().platform().call().await?)
    }

    pub async fn version(&self) -> Result<String> {
        Ok(self.market_contract().VERSION().call().await?)
    }
}

/// Deploy MockUSDC + SubwayMarket on a wallet provider (anvil / local).
pub async fn deploy_local<P>(provider: &P, platform: Address) -> Result<(Address, Address)>
where
    P: Provider + Clone,
{
    let usdc = MockUSDC::deploy(provider).await?;
    let usdc_addr = *usdc.address();
    let market = SubwayMarket::deploy(provider, usdc_addr, platform).await?;
    Ok((usdc_addr, *market.address()))
}

/// Mint MockUSDC to `to` (local/dev only).
pub async fn mint_mock_usdc<P>(
    provider: &P,
    usdc: Address,
    to: Address,
    amount: U256,
) -> Result<B256>
where
    P: Provider + Clone,
{
    let token = MockUSDC::new(usdc, provider);
    let pending = token.mint(to, amount).send().await?;
    let receipt = pending.get_receipt().await?;
    Ok(receipt.transaction_hash())
}

/// Build an HTTP+wallet provider from a signer and RPC URL (tests / scripts).
pub fn http_provider_with_signer(
    rpc_url: &str,
    signer: PrivateKeySigner,
) -> Result<impl Provider + Clone> {
    let url: alloy::transports::http::reqwest::Url = rpc_url
        .parse()
        .map_err(|e| ChainError::Other(anyhow::anyhow!("invalid rpc url: {e}")))?;
    let wallet = alloy::network::EthereumWallet::from(signer);
    Ok(ProviderBuilder::new().wallet(wallet).connect_http(url))
}
