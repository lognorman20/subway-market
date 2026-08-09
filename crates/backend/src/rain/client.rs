//! HTTP Rain client (`Api-Key` header).

use async_trait::async_trait;
use reqwest::StatusCode;
use thiserror::Error;

use crate::funding::RainConfig;

use super::types::{
    CreatePaymentRouteRequest, OnchainAddress, OnrampDestination, PaymentRoute, RailCurrency,
    RainTransaction, SimulatePaymentRouteRequest, SimulatePaymentRouteResponse,
};

#[derive(Debug, Error)]
pub enum RainClientError {
    #[error("rain http: {0}")]
    Http(#[from] reqwest::Error),
    #[error("rain api {status}: {body}")]
    Api { status: StatusCode, body: String },
    #[error("rain decode: {0}")]
    Decode(String),
    #[error("rain misconfigured: {0}")]
    Config(String),
}

#[async_trait]
pub trait RainApi: Send + Sync {
    async fn create_payment_route(
        &self,
        req: CreatePaymentRouteRequest,
    ) -> Result<PaymentRoute, RainClientError>;

    async fn list_payment_routes(&self) -> Result<Vec<PaymentRoute>, RainClientError>;

    async fn simulate_payment_route(
        &self,
        req: SimulatePaymentRouteRequest,
    ) -> Result<SimulatePaymentRouteResponse, RainClientError>;

    async fn get_transaction(&self, id: &str) -> Result<RainTransaction, RainClientError>;

    async fn list_transfer_transactions(
        &self,
        limit: u32,
    ) -> Result<Vec<RainTransaction>, RainClientError>;

    fn config(&self) -> &RainConfig;
}

#[derive(Clone)]
pub struct HttpRainClient {
    http: reqwest::Client,
    cfg: RainConfig,
}

impl HttpRainClient {
    pub fn new(cfg: RainConfig) -> Self {
        Self {
            http: reqwest::Client::new(),
            cfg,
        }
    }

    fn url(&self, path: &str) -> String {
        let base = self.cfg.api_base_url.trim_end_matches('/');
        let path = path.trim_start_matches('/');
        format!("{base}/{path}")
    }

    async fn send_json<T: serde::de::DeserializeOwned>(
        &self,
        builder: reqwest::RequestBuilder,
    ) -> Result<T, RainClientError> {
        let res = builder.header("Api-Key", &self.cfg.api_key).send().await?;
        let status = res.status();
        let bytes = res.bytes().await?;
        if !status.is_success() {
            return Err(RainClientError::Api {
                status,
                body: String::from_utf8_lossy(&bytes).into_owned(),
            });
        }
        serde_json::from_slice(&bytes).map_err(|e| RainClientError::Decode(e.to_string()))
    }
}

#[async_trait]
impl RainApi for HttpRainClient {
    async fn create_payment_route(
        &self,
        req: CreatePaymentRouteRequest,
    ) -> Result<PaymentRoute, RainClientError> {
        self.send_json(
            self.http
                .post(self.url("/payment-routes"))
                .header("Content-Type", "application/json")
                .json(&req),
        )
        .await
    }

    async fn list_payment_routes(&self) -> Result<Vec<PaymentRoute>, RainClientError> {
        self.send_json(self.http.get(self.url("/payment-routes")))
            .await
    }

    async fn simulate_payment_route(
        &self,
        req: SimulatePaymentRouteRequest,
    ) -> Result<SimulatePaymentRouteResponse, RainClientError> {
        self.send_json(
            self.http
                .post(self.url("/simulate/payment-routes"))
                .header("Content-Type", "application/json")
                .json(&req),
        )
        .await
    }

    async fn get_transaction(&self, id: &str) -> Result<RainTransaction, RainClientError> {
        self.send_json(
            self.http
                .get(self.url(&format!("/issuing/transactions/{id}"))),
        )
        .await
    }

    async fn list_transfer_transactions(
        &self,
        limit: u32,
    ) -> Result<Vec<RainTransaction>, RainClientError> {
        let url = format!(
            "{}?type=transfer&limit={limit}&userId={}",
            self.url("/issuing/transactions"),
            self.cfg.user_id
        );
        self.send_json(self.http.get(url)).await
    }

    fn config(&self) -> &RainConfig {
        &self.cfg
    }
}

/// Ensure active ACH→USDC onramp route (create if missing).
pub async fn ensure_ach_usdc_onramp(
    client: &dyn RainApi,
) -> Result<PaymentRoute, RainClientError> {
    let cfg = client.config();
    let existing = client.list_payment_routes().await?;
    if let Some(route) = existing.into_iter().find(is_active_ach_usdc_onramp) {
        return Ok(route);
    }

    let req = CreatePaymentRouteRequest {
        user_id: cfg.user_id.clone(),
        source: RailCurrency {
            currency: "usd".into(),
            rail: "ach".into(),
        },
        destination: OnrampDestination {
            currency: "usdc".into(),
            rail: cfg.destination_rail.clone(),
            address: OnchainAddress {
                address_type: "onchain".into(),
                address: cfg.destination_address.clone(),
            },
        },
    };

    match client.create_payment_route(req).await {
        Ok(route) => Ok(route),
        Err(RainClientError::Api { status, body }) if status.as_u16() == 400 => {
            // Duplicate — re-list.
            tracing::warn!(%body, "create payment route 400; re-listing");
            let existing = client.list_payment_routes().await?;
            existing
                .into_iter()
                .find(is_active_ach_usdc_onramp)
                .ok_or_else(|| RainClientError::Config(format!("no onramp after 400: {body}")))
        }
        Err(e) => Err(e),
    }
}

fn is_active_ach_usdc_onramp(route: &PaymentRoute) -> bool {
    route.status.eq_ignore_ascii_case("active")
        && route.source.currency.eq_ignore_ascii_case("usd")
        && route.source.rail.eq_ignore_ascii_case("ach")
        && route
            .destination
            .get("currency")
            .and_then(|v| v.as_str())
            .is_some_and(|c| c.eq_ignore_ascii_case("usdc"))
}
