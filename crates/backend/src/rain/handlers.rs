//! HTTP handlers: initiate Rain fund, webhook, poll settle.

use axum::{
    extract::{Path, State},
    http::HeaderMap,
    Json,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::agents::{credit_bankroll_locked, ensure_human_owns_agent};
use crate::auth::{AuthIdentity, AuthUser};
use crate::error::ApiError;
use crate::funding::{whole_usdc_to_micro, FundingMode};
use crate::state::AppState;

use super::client::{ensure_ach_usdc_onramp, RainClientError};
use super::pending::{FundingStatus, PendingFunding, PendingFundingStore};
use super::poll::{poll_until_settled, PollOutcome, SettlePollConfig};
use super::types::{
    major_decimal_to_micro, SimulatePaymentRouteRequest, TransferStatus,
};

#[derive(Debug, Deserialize)]
pub struct FundRainRequest {
    pub agent_id: String,
    /// Whole USDC to on-ramp (min 2 for Rain sandbox simulate).
    pub amount_usdc: i64,
}

#[derive(Debug, Serialize)]
pub struct FundRainResponse {
    pub funding_id: String,
    pub payment_route_id: String,
    pub simulation_id: Option<String>,
    pub amount_usdc: i64,
    pub amount_micro: i64,
    pub status: FundingStatus,
    pub deposit_address: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
pub struct FundingStatusResponse {
    pub funding_id: String,
    pub agent_id: String,
    pub amount_usdc: i64,
    pub amount_micro: i64,
    pub status: FundingStatus,
    pub payment_route_id: String,
    pub simulation_id: Option<String>,
    pub rain_tx_id: Option<String>,
    pub bankroll_usdc: Option<i64>,
}

/// `POST /funding/rain` — human session; ACH→USDC onramp + sandbox simulate.
pub async fn fund_rain(
    State(state): State<AppState>,
    AuthUser(identity): AuthUser,
    Json(req): Json<FundRainRequest>,
) -> Result<Json<FundRainResponse>, ApiError> {
    require_testnet(&state)?;
    let rain = state
        .rain
        .as_ref()
        .ok_or_else(|| ApiError::Unavailable("Rain client not configured".into()))?;

    let AuthIdentity::Human { wallet } = &identity else {
        return Err(ApiError::Forbidden(
            "only human session may initiate Rain funding".into(),
        ));
    };

    if req.amount_usdc < 2 {
        return Err(ApiError::BadRequest(
            "amount_usdc must be >= 2 (Rain sandbox minimum)".into(),
        ));
    }

    ensure_human_owns_agent(&state, wallet, &req.agent_id).await?;

    let route = ensure_ach_usdc_onramp(rain.as_ref())
        .await
        .map_err(map_rain_err)?;

    let sim = rain
        .simulate_payment_route(SimulatePaymentRouteRequest {
            payment_route_id: route.id.clone(),
            amount: req.amount_usdc.to_string(),
        })
        .await
        .map_err(map_rain_err)?;

    let funding_id = PendingFundingStore::new_id();
    let amount_micro = whole_usdc_to_micro(req.amount_usdc);
    let pending = PendingFunding {
        id: funding_id.clone(),
        agent_id: req.agent_id,
        owner_wallet: wallet.clone(),
        amount_usdc: req.amount_usdc,
        amount_micro,
        payment_route_id: route.id.clone(),
        simulation_id: Some(sim.simulation_id.clone()),
        rain_tx_id: None,
        status: FundingStatus::Pending,
        created_at: Utc::now(),
    };
    state.pending_funding.insert(pending).await;

    info!(
        %funding_id,
        route_id = %route.id,
        simulation_id = %sim.simulation_id,
        amount = req.amount_usdc,
        "rain funding initiated"
    );

    Ok(Json(FundRainResponse {
        funding_id,
        payment_route_id: route.id,
        simulation_id: Some(sim.simulation_id),
        amount_usdc: req.amount_usdc,
        amount_micro,
        status: FundingStatus::Pending,
        deposit_address: route.deposit_address,
    }))
}

/// `GET /funding/rain/{id}`
pub async fn get_funding(
    State(state): State<AppState>,
    AuthUser(identity): AuthUser,
    Path(id): Path<String>,
) -> Result<Json<FundingStatusResponse>, ApiError> {
    require_testnet(&state)?;
    let funding = state
        .pending_funding
        .get(&id)
        .await
        .ok_or_else(|| ApiError::NotFound(format!("funding {id} not found")))?;
    ensure_funding_access(&identity, &funding)?;

    let bankroll = if funding.status == FundingStatus::Settled {
        let (b, _) = crate::agents::load_bankroll(&state, &funding.agent_id).await?;
        Some(b)
    } else {
        None
    };

    Ok(Json(status_response(&funding, bankroll)))
}

/// `POST /funding/rain/{id}/poll` — bounded backoff until settled/failed/timeout.
pub async fn poll_funding(
    State(state): State<AppState>,
    AuthUser(identity): AuthUser,
    Path(id): Path<String>,
) -> Result<Json<FundingStatusResponse>, ApiError> {
    require_testnet(&state)?;
    let rain = state
        .rain
        .as_ref()
        .ok_or_else(|| ApiError::Unavailable("Rain client not configured".into()))?;

    let funding = state
        .pending_funding
        .get(&id)
        .await
        .ok_or_else(|| ApiError::NotFound(format!("funding {id} not found")))?;
    ensure_funding_access(&identity, &funding)?;

    if funding.status == FundingStatus::Settled {
        let (b, _) = crate::agents::load_bankroll(&state, &funding.agent_id).await?;
        return Ok(Json(status_response(&funding, Some(b))));
    }
    if funding.status == FundingStatus::Failed {
        return Ok(Json(status_response(&funding, None)));
    }

    let poll_cfg = state
        .settle_poll
        .clone()
        .unwrap_or_else(SettlePollConfig::default);

    let outcome = poll_until_settled(rain.as_ref(), &state.pending_funding, &funding, &poll_cfg)
        .await
        .map_err(map_rain_err)?;

    match outcome {
        PollOutcome::Settled { tx } => {
            let bankroll = apply_settlement(&state, &funding.id, &tx.id, funding.amount_micro).await?;
            let updated = state
                .pending_funding
                .get(&id)
                .await
                .unwrap_or(funding);
            Ok(Json(status_response(&updated, Some(bankroll))))
        }
        PollOutcome::Failed { tx } => {
            state
                .pending_funding
                .update(&id, |f| {
                    f.status = FundingStatus::Failed;
                    f.rain_tx_id = Some(tx.id.clone());
                })
                .await;
            let updated = state.pending_funding.get(&id).await.unwrap_or(funding);
            Ok(Json(status_response(&updated, None)))
        }
        PollOutcome::TimedOut => Err(ApiError::Unavailable(
            "Rain settlement poll timed out; retry or wait for webhook".into(),
        )),
    }
}

/// `POST /webhooks/rain` — Rain transfer webhooks → credit bankroll.
pub async fn rain_webhook(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<serde_json::Value>, ApiError> {
    if state.funding.mode != FundingMode::Testnet {
        return Err(ApiError::Forbidden(
            "Rain webhook only when FUNDING_MODE=testnet".into(),
        ));
    }

    // Optional Api-Key check when Rain client configured.
    if let Some(rain) = state.rain.as_ref() {
        let expected = &rain.config().api_key;
        let provided = headers
            .get("Api-Key")
            .or_else(|| headers.get("api-key"))
            .and_then(|v| v.to_str().ok());
        if let Some(key) = provided {
            if key != expected {
                return Err(ApiError::Unauthorized("invalid Api-Key".into()));
            }
        }
    }

    let payload: serde_json::Value = serde_json::from_slice(&body).map_err(|e| {
        ApiError::BadRequest(format!("invalid webhook json: {e}"))
    })?;

    let parsed = parse_webhook(&payload);
    let Some(parsed) = parsed else {
        info!("rain webhook ignored (unrecognized shape)");
        return Ok(Json(serde_json::json!({ "ok": true, "handled": false })));
    };

    if !parsed.status.is_terminal_success() {
        if parsed.status.is_terminal_failure() {
            if let Some(funding) = match_pending(&state.pending_funding, &parsed).await {
                state
                    .pending_funding
                    .update(&funding.id, |f| {
                        f.status = FundingStatus::Failed;
                        f.rain_tx_id = Some(parsed.tx_id.clone());
                    })
                    .await;
            }
        }
        return Ok(Json(serde_json::json!({
            "ok": true,
            "handled": false,
            "status": format!("{:?}", parsed.status)
        })));
    }

    let funding = match_pending(&state.pending_funding, &parsed)
        .await
        .ok_or_else(|| {
            warn!(tx_id = %parsed.tx_id, "rain webhook: no matching pending funding");
            ApiError::NotFound("no matching pending funding".into())
        })?;

    let amount_micro = parsed
        .amount_micro
        .unwrap_or(funding.amount_micro);

    let bankroll = apply_settlement(&state, &funding.id, &parsed.tx_id, amount_micro).await?;

    Ok(Json(serde_json::json!({
        "ok": true,
        "handled": true,
        "funding_id": funding.id,
        "bankroll_usdc": bankroll
    })))
}

struct ParsedWebhook {
    tx_id: String,
    status: TransferStatus,
    amount_micro: Option<i64>,
    simulation_id: Option<String>,
}

fn parse_webhook(payload: &serde_json::Value) -> Option<ParsedWebhook> {
    // Nested data / resource shapes.
    let root = payload
        .get("data")
        .or_else(|| payload.get("resource"))
        .unwrap_or(payload);

    let tx_id = root
        .get("id")
        .or_else(|| payload.get("id"))
        .or_else(|| payload.get("transactionId"))
        .and_then(|v| v.as_str())?
        .to_string();

    let transfer = root.get("transfer").unwrap_or(root);

    let status = transfer
        .get("status")
        .and_then(|v| v.as_str())
        .or_else(|| payload.get("action").and_then(|v| v.as_str()))
        .and_then(|s| serde_json::from_value(serde_json::Value::String(s.to_string())).ok())
        .or_else(|| {
            // action: completed → Settled
            payload.get("action").and_then(|a| a.as_str()).and_then(|a| {
                if a.eq_ignore_ascii_case("completed") {
                    Some(TransferStatus::Completed)
                } else {
                    None
                }
            })
        })?;

    let amount_str = transfer
        .pointer("/destination/amount")
        .or_else(|| transfer.pointer("/source/amount"))
        .or_else(|| root.get("amount"))
        .and_then(|v| v.as_str().map(str::to_string).or_else(|| v.as_i64().map(|n| n.to_string())));

    let amount_micro = amount_str.as_deref().and_then(major_decimal_to_micro);

    let simulation_id = payload
        .get("simulationId")
        .or_else(|| root.get("simulationId"))
        .and_then(|v| v.as_str())
        .map(str::to_string);

    Some(ParsedWebhook {
        tx_id,
        status,
        amount_micro,
        simulation_id,
    })
}

async fn match_pending(
    store: &PendingFundingStore,
    parsed: &ParsedWebhook,
) -> Option<PendingFunding> {
    if let Some(sim) = parsed.simulation_id.as_deref() {
        if let Some(f) = store.find_pending_by_simulation(sim).await {
            return Some(f);
        }
    }
    if let Some(micro) = parsed.amount_micro {
        if let Some(f) = store.find_pending_by_amount(micro).await {
            return Some(f);
        }
    }
    None
}

async fn apply_settlement(
    state: &AppState,
    funding_id: &str,
    rain_tx_id: &str,
    amount_micro: i64,
) -> Result<i64, ApiError> {
    if state.pending_funding.already_credited(rain_tx_id).await {
        let funding = state
            .pending_funding
            .get(funding_id)
            .await
            .ok_or_else(|| ApiError::NotFound(format!("funding {funding_id} not found")))?;
        let (b, _) = crate::agents::load_bankroll(state, &funding.agent_id).await?;
        return Ok(b);
    }

    let funding = state
        .pending_funding
        .get(funding_id)
        .await
        .ok_or_else(|| ApiError::NotFound(format!("funding {funding_id} not found")))?;

    if funding.status == FundingStatus::Settled {
        let (b, _) = crate::agents::load_bankroll(state, &funding.agent_id).await?;
        return Ok(b);
    }

    let bankroll = credit_bankroll_locked(state, &funding.agent_id, amount_micro).await?;

    state
        .pending_funding
        .update(funding_id, |f| {
            f.status = FundingStatus::Settled;
            f.rain_tx_id = Some(rain_tx_id.to_string());
        })
        .await;
    state
        .pending_funding
        .mark_credited(rain_tx_id, funding_id)
        .await;

    info!(
        %funding_id,
        %rain_tx_id,
        agent_id = %funding.agent_id,
        amount_micro,
        bankroll,
        "rain funding settled — bankroll credited"
    );

    Ok(bankroll)
}

fn require_testnet(state: &AppState) -> Result<(), ApiError> {
    if state.funding.mode != FundingMode::Testnet {
        return Err(ApiError::Forbidden(
            "Rain funding only when FUNDING_MODE=testnet".into(),
        ));
    }
    Ok(())
}

fn ensure_funding_access(
    identity: &AuthIdentity,
    funding: &PendingFunding,
) -> Result<(), ApiError> {
    match identity {
        AuthIdentity::Human { wallet } if wallet.eq_ignore_ascii_case(&funding.owner_wallet) => {
            Ok(())
        }
        AuthIdentity::Agent { agent_id, .. } if agent_id == &funding.agent_id => Ok(()),
        _ => Err(ApiError::Forbidden("not allowed to view this funding".into())),
    }
}

fn status_response(funding: &PendingFunding, bankroll: Option<i64>) -> FundingStatusResponse {
    FundingStatusResponse {
        funding_id: funding.id.clone(),
        agent_id: funding.agent_id.clone(),
        amount_usdc: funding.amount_usdc,
        amount_micro: funding.amount_micro,
        status: funding.status,
        payment_route_id: funding.payment_route_id.clone(),
        simulation_id: funding.simulation_id.clone(),
        rain_tx_id: funding.rain_tx_id.clone(),
        bankroll_usdc: bankroll,
    }
}

fn map_rain_err(err: RainClientError) -> ApiError {
    match err {
        RainClientError::Api { status, body } if status.as_u16() == 401 => {
            ApiError::Unauthorized(format!("Rain auth failed: {body}"))
        }
        RainClientError::Api { status, body } if status.as_u16() == 404 => {
            ApiError::NotFound(format!("Rain resource: {body}"))
        }
        RainClientError::Api { status, body } => {
            ApiError::Unavailable(format!("Rain API {status}: {body}"))
        }
        RainClientError::Config(msg) => ApiError::Unavailable(msg),
        RainClientError::Http(e) => ApiError::Unavailable(format!("Rain HTTP: {e}")),
        RainClientError::Decode(e) => ApiError::Internal(format!("Rain decode: {e}")),
    }
}
