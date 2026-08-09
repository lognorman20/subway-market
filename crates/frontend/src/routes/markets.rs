//! Market list/detail + live partials + buy/sell/agent proxies.

use axum::{
    extract::{Form, Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
    Router,
};
use serde::Deserialize;

use crate::backend::{AppState, BackendFetch};
use crate::markets::{
    agent_options, market_detail_view, pnl_rows_for_market, AgentOption, BannerKind,
    MarketDetailView, MarketsPresentation,
};
use crate::session::session_token_from_headers;
use crate::templates::{
    MarketLiveTemplate, MarketPageTemplate, MarketPnlTemplate, MarketsPageTemplate,
    TradeFlashTemplate, TemplateResponse,
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/markets", get(markets_page))
        .route("/markets/active", get(active_market))
        .route("/markets/{id}", get(market_page))
        .route("/markets/{id}/live", get(market_live))
        .route("/markets/{id}/pnl", get(market_pnl))
        .route("/markets/{id}/trade", post(market_trade))
        .route("/markets/{id}/agents", post(create_agent))
}

async fn markets_page(State(state): State<AppState>) -> TemplateResponse<MarketsPageTemplate> {
    let presentation = match state.fetch_markets().await {
        BackendFetch::Ok(body) => MarketsPresentation::from_summaries(body.markets),
        BackendFetch::Unreachable(err) => MarketsPresentation {
            active: None,
            open: Vec::new(),
            resolved: Vec::new(),
            banner: Some((
                BannerKind::Warn,
                format!("Backend unreachable ({err})."),
            )),
        },
        BackendFetch::HttpError(status, msg) => MarketsPresentation {
            active: None,
            open: Vec::new(),
            resolved: Vec::new(),
            banner: Some((
                BannerKind::Warn,
                format!("Backend HTTP {status}: {msg}"),
            )),
        },
        BackendFetch::ParseError(err) => MarketsPresentation {
            active: None,
            open: Vec::new(),
            resolved: Vec::new(),
            banner: Some((BannerKind::Error, format!("Could not parse markets ({err})."))),
        },
        BackendFetch::Empty => MarketsPresentation::from_summaries(Vec::new()),
    };
    TemplateResponse(MarketsPageTemplate::from(presentation))
}

async fn active_market(State(state): State<AppState>) -> Response {
    match state.fetch_markets().await {
        BackendFetch::Ok(body) => {
            let presentation = MarketsPresentation::from_summaries(body.markets);
            if let Some(active) = presentation.active {
                Redirect::to(&format!("/markets/{}", active.market_id)).into_response()
            } else {
                Redirect::to("/markets").into_response()
            }
        }
        _ => Redirect::to("/markets").into_response(),
    }
}

async fn market_page(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Response {
    let session = session_token_from_headers(&headers);
    match load_detail(&state, &id).await {
        Ok(detail) => {
            let (agents, agents_banner) = load_agents(&state, session.as_deref()).await;
            let session_present = session.is_some();
            let trade_enabled = session_present && !agents.is_empty() && detail.is_open;
            let tpl = MarketPageTemplate {
                detail,
                agents,
                session_present,
                trade_enabled,
                agents_banner,
                trade_flash: None,
                trade_ok: None,
            };
            TemplateResponse(tpl).into_response()
        }
        Err(resp) => resp,
    }
}

async fn market_live(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    match load_detail(&state, &id).await {
        Ok(detail) => TemplateResponse(MarketLiveTemplate { detail }).into_response(),
        Err(resp) => resp,
    }
}

async fn market_pnl(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> TemplateResponse<MarketPnlTemplate> {
    let Some(session) = session_token_from_headers(&headers) else {
        return TemplateResponse(MarketPnlTemplate {
            rows: Vec::new(),
            banner_class: Some("banner banner-info"),
            banner_message: Some(
                "Connect wallet to see per-agent PnL for this market.".into(),
            ),
        });
    };

    let agents = match state.list_agents(&session).await {
        BackendFetch::Ok(body) => body.agents,
        _ => Vec::new(),
    };

    match state.list_pnl(&session).await {
        BackendFetch::Ok(body) => {
            let rows = pnl_rows_for_market(&body.rows, &id, &agents);
            let (banner_class, banner_message) = if rows.is_empty() {
                (
                    Some("banner banner-info"),
                    Some("No PnL rows for your agents on this market yet.".into()),
                )
            } else {
                (None, None)
            };
            TemplateResponse(MarketPnlTemplate {
                rows,
                banner_class,
                banner_message,
            })
        }
        BackendFetch::HttpError(status, msg) => TemplateResponse(MarketPnlTemplate {
            rows: Vec::new(),
            banner_class: Some("banner banner-warn"),
            banner_message: Some(format!("PnL unavailable (HTTP {status}): {msg}")),
        }),
        BackendFetch::Unreachable(err) => TemplateResponse(MarketPnlTemplate {
            rows: Vec::new(),
            banner_class: Some("banner banner-warn"),
            banner_message: Some(format!("Backend unreachable ({err}).")),
        }),
        other => TemplateResponse(MarketPnlTemplate {
            rows: Vec::new(),
            banner_class: Some("banner banner-error"),
            banner_message: Some(format!("PnL error: {other:?}")),
        }),
    }
}

#[derive(Debug, Deserialize)]
struct TradeForm {
    action: String,
    side: String,
    shares: String,
    agent_id: String,
}

async fn market_trade(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Form(form): Form<TradeForm>,
) -> TemplateResponse<TradeFlashTemplate> {
    let Some(session) = session_token_from_headers(&headers) else {
        return TemplateResponse(TradeFlashTemplate {
            ok: false,
            message: "Connect wallet before buy/sell.".into(),
        });
    };

    let side = form.side.trim().to_ascii_lowercase();
    if side != "early" && side != "late" {
        return TemplateResponse(TradeFlashTemplate {
            ok: false,
            message: "Side must be early or late.".into(),
        });
    }

    let shares = normalize_shares(form.shares.trim());
    if shares.is_empty() {
        return TemplateResponse(TradeFlashTemplate {
            ok: false,
            message: "Enter a share quantity (micro-USDC integer or whole USDC).".into(),
        });
    }

    let agent_id = form.agent_id.trim();
    if agent_id.is_empty() {
        return TemplateResponse(TradeFlashTemplate {
            ok: false,
            message: "Select an agent to trade with.".into(),
        });
    }

    let result = match form.action.trim().to_ascii_lowercase().as_str() {
        "sell" => {
            state
                .sell(&session, &id, &side, &shares, agent_id)
                .await
        }
        _ => state.buy(&session, &id, &side, &shares, agent_id).await,
    };

    TemplateResponse(TradeFlashTemplate {
        ok: result.ok,
        message: result.message,
    })
}

#[derive(Debug, Deserialize)]
struct CreateAgentForm {
    display_name: Option<String>,
}

async fn create_agent(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Form(form): Form<CreateAgentForm>,
) -> Response {
    let Some(session) = session_token_from_headers(&headers) else {
        return TemplateResponse(TradeFlashTemplate {
            ok: false,
            message: "Connect wallet before creating an agent.".into(),
        })
        .into_response();
    };

    match state
        .create_agent(&session, form.display_name.as_deref())
        .await
    {
        BackendFetch::Ok(_) => Redirect::to(&format!("/markets/{id}")).into_response(),
        BackendFetch::HttpError(status, msg) => TemplateResponse(TradeFlashTemplate {
            ok: false,
            message: format!("Create agent failed (HTTP {status}): {msg}"),
        })
        .into_response(),
        BackendFetch::Unreachable(err) => TemplateResponse(TradeFlashTemplate {
            ok: false,
            message: format!("Backend unreachable: {err}"),
        })
        .into_response(),
        other => TemplateResponse(TradeFlashTemplate {
            ok: false,
            message: format!("Create agent failed: {other:?}"),
        })
        .into_response(),
    }
}

async fn load_detail(state: &AppState, id: &str) -> Result<MarketDetailView, Response> {
    let detail = match state.fetch_market(id).await {
        BackendFetch::Ok(body) => body,
        BackendFetch::HttpError(404, msg) => {
            return Err((StatusCode::NOT_FOUND, msg).into_response());
        }
        BackendFetch::HttpError(status, msg) => {
            return Err((
                StatusCode::BAD_GATEWAY,
                format!("market fetch HTTP {status}: {msg}"),
            )
                .into_response());
        }
        BackendFetch::Unreachable(err) => {
            return Err((
                StatusCode::BAD_GATEWAY,
                format!("backend unreachable: {err}"),
            )
                .into_response());
        }
        BackendFetch::ParseError(err) => {
            return Err((
                StatusCode::BAD_GATEWAY,
                format!("parse error: {err}"),
            )
                .into_response());
        }
        BackendFetch::Empty => {
            return Err((StatusCode::BAD_GATEWAY, "empty market response").into_response());
        }
    };

    let trades = match state.fetch_market_trades(id, 100).await {
        BackendFetch::Ok(body) => body.trades,
        _ => Vec::new(),
    };

    Ok(market_detail_view(
        &detail.market,
        &detail.price_early_wad,
        &detail.price_late_wad,
        &trades,
    ))
}

async fn load_agents(
    state: &AppState,
    session: Option<&str>,
) -> (Vec<AgentOption>, Option<String>) {
    let Some(token) = session else {
        return (
            Vec::new(),
            Some("Connect wallet to load agents and enable buy/sell.".into()),
        );
    };
    match state.list_agents(token).await {
        BackendFetch::Ok(body) => {
            let opts = agent_options(&body.agents);
            if opts.is_empty() {
                (
                    opts,
                    Some("No agents yet — create one to trade.".into()),
                )
            } else {
                (opts, None)
            }
        }
        BackendFetch::HttpError(status, msg) => (
            Vec::new(),
            Some(format!("Could not load agents (HTTP {status}): {msg}")),
        ),
        BackendFetch::Unreachable(err) => (
            Vec::new(),
            Some(format!("Backend unreachable ({err}).")),
        ),
        other => (Vec::new(), Some(format!("Agents error: {other:?}"))),
    }
}

/// Accept whole USDC ("1.5") or micro-USDC integer ("1500000").
fn normalize_shares(raw: &str) -> String {
    let raw = raw.trim().replace(',', "");
    if raw.is_empty() {
        return String::new();
    }
    if raw.contains('.') {
        let Ok(v) = raw.parse::<f64>() else {
            return String::new();
        };
        if !v.is_finite() || v <= 0.0 {
            return String::new();
        }
        let micro = (v * 1_000_000.0).round() as i64;
        if micro <= 0 {
            return String::new();
        }
        return micro.to_string();
    }
    if raw.chars().all(|c| c.is_ascii_digit()) {
        raw
    } else {
        String::new()
    }
}

#[cfg(test)]
mod tests {
    use super::normalize_shares;

    #[test]
    fn normalize_whole_and_micro() {
        assert_eq!(normalize_shares("1.5"), "1500000");
        assert_eq!(normalize_shares("1500000"), "1500000");
        assert_eq!(normalize_shares(""), "");
        assert_eq!(normalize_shares("abc"), "");
    }
}
