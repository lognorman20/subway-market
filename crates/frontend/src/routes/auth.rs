//! Auth proxy — challenge/verify against backend; session cookie for humans only.

use axum::{
    extract::State,
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Json, Response},
    routing::{get, post},
    Router,
};
use serde::Deserialize;
use serde_json::json;

use crate::backend::{AppState, BackendFetch};
use crate::session::{
    clear_session_cookie_header, session_token_from_headers, set_session_cookie_header,
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/auth/challenge", post(challenge))
        .route("/auth/verify", post(verify))
        .route("/auth/logout", post(logout))
        .route("/auth/session", get(session_status))
}

#[derive(Debug, Deserialize)]
struct ChallengeBody {
    address: String,
}

#[derive(Debug, Deserialize)]
struct VerifyBody {
    challenge_id: String,
    address: String,
    signature: String,
}

async fn challenge(
    State(state): State<AppState>,
    Json(body): Json<ChallengeBody>,
) -> Response {
    match state.auth_challenge(body.address.trim()).await {
        BackendFetch::Ok(resp) => Json(resp).into_response(),
        BackendFetch::HttpError(status, msg) => json_err(status, msg),
        BackendFetch::Unreachable(err) => json_err(502, format!("backend unreachable: {err}")),
        BackendFetch::ParseError(err) => json_err(502, format!("parse error: {err}")),
        BackendFetch::Empty => json_err(502, "empty challenge response".into()),
    }
}

async fn verify(State(state): State<AppState>, Json(body): Json<VerifyBody>) -> Response {
    match state
        .auth_verify(
            body.challenge_id.trim(),
            body.address.trim(),
            body.signature.trim(),
        )
        .await
    {
        BackendFetch::Ok(resp) => {
            let mut response = Json(json!({
                "wallet": resp.wallet,
                "expires_at": resp.expires_at.to_rfc3339(),
                "ok": true,
            }))
            .into_response();
            response.headers_mut().insert(
                header::SET_COOKIE,
                set_session_cookie_header(&resp.session_token),
            );
            response
        }
        BackendFetch::HttpError(status, msg) => json_err(status, msg),
        BackendFetch::Unreachable(err) => json_err(502, format!("backend unreachable: {err}")),
        BackendFetch::ParseError(err) => json_err(502, format!("parse error: {err}")),
        BackendFetch::Empty => json_err(502, "empty verify response".into()),
    }
}

async fn logout() -> Response {
    let mut response = Json(json!({ "ok": true })).into_response();
    response
        .headers_mut()
        .insert(header::SET_COOKIE, clear_session_cookie_header());
    response
}

async fn session_status(headers: HeaderMap) -> Response {
    match session_token_from_headers(&headers) {
        Some(token) if token.starts_with("sms_") => Json(json!({
            "authenticated": true,
            "has_session": true,
        }))
        .into_response(),
        _ => Json(json!({
            "authenticated": false,
            "has_session": false,
        }))
        .into_response(),
    }
}

fn json_err(status: u16, msg: String) -> Response {
    let code = StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_GATEWAY);
    (code, Json(json!({ "error": msg, "ok": false }))).into_response()
}
