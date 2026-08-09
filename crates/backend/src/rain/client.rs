//! HTTP Rain client (`Api-Key` header).

use async_trait::async_trait;
use reqwest::StatusCode;
use thiserror::Error;

use crate::funding::RainConfig;

use super::types::{
    CreatePaymentRouteRequest, OnchainAddress, OnrampDestination, PaymentRoute,
    PaymentRoutesListBody, RailCurrency, RainTransaction, SimulatePaymentRouteRequest,
    SimulatePaymentRouteResponse,
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
        let body: PaymentRoutesListBody = self
            .send_json(self.http.get(self.url("/payment-routes")))
            .await?;
        Ok(body.into_routes())
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
///
/// Destination must pass Rain compliance (Base/Eth/… rails — **no Monad**). Prefer
/// `RAIN_DESTINATION_ADDRESS`; if create fails and an existing compliant onramp exists,
/// reuse that route's destination (discover from Rain API).
pub async fn ensure_ach_usdc_onramp(
    client: &dyn RainApi,
) -> Result<PaymentRoute, RainClientError> {
    let cfg = client.config();
    let existing = client.list_payment_routes().await?;

    // Prefer active onramp already pointing at configured destination.
    if let Some(route) = existing
        .iter()
        .find(|r| is_matching_ach_usdc_onramp(r, Some(&cfg.destination_address)))
        .cloned()
    {
        return Ok(route);
    }

    // Any active ACH→USDC onramp (discovered compliant dest from prior create).
    if let Some(route) = existing.iter().find(|r| is_matching_ach_usdc_onramp(r, None)).cloned()
    {
        let discovered = route_destination_address(&route);
        tracing::info!(
            configured = %cfg.destination_address,
            ?discovered,
            route_id = %route.id,
            "reusing existing Rain ACH→USDC onramp (destination discovered from Rain API)"
        );
        return Ok(route);
    }

    let dest = cfg.destination_address.clone();
    if crate::funding::is_anvil_default_address(&dest) {
        return Err(RainClientError::Config(format!(
            "RAIN_DESTINATION_ADDRESS is anvil #0 ({dest}); Rain compliance rejects it. \
             Set RAIN_DESTINATION_ADDRESS to a Base wallet that passes Rain checks \
             (sandbox tip: {})",
            crate::funding::RAIN_SANDBOX_COMPLIANT_DEST_EXAMPLE
        )));
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
                address: dest,
            },
        },
    };

    match client.create_payment_route(req).await {
        Ok(route) => Ok(route),
        Err(RainClientError::Api { status, body }) if status.as_u16() == 400 => {
            // Duplicate or compliance — re-list; reuse any active onramp if present.
            tracing::warn!(%body, "create payment route 400; re-listing");
            let existing = client.list_payment_routes().await?;
            if let Some(route) = existing
                .into_iter()
                .find(|r| is_matching_ach_usdc_onramp(r, None))
            {
                return Ok(route);
            }
            if body.to_lowercase().contains("compliance") {
                return Err(RainClientError::Config(format!(
                    "Rain destination failed compliance: {body}. Set RAIN_DESTINATION_ADDRESS \
                     to a compliant Base address (sandbox tip: {})",
                    crate::funding::RAIN_SANDBOX_COMPLIANT_DEST_EXAMPLE
                )));
            }
            Err(RainClientError::Config(format!("no onramp after 400: {body}")))
        }
        Err(e) => Err(e),
    }
}

fn is_matching_ach_usdc_onramp(route: &PaymentRoute, want_addr: Option<&str>) -> bool {
    if !route.status.eq_ignore_ascii_case("active") {
        return false;
    }
    if !route.source.currency.eq_ignore_ascii_case("usd")
        || !route.source.rail.eq_ignore_ascii_case("ach")
    {
        return false;
    }
    let currency_ok = route
        .destination
        .get("currency")
        .and_then(|v| v.as_str())
        .is_some_and(|c| c.eq_ignore_ascii_case("usdc"));
    if !currency_ok {
        return false;
    }
    match want_addr {
        None => true,
        Some(want) => route_destination_address(route)
            .is_some_and(|a| a.eq_ignore_ascii_case(want)),
    }
}

/// Extract on-chain destination address from a payment route JSON destination blob.
pub fn route_destination_address(route: &PaymentRoute) -> Option<String> {
    let dest = &route.destination;
    if let Some(s) = dest.get("address").and_then(|v| v.as_str()) {
        return Some(s.to_string());
    }
    dest.get("address")
        .and_then(|v| v.get("address"))
        .and_then(|v| v.as_str())
        .map(str::to_string)
}
