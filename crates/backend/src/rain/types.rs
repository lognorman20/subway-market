//! Rain API request/response shapes (subset needed for ACH onramp).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatePaymentRouteRequest {
    pub user_id: String,
    pub source: RailCurrency,
    pub destination: OnrampDestination,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RailCurrency {
    pub currency: String,
    pub rail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnrampDestination {
    pub currency: String,
    pub rail: String,
    pub address: OnchainAddress,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnchainAddress {
    #[serde(rename = "type")]
    pub address_type: String,
    pub address: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaymentRoute {
    pub id: String,
    #[serde(default)]
    pub user_id: Option<String>,
    pub status: String,
    pub source: RailCurrency,
    pub destination: serde_json::Value,
    #[serde(default)]
    pub deposit_address: Option<serde_json::Value>,
}

/// GET `/payment-routes` body. Sandbox returns `{ paymentRoutes, automations }`;
/// wiremock/legacy may still send a bare array.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum PaymentRoutesListBody {
    Wrapped {
        #[serde(rename = "paymentRoutes", default)]
        payment_routes: Vec<PaymentRoute>,
    },
    Bare(Vec<PaymentRoute>),
}

impl PaymentRoutesListBody {
    pub fn into_routes(self) -> Vec<PaymentRoute> {
        match self {
            Self::Wrapped { payment_routes, .. } => payment_routes,
            Self::Bare(routes) => routes,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SimulatePaymentRouteRequest {
    pub payment_route_id: String,
    /// Major units decimal string (e.g. `"100"`).
    pub amount: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SimulatePaymentRouteResponse {
    /// Present on some Rain responses; sandbox often omits and returns `{ "success": true }`.
    #[serde(default)]
    pub simulation_id: Option<String>,
    /// Sandbox simulate (`POST /simulate/payment-routes`) returns HTTP 202 + `{ "success": true }`.
    #[serde(default)]
    pub success: Option<bool>,
    #[serde(default)]
    pub flow: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
}

impl SimulatePaymentRouteResponse {
    /// Accepted when Rain returns a simulation id and/or explicit `success: true`.
    pub fn accepted(&self) -> bool {
        self.simulation_id.as_ref().is_some_and(|s| !s.is_empty())
            || self.success == Some(true)
    }
}

/// Transfer status from Rain `TransferDetails.status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransferStatus {
    Pending,
    AwaitingTransfer,
    Processing,
    Settled,
    Failed,
    Cancelled,
    Expired,
    Refunded,
    /// Webhook `action: completed` (maps to settled credit).
    Completed,
}

impl TransferStatus {
    pub fn is_terminal_success(self) -> bool {
        matches!(self, Self::Settled | Self::Completed)
    }

    pub fn is_terminal_failure(self) -> bool {
        matches!(
            self,
            Self::Failed | Self::Cancelled | Self::Expired | Self::Refunded
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RainTransaction {
    pub id: String,
    #[serde(rename = "type")]
    pub tx_type: String,
    #[serde(default)]
    pub transfer: Option<TransferDetails>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferDetails {
    pub status: TransferStatus,
    #[serde(default)]
    pub source: Option<TransferLeg>,
    #[serde(default)]
    pub destination: Option<TransferLeg>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferLeg {
    #[serde(default)]
    pub currency: Option<String>,
    #[serde(default)]
    pub rail: Option<String>,
    /// Decimal major-units string.
    #[serde(default)]
    pub amount: Option<String>,
}

impl RainTransaction {
    pub fn transfer_status(&self) -> Option<TransferStatus> {
        self.transfer.as_ref().map(|t| t.status)
    }

    /// Prefer destination (USDC received), else source amount.
    pub fn amount_major_str(&self) -> Option<&str> {
        let t = self.transfer.as_ref()?;
        t.destination
            .as_ref()
            .and_then(|d| d.amount.as_deref())
            .or_else(|| t.source.as_ref().and_then(|s| s.amount.as_deref()))
    }
}

/// Parse Rain major-unit decimal string → micro-USDC (6 decimals).
pub fn major_decimal_to_micro(s: &str) -> Option<i64> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let (whole, frac) = match s.split_once('.') {
        Some((w, f)) => (w, f),
        None => (s, ""),
    };
    if whole.is_empty() || !whole.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    if frac.chars().any(|c| !c.is_ascii_digit()) {
        return None;
    }
    let whole_i: i64 = whole.parse().ok()?;
    let mut frac_digits = frac.chars().take(6).collect::<String>();
    while frac_digits.len() < 6 {
        frac_digits.push('0');
    }
    let frac_i: i64 = frac_digits.parse().ok()?;
    whole_i.checked_mul(1_000_000)?.checked_add(frac_i)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payment_routes_list_accepts_sandbox_map() {
        let raw = r#"{"paymentRoutes":[{"id":"a1b2c3d4-e5f6-7890-abcd-ef1234567890","userId":"11111111-1111-1111-1111-111111111111","status":"active","source":{"currency":"usd","rail":"ach"},"destination":{"currency":"usdc","rail":"base"},"depositAddress":{"type":"fiat"}}],"automations":[]}"#;
        let body: PaymentRoutesListBody = serde_json::from_str(raw).expect("decode map");
        let routes = body.into_routes();
        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0].id, "a1b2c3d4-e5f6-7890-abcd-ef1234567890");
    }

    #[test]
    fn payment_routes_list_accepts_bare_array() {
        let raw = r#"[]"#;
        let body: PaymentRoutesListBody = serde_json::from_str(raw).expect("decode array");
        assert!(body.into_routes().is_empty());
    }

    #[test]
    fn simulate_response_accepts_sandbox_success_only() {
        let body: SimulatePaymentRouteResponse =
            serde_json::from_str(r#"{"success":true}"#).expect("decode success");
        assert!(body.accepted());
        assert!(body.simulation_id.is_none());
        assert_eq!(body.success, Some(true));
    }

    #[test]
    fn simulate_response_accepts_simulation_id() {
        let body: SimulatePaymentRouteResponse =
            serde_json::from_str(r#"{"simulationId":"sim_abc","status":"accepted"}"#)
                .expect("decode sim id");
        assert!(body.accepted());
        assert_eq!(body.simulation_id.as_deref(), Some("sim_abc"));
    }
}
