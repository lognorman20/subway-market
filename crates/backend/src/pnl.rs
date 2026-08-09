//! PnL read API — ledger projection `agent_market_pnl`.

use axum::{
    extract::{Path, State},
    Json,
};
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use dto::AgentPnlResponse;
use db::AgentMarketPnlRow;

use crate::auth::{AuthIdentity, AuthUser};
use crate::error::ApiError;
use crate::markets::pnl_row_to_dto;
use crate::state::AppState;

pub async fn list_pnl(
    State(state): State<AppState>,
    AuthUser(identity): AuthUser,
) -> Result<Json<AgentPnlResponse>, ApiError> {
    use db::schema::{agent_market_pnl, agents};

    let mut conn = state
        .pool
        .get()
        .await
        .map_err(|e| ApiError::Internal(format!("pool get: {e}")))?;

    let rows: Vec<AgentMarketPnlRow> = match &identity {
        AuthIdentity::Agent { agent_id, .. } => agent_market_pnl::table
            .filter(agent_market_pnl::agent_id.eq(agent_id))
            .order(agent_market_pnl::updated_at.desc())
            .select(AgentMarketPnlRow::as_select())
            .load(&mut conn)
            .await?,
        AuthIdentity::Human { wallet } => {
            let owned: Vec<String> = agents::table
                .filter(agents::owner_wallet.eq(wallet))
                .select(agents::agent_id)
                .load(&mut conn)
                .await?;
            if owned.is_empty() {
                Vec::new()
            } else {
                agent_market_pnl::table
                    .filter(agent_market_pnl::agent_id.eq_any(owned))
                    .order(agent_market_pnl::updated_at.desc())
                    .select(AgentMarketPnlRow::as_select())
                    .load(&mut conn)
                    .await?
            }
        }
    };

    Ok(Json(AgentPnlResponse {
        rows: rows.into_iter().map(pnl_row_to_dto).collect(),
    }))
}

pub async fn get_agent_market_pnl(
    State(state): State<AppState>,
    AuthUser(identity): AuthUser,
    Path((aid, mid)): Path<(String, String)>,
) -> Result<Json<dto::AgentMarketPnl>, ApiError> {
    use db::schema::{agent_market_pnl, agents};

    let mut conn = state
        .pool
        .get()
        .await
        .map_err(|e| ApiError::Internal(format!("pool get: {e}")))?;

    // Authz: agent self or human owner.
    match &identity {
        AuthIdentity::Agent { agent_id, .. } if agent_id == &aid => {}
        AuthIdentity::Human { wallet } => {
            let owner: Option<String> = agents::table
                .filter(agents::agent_id.eq(&aid))
                .select(agents::owner_wallet)
                .first(&mut conn)
                .await
                .optional()?;
            let owner = owner.ok_or_else(|| ApiError::NotFound(format!("agent {aid} not found")))?;
            if !owner.eq_ignore_ascii_case(wallet) {
                return Err(ApiError::Forbidden("not allowed".into()));
            }
        }
        _ => return Err(ApiError::Forbidden("not allowed".into())),
    }

    let row: Option<AgentMarketPnlRow> = agent_market_pnl::table
        .filter(agent_market_pnl::agent_id.eq(&aid))
        .filter(agent_market_pnl::market_id.eq(&mid))
        .select(AgentMarketPnlRow::as_select())
        .first(&mut conn)
        .await
        .optional()?;

    let row = row.ok_or_else(|| ApiError::NotFound("pnl row not found".into()))?;
    Ok(Json(pnl_row_to_dto(row)))
}
