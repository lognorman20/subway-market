//! HTTP client for backend E2E calls.

use anyhow::{anyhow, bail, Context, Result};
use dto::{
    AgentId, AgentMarketPnl, AgentPnlResponse, AuthChallengeResponse, AuthVerifyResponse,
    BankrollResponse, BuyRequest, CreateAgentResponse, MarketDetailResponse, MarketSide,
    MarketsListResponse, SellRequest, TradeResponse,
};
use reqwest::{Client, StatusCode};
use serde::Deserialize;
use serde_json::json;
use std::time::Duration;

use crate::config::BackendConfig;

#[derive(Clone)]
pub struct BackendClient {
    http: Client,
    base: String,
}

impl BackendClient {
    pub fn new(cfg: &BackendConfig) -> Result<Self> {
        // Per-request timeout: allow slow chain-busy backend without failing wait loops.
        let req_timeout = Duration::from_secs(30).min(cfg.timeout.max(Duration::from_secs(30)));
        let http = Client::builder()
            .timeout(req_timeout)
            .connect_timeout(Duration::from_secs(5))
            .build()
            .context("build reqwest")?;
        Ok(Self {
            http,
            base: cfg.base_url.trim_end_matches('/').to_string(),
        })
    }

    pub fn base_url(&self) -> &str {
        &self.base
    }

    pub fn http(&self) -> &Client {
        &self.http
    }

    pub async fn health(&self) -> Result<()> {
        let res = self.http.get(format!("{}/health", self.base)).send().await?;
        if !res.status().is_success() {
            bail!("health {}", res.status());
        }
        Ok(())
    }

    pub async fn llms_txt(&self) -> Result<String> {
        let res = self.http.get(format!("{}/llms.txt", self.base)).send().await?;
        if !res.status().is_success() {
            bail!("llms.txt {}", res.status());
        }
        Ok(res.text().await?)
    }

    pub async fn auth_challenge(&self, address: &str) -> Result<AuthChallengeResponse> {
        let res = self
            .http
            .post(format!("{}/auth/challenge", self.base))
            .json(&json!({ "address": address }))
            .send()
            .await?;
        parse_json(res).await
    }

    pub async fn auth_verify(
        &self,
        challenge_id: &str,
        address: &str,
        signature: &str,
    ) -> Result<AuthVerifyResponse> {
        let res = self
            .http
            .post(format!("{}/auth/verify", self.base))
            .json(&json!({
                "challenge_id": challenge_id,
                "address": address,
                "signature": signature,
            }))
            .send()
            .await?;
        parse_json(res).await
    }

    pub async fn create_agent(&self, session: &str, display_name: Option<&str>) -> Result<CreateAgentResponse> {
        let body = match display_name {
            Some(n) => json!({ "display_name": n }),
            None => json!({}),
        };
        let res = self
            .http
            .post(format!("{}/agents", self.base))
            .header("authorization", format!("Bearer {session}"))
            .json(&body)
            .send()
            .await?;
        parse_json(res).await
    }

    pub async fn seed_bankroll(
        &self,
        session: &str,
        agent_id: &str,
        amount_usdc: Option<i64>,
    ) -> Result<BankrollResponse> {
        let mut body = json!({ "agent_id": agent_id });
        if let Some(a) = amount_usdc {
            body["amount_usdc"] = json!(a);
        }
        let res = self
            .http
            .post(format!("{}/bankroll/seed", self.base))
            .header("authorization", format!("Bearer {session}"))
            .json(&body)
            .send()
            .await?;
        parse_json(res).await
    }

    pub async fn bankroll(&self, bearer: &str, agent_id: &str) -> Result<BankrollResponse> {
        let res = self
            .http
            .get(format!("{}/agents/{agent_id}/bankroll", self.base))
            .header("authorization", format!("Bearer {bearer}"))
            .send()
            .await?;
        parse_json(res).await
    }

    pub async fn list_markets(&self) -> Result<MarketsListResponse> {
        let res = self.http.get(format!("{}/markets", self.base)).send().await?;
        parse_json(res).await
    }

