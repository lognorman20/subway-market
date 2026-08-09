//! Agent Bearer + human wallet session auth.

use std::collections::HashMap;
use std::sync::Arc;

use alloy::primitives::Address;
use alloy::signers::Signature;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use chrono::{DateTime, Duration, Utc};
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use rand::RngCore;
use sha2::{Digest, Sha256};
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::error::ApiError;
use crate::state::AppState;

pub const AGENT_KEY_PREFIX: &str = "smk_";
pub const SESSION_PREFIX: &str = "sms_";

const CHALLENGE_TTL_SECS: i64 = 300;
const SESSION_TTL_SECS: i64 = 86_400;

#[derive(Debug, Clone)]
pub enum AuthIdentity {
    /// Bearer `smk_*` — trade as this agent.
    Agent {
        agent_id: String,
        #[allow(dead_code)]
        owner_wallet: String,
    },
    /// Session from signed EVM challenge — owns agents under `wallet`.
    Human { wallet: String },
}

impl AuthIdentity {
    #[allow(dead_code)]
    pub fn wallet(&self) -> &str {
        match self {
            Self::Agent { owner_wallet, .. } => owner_wallet,
            Self::Human { wallet } => wallet,
        }
    }

    #[allow(dead_code)]
    pub fn agent_id(&self) -> Option<&str> {
        match self {
            Self::Agent { agent_id, .. } => Some(agent_id),
            Self::Human { .. } => None,
        }
    }
}

#[derive(Debug, Clone)]
struct Challenge {
    address: String,
    message: String,
    expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
struct Session {
    wallet: String,
    expires_at: DateTime<Utc>,
}

#[derive(Clone, Default)]
pub struct AuthStore {
    challenges: Arc<RwLock<HashMap<String, Challenge>>>,
    sessions: Arc<RwLock<HashMap<String, Session>>>,
}

impl AuthStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn issue_challenge(&self, address: &str) -> Result<(String, String, DateTime<Utc>), ApiError> {
        let wallet = normalize_address(address)?;
        let challenge_id = Uuid::new_v4().to_string();
        let nonce = Uuid::new_v4().to_string();
        let issued_at = Utc::now();
        let expires_at = issued_at + Duration::seconds(CHALLENGE_TTL_SECS);
        let message = format!(
            "subway-market login\nAddress: {wallet}\nNonce: {nonce}\nIssued At: {issued_at}\nExpiration Time: {expires_at}"
        );
        self.challenges.write().await.insert(
            challenge_id.clone(),
            Challenge {
                address: wallet,
                message: message.clone(),
                expires_at,
            },
        );
        Ok((challenge_id, message, expires_at))
    }

    pub async fn verify_challenge(
        &self,
        challenge_id: &str,
        address: &str,
        signature_hex: &str,
    ) -> Result<(String, String, DateTime<Utc>), ApiError> {
        let wallet = normalize_address(address)?;
        let challenge = {
            let mut map = self.challenges.write().await;
            map.remove(challenge_id)
                .ok_or_else(|| ApiError::BadRequest("unknown or used challenge_id".into()))?
        };
        if challenge.expires_at < Utc::now() {
            return Err(ApiError::BadRequest("challenge expired".into()));
        }
        if !eq_addr(&challenge.address, &wallet) {
            return Err(ApiError::BadRequest("address mismatch".into()));
        }
        let recovered = recover_signer(&challenge.message, signature_hex)?;
        if !eq_addr(&recovered, &wallet) {
            return Err(ApiError::Unauthorized("signature does not match address".into()));
        }
        let token_body = random_token();
        let session_token = format!("{SESSION_PREFIX}{token_body}");
        let expires_at = Utc::now() + Duration::seconds(SESSION_TTL_SECS);
        self.sessions.write().await.insert(
            session_token.clone(),
            Session {
                wallet: wallet.clone(),
                expires_at,
            },
        );
        Ok((session_token, wallet, expires_at))
    }

    pub async fn resolve_session(&self, token: &str) -> Result<String, ApiError> {
        let map = self.sessions.read().await;
        let session = map
            .get(token)
            .ok_or_else(|| ApiError::Unauthorized("invalid session".into()))?;
        if session.expires_at < Utc::now() {
            return Err(ApiError::Unauthorized("session expired".into()));
        }
        Ok(session.wallet.clone())
    }
}

pub fn hash_api_key(api_key: &str) -> String {
    let digest = Sha256::digest(api_key.as_bytes());
    hex::encode(digest)
}

pub fn generate_api_key() -> String {
    format!("{AGENT_KEY_PREFIX}{}", random_token())
}

fn random_token() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

pub fn normalize_address(raw: &str) -> Result<String, ApiError> {
    let addr = raw
        .trim()
        .parse::<Address>()
        .map_err(|_| ApiError::BadRequest(format!("invalid address: {raw}")))?;
    Ok(format!("{addr:#x}"))
}

