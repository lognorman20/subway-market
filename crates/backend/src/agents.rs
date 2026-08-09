//! Agent CRUD + bankroll seed (backend funding/identity writes).

use axum::{
    extract::{Path, State},
    Json,
};
use chrono::Utc;
use db::NewAgentRow;
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use dto::{
    AgentId, AgentSummary, AgentsListResponse, BankrollResponse, CreateAgentRequest,
    CreateAgentResponse, SeedBankrollRequest,
};

use crate::auth::{generate_api_key, hash_api_key, AuthIdentity, AuthUser};
use crate::error::ApiError;
use crate::funding::whole_usdc_to_micro;
use crate::state::AppState;

pub async fn create_agent(
    State(state): State<AppState>,
    AuthUser(identity): AuthUser,
    Json(req): Json<CreateAgentRequest>,
) -> Result<Json<CreateAgentResponse>, ApiError> {
    let owner_wallet = match &identity {
        AuthIdentity::Human { wallet } => wallet.clone(),
        AuthIdentity::Agent { .. } => {
            return Err(ApiError::Forbidden(
                "only human session may create agents".into(),
            ));
        }
    };

    let agent_id = AgentId::new();
    let api_key = generate_api_key();
    let api_key_hash = hash_api_key(&api_key);
    let seed = state.funding.local_seed_micro();
    let now = Utc::now();

    let row = NewAgentRow {
        agent_id: agent_id.0.clone(),
        owner_wallet,
        display_name: req.display_name,
        api_key_hash,
        created_at: now,
        bankroll_usdc: seed,
    };

    let mut conn = state
        .pool
        .get()
        .await
        .map_err(|e| ApiError::Internal(format!("pool get: {e}")))?;

    diesel::insert_into(db::schema::agents::table)
        .values(&row)
        .execute(&mut conn)
        .await?;

    Ok(Json(CreateAgentResponse { agent_id, api_key }))
}

pub async fn list_agents(
    State(state): State<AppState>,
    AuthUser(identity): AuthUser,
) -> Result<Json<AgentsListResponse>, ApiError> {
    use db::schema::agents::dsl::*;

    let mut conn = state
        .pool
        .get()
        .await
        .map_err(|e| ApiError::Internal(format!("pool get: {e}")))?;

    let rows: Vec<(String, Option<String>, chrono::DateTime<Utc>)> = match &identity {
        AuthIdentity::Human { wallet } => agents
            .filter(owner_wallet.eq(wallet))
            .select((agent_id, display_name, created_at))
            .order(created_at.desc())
            .load(&mut conn)
            .await?,
        AuthIdentity::Agent {
            agent_id: aid,
            ..
        } => agents
            .filter(agent_id.eq(aid))
            .select((agent_id, display_name, created_at))
            .load(&mut conn)
            .await?,
    };

    let summaries: Vec<AgentSummary> = rows
        .into_iter()
        .map(|(id, name, created)| AgentSummary {
            agent_id: AgentId(id),
            display_name: name,
            created_at: created,
        })
        .collect();

    Ok(Json(AgentsListResponse {
        agents: summaries,
    }))
}

pub async fn get_agent(
    State(state): State<AppState>,
    AuthUser(identity): AuthUser,
    Path(id): Path<String>,
) -> Result<Json<AgentSummary>, ApiError> {
    use db::schema::agents::dsl::*;

    let mut conn = state
        .pool
        .get()
        .await
        .map_err(|e| ApiError::Internal(format!("pool get: {e}")))?;

    let row: Option<(String, Option<String>, chrono::DateTime<Utc>, String)> = agents
        .filter(agent_id.eq(&id))
        .select((agent_id, display_name, created_at, owner_wallet))
        .first(&mut conn)
        .await
        .optional()?;

    let (aid, name, created, owner) =
        row.ok_or_else(|| ApiError::NotFound(format!("agent {id} not found")))?;

    ensure_can_view(&identity, &aid, &owner)?;

    Ok(Json(AgentSummary {
        agent_id: AgentId(aid),
        display_name: name,
        created_at: created,
    }))
}

pub async fn delete_agent(
    State(state): State<AppState>,
    AuthUser(identity): AuthUser,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    use db::schema::agents::dsl::*;

    let AuthIdentity::Human { wallet } = &identity else {
        return Err(ApiError::Forbidden(
            "only human session may delete agents".into(),
        ));
    };

    let mut conn = state
        .pool
        .get()
        .await
        .map_err(|e| ApiError::Internal(format!("pool get: {e}")))?;

    let deleted = diesel::delete(agents.filter(agent_id.eq(&id)).filter(owner_wallet.eq(wallet)))
        .execute(&mut conn)
        .await?;

    if deleted == 0 {
        return Err(ApiError::NotFound(format!("agent {id} not found")));
    }
    Ok(Json(serde_json::json!({ "deleted": true })))
}

pub async fn get_bankroll(
    State(state): State<AppState>,
    AuthUser(identity): AuthUser,
    Path(id): Path<String>,
) -> Result<Json<BankrollResponse>, ApiError> {
    let (bankroll, owner) = load_bankroll(&state, &id).await?;
    ensure_can_view(&identity, &id, &owner)?;
    Ok(Json(BankrollResponse {
        available_usdc: bankroll,
    }))
}