    pub async fn get_market(&self, market_id: &str) -> Result<MarketDetailResponse> {
        let res = self
            .http
            .get(format!("{}/markets/{market_id}", self.base))
            .send()
            .await?;
        parse_json(res).await
    }

    pub async fn buy(
        &self,
        bearer: &str,
        market_id: &str,
        side: MarketSide,
        shares: &str,
        agent_id: Option<&str>,
    ) -> Result<TradeResponse> {
        let req = BuyRequest {
            market_id: market_id.to_string(),
            side,
            shares: shares.to_string(),
            agent_id: agent_id.map(|s| AgentId(s.to_string())),
        };
        let res = self
            .http
            .post(format!("{}/buy", self.base))
            .header("authorization", format!("Bearer {bearer}"))
            .json(&req)
            .send()
            .await?;
        parse_json(res).await
    }

    pub async fn sell(
        &self,
        bearer: &str,
        market_id: &str,
        side: MarketSide,
        shares: &str,
        agent_id: Option<&str>,
    ) -> Result<TradeResponse> {
        let req = SellRequest {
            market_id: market_id.to_string(),
            side,
            shares: shares.to_string(),
            agent_id: agent_id.map(|s| AgentId(s.to_string())),
        };
        let res = self
            .http
            .post(format!("{}/sell", self.base))
            .header("authorization", format!("Bearer {bearer}"))
            .json(&req)
            .send()
            .await?;
        parse_json(res).await
    }

    pub async fn pnl(&self, bearer: &str) -> Result<AgentPnlResponse> {
        let res = self
            .http
            .get(format!("{}/pnl", self.base))
            .header("authorization", format!("Bearer {bearer}"))
            .send()
            .await?;
        parse_json(res).await
    }

    pub async fn agent_market_pnl(
        &self,
        bearer: &str,
        agent_id: &str,
        market_id: &str,
    ) -> Result<AgentMarketPnl> {
        let res = self
            .http
            .get(format!("{}/pnl/{agent_id}/{market_id}", self.base))
            .header("authorization", format!("Bearer {bearer}"))
            .send()
            .await?;
        parse_json(res).await
    }

    pub async fn fund_rain(
        &self,
        session: &str,
        agent_id: &str,
        amount_usdc: i64,
    ) -> Result<FundRainResponse> {
        let res = self
            .http
            .post(format!("{}/funding/rain", self.base))
            .header("authorization", format!("Bearer {session}"))
            .json(&json!({
                "agent_id": agent_id,
                "amount_usdc": amount_usdc,
            }))
            .send()
            .await?;
        parse_json(res).await
    }

    pub async fn poll_funding(&self, session: &str, funding_id: &str) -> Result<FundingStatusResponse> {
        let res = self
            .http
            .post(format!("{}/funding/rain/{funding_id}/poll", self.base))
            .header("authorization", format!("Bearer {session}"))
            .send()
            .await?;
        parse_json(res).await
    }
}

#[derive(Debug, Deserialize)]
pub struct FundRainResponse {
    pub funding_id: String,
    pub status: String,
    pub amount_usdc: i64,
    pub amount_micro: i64,
}

#[derive(Debug, Deserialize)]
pub struct FundingStatusResponse {
    pub funding_id: String,
    pub status: String,
    pub bankroll_usdc: Option<i64>,
    pub amount_micro: i64,
}

async fn parse_json<T: for<'de> Deserialize<'de>>(res: reqwest::Response) -> Result<T> {
    let status = res.status();
    let bytes = res.bytes().await.context("read body")?;
    if !status.is_success() {
        let raw = String::from_utf8_lossy(&bytes);
        return Err(anyhow!("HTTP {status}: {raw}"));
    }
    serde_json::from_slice(&bytes).with_context(|| {
        format!(
            "decode JSON ({status}): {}",
            String::from_utf8_lossy(&bytes)
        )
    })
}

/// Convenience: treat StatusCode for callers that need raw checks.
#[allow(dead_code)]
pub fn is_ok(status: StatusCode) -> bool {
    status.is_success()
}