fn eq_addr(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

fn recover_signer(message: &str, signature_hex: &str) -> Result<String, ApiError> {
    let sig = signature_hex
        .trim()
        .parse::<Signature>()
        .map_err(|e| ApiError::BadRequest(format!("invalid signature: {e}")))?;
    let address = sig
        .recover_address_from_msg(message.as_bytes())
        .map_err(|e| ApiError::Unauthorized(format!("recover failed: {e}")))?;
    Ok(format!("{address:#x}"))
}

/// Extract Bearer / session identity from request parts.
pub async fn authenticate(state: &AppState, parts: &Parts) -> Result<AuthIdentity, ApiError> {
    if let Some(token) = bearer_token(parts) {
        if token.starts_with(SESSION_PREFIX) {
            let wallet = state.auth.resolve_session(token).await?;
            return Ok(AuthIdentity::Human { wallet });
        }
        if token.starts_with(AGENT_KEY_PREFIX) {
            return lookup_agent_by_key(state, token).await;
        }
        // Accept raw session without prefix when sent as X-Session-Token style via Bearer sms_
        // Fallback: try session then agent hash.
        if let Ok(wallet) = state.auth.resolve_session(token).await {
            return Ok(AuthIdentity::Human { wallet });
        }
        return lookup_agent_by_key(state, token).await;
    }
    if let Some(token) = header_value(parts, "x-session-token") {
        let full = if token.starts_with(SESSION_PREFIX) {
            token.to_string()
        } else {
            format!("{SESSION_PREFIX}{token}")
        };
        let wallet = state.auth.resolve_session(&full).await?;
        return Ok(AuthIdentity::Human { wallet });
    }
    Err(ApiError::Unauthorized(
        "missing Authorization Bearer or X-Session-Token".into(),
    ))
}

async fn lookup_agent_by_key(state: &AppState, api_key: &str) -> Result<AuthIdentity, ApiError> {
    use db::schema::agents::dsl::*;
    let key_hash = hash_api_key(api_key);
    let mut conn = state
        .pool
        .get()
        .await
        .map_err(|e| ApiError::Internal(format!("pool get: {e}")))?;
    let row: Option<(String, String)> = agents
        .filter(api_key_hash.eq(key_hash))
        .select((agent_id, owner_wallet))
        .first(&mut conn)
        .await
        .optional()?;
    let (aid, owner) = row.ok_or_else(|| ApiError::Unauthorized("invalid api key".into()))?;
    Ok(AuthIdentity::Agent {
        agent_id: aid,
        owner_wallet: owner,
    })
}

fn bearer_token(parts: &Parts) -> Option<&str> {
    let value = header_value(parts, "authorization")?;
    let rest = value.strip_prefix("Bearer ").or_else(|| value.strip_prefix("bearer "))?;
    Some(rest.trim())
}

fn header_value<'a>(parts: &'a Parts, name: &str) -> Option<&'a str> {
    parts
        .headers
        .get(name)
        .and_then(|v| v.to_str().ok())
}

/// Axum extractor for authenticated identity.
pub struct AuthUser(pub AuthIdentity);

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let identity = authenticate(state, parts).await?;
        Ok(AuthUser(identity))
    }
}

/// Optional auth (public routes that behave differently when signed in).
#[allow(dead_code)]
pub struct OptionalAuth(pub Option<AuthIdentity>);

impl FromRequestParts<AppState> for OptionalAuth {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        match authenticate(state, parts).await {
            Ok(id) => Ok(OptionalAuth(Some(id))),
            Err(ApiError::Unauthorized(_)) => Ok(OptionalAuth(None)),
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy::signers::local::PrivateKeySigner;
    use alloy::signers::Signer;

    #[tokio::test]
    async fn challenge_verify_round_trip() {
        let store = AuthStore::new();
        let signer = PrivateKeySigner::random();
        let address = format!("{:#x}", signer.address());
        let (cid, message, _) = store.issue_challenge(&address).await.unwrap();
        let sig = signer.sign_message(message.as_bytes()).await.unwrap();
        let sig_hex = format!("0x{}", hex::encode(sig.as_bytes()));
        let (token, wallet, _) = store
            .verify_challenge(&cid, &address, &sig_hex)
            .await
            .unwrap();
        assert!(token.starts_with(SESSION_PREFIX));
        assert!(eq_addr(&wallet, &address));
        let resolved = store.resolve_session(&token).await.unwrap();
        assert!(eq_addr(&resolved, &address));
    }

    #[test]
    fn api_key_hash_stable() {
        let k = "smk_abc";
        assert_eq!(hash_api_key(k), hash_api_key(k));
        assert_ne!(hash_api_key(k), hash_api_key("smk_abd"));
    }
}