pub async fn seed_bankroll(
    State(state): State<AppState>,
    AuthUser(identity): AuthUser,
    Json(req): Json<SeedBankrollRequest>,
) -> Result<Json<BankrollResponse>, ApiError> {
    if !state.funding.mode.is_local() {
        return Err(ApiError::Forbidden(
            "bankroll seed only when FUNDING_MODE=local".into(),
        ));
    }

    let AuthIdentity::Human { wallet } = &identity else {
        return Err(ApiError::Forbidden(
            "only human session may seed bankroll".into(),
        ));
    };

    let amount = match req.amount_usdc {
        Some(whole) => whole_usdc_to_micro(whole),
        None => state.funding.seed_bankroll_usdc,
    };

    use db::schema::agents::dsl::*;
    let mut conn = state
        .pool
        .get()
        .await
        .map_err(|e| ApiError::Internal(format!("pool get: {e}")))?;

    let updated = diesel::update(
        agents
            .filter(agent_id.eq(req.agent_id.as_ref()))
            .filter(owner_wallet.eq(wallet)),
    )
    .set(bankroll_usdc.eq(amount))
    .execute(&mut conn)
    .await?;

    if updated == 0 {
        return Err(ApiError::NotFound(format!(
            "agent {} not found",
            req.agent_id.as_ref()
        )));
    }

    Ok(Json(BankrollResponse {
        available_usdc: amount,
    }))
}

pub(crate) async fn load_bankroll(
    state: &AppState,
    aid: &str,
) -> Result<(i64, String), ApiError> {
    use db::schema::agents::dsl::*;
    let mut conn = state
        .pool
        .get()
        .await
        .map_err(|e| ApiError::Internal(format!("pool get: {e}")))?;
    let row: Option<(i64, String)> = agents
        .filter(agent_id.eq(aid))
        .select((bankroll_usdc, owner_wallet))
        .first(&mut conn)
        .await
        .optional()?;
    row.ok_or_else(|| ApiError::NotFound(format!("agent {aid} not found")))
}

/// Debit bankroll under `SELECT … FOR UPDATE` (commits before chain).
pub(crate) async fn debit_bankroll_locked(
    state: &AppState,
    aid: &str,
    amount: i64,
) -> Result<i64, ApiError> {
    if amount < 0 {
        return Err(ApiError::Internal("debit amount must be >= 0".into()));
    }
    adjust_bankroll_locked(state, aid, -amount).await
}

/// Credit bankroll under `SELECT … FOR UPDATE`.
pub(crate) async fn credit_bankroll_locked(
    state: &AppState,
    aid: &str,
    amount: i64,
) -> Result<i64, ApiError> {
    if amount < 0 {
        return Err(ApiError::Internal("credit amount must be >= 0".into()));
    }
    adjust_bankroll_locked(state, aid, amount).await
}

async fn adjust_bankroll_locked(
    state: &AppState,
    aid: &str,
    delta: i64,
) -> Result<i64, ApiError> {
    use db::schema::agents::dsl::*;
    use diesel_async::AsyncConnection;

    let mut conn = state
        .pool
        .get()
        .await
        .map_err(|e| ApiError::Internal(format!("pool get: {e}")))?;

    let aid = aid.to_string();
    conn.transaction(|conn| {
        Box::pin(async move {
            let current: i64 = agents
                .filter(agent_id.eq(&aid))
                .select(bankroll_usdc)
                .for_update()
                .first(conn)
                .await
                .optional()?
                .ok_or_else(|| ApiError::NotFound(format!("agent {aid} not found")))?;

            let next = current
                .checked_add(delta)
                .ok_or_else(|| ApiError::Internal("bankroll overflow".into()))?;
            if next < 0 {
                return Err(ApiError::InsufficientBankroll);
            }

            diesel::update(agents.filter(agent_id.eq(&aid)))
                .set(bankroll_usdc.eq(next))
                .execute(conn)
                .await?;

            Ok(next)
        })
    })
    .await
}

fn ensure_can_view(identity: &AuthIdentity, aid: &str, owner: &str) -> Result<(), ApiError> {
    match identity {
        AuthIdentity::Agent { agent_id, .. } if agent_id == aid => Ok(()),
        AuthIdentity::Human { wallet } if wallet.eq_ignore_ascii_case(owner) => Ok(()),
        _ => Err(ApiError::Forbidden("not allowed to view this agent".into())),
    }
}

/// Resolve which agent_id a buy/sell runs as.
pub(crate) fn resolve_trading_agent(
    identity: &AuthIdentity,
    requested: Option<&AgentId>,
) -> Result<String, ApiError> {
    match identity {
        AuthIdentity::Agent { agent_id, .. } => {
            if let Some(req) = requested {
                if req.as_ref() != agent_id {
                    return Err(ApiError::Forbidden(
                        "bearer agent cannot trade as another agent".into(),
                    ));
                }
            }
            Ok(agent_id.clone())
        }
        AuthIdentity::Human { .. } => requested
            .map(|a| a.0.clone())
            .ok_or_else(|| ApiError::BadRequest("agent_id required for human session trades".into())),
    }
}

pub(crate) async fn ensure_human_owns_agent(
    state: &AppState,
    wallet: &str,
    aid: &str,
) -> Result<(), ApiError> {
    use db::schema::agents::dsl::*;
    let mut conn = state
        .pool
        .get()
        .await
        .map_err(|e| ApiError::Internal(format!("pool get: {e}")))?;
    let owner: Option<String> = agents
        .filter(agent_id.eq(aid))
        .select(owner_wallet)
        .first(&mut conn)
        .await
        .optional()?;
    let owner = owner.ok_or_else(|| ApiError::NotFound(format!("agent {aid} not found")))?;
    if !owner.eq_ignore_ascii_case(wallet) {
        return Err(ApiError::Forbidden("agent not owned by session wallet".into()));
    }
    Ok(())
}
