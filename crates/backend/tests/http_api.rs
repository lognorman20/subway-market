//! HTTP integration tests — auth, buy/sell (mocked chain), PnL shape.

use std::sync::Arc;

use alloy::signers::local::PrivateKeySigner;
use alloy::signers::Signer;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use backend::{router, AppState, AuthStore, FundingConfig, FundingMode, MockTradingChain};
use chrono::Utc;
use diesel::prelude::*;
use diesel_async::pooled_connection::deadpool::Pool;
use diesel_async::pooled_connection::AsyncDieselConnectionManager;
use diesel_async::{AsyncPgConnection, RunQueryDsl};
use dto::{
    AgentMarketPnl, AgentPnlResponse, AuthChallengeResponse, AuthVerifyResponse, BankrollResponse,
    CreateAgentResponse, MarketSide, TradeResponse,
};
use http_body_util::BodyExt;
use serde_json::json;
use tower::ServiceExt;
use uuid::Uuid;

fn database_url() -> String {
    std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://subway:subway@127.0.0.1:5433/subway_ledger".into())
}

async fn pool() -> Pool<AsyncPgConnection> {
    let config = AsyncDieselConnectionManager::<AsyncPgConnection>::new(database_url());
    Pool::builder(config)
        .max_size(4)
        .build()
        .expect("pool")
}

fn funding_local() -> FundingConfig {
    FundingConfig {
        mode: FundingMode::Local,
        seed_bankroll_usdc: 1_000 * 1_000_000,
        rain: None,
    }
}

async fn app_state(chain: Option<Arc<dyn backend::TradingChain>>) -> AppState {
    AppState {
        pool: pool().await,
        auth: AuthStore::new(),
        funding: funding_local(),
        chain,
        nats: None,
        rain: None,
        pending_funding: backend::PendingFundingStore::new(),
        settle_poll: None,
    }
}

async fn connect_nats() -> transport::TransportClient {
    let url = std::env::var("NATS_URL").unwrap_or_else(|_| "nats://127.0.0.1:4223".into());
    transport::connect(&url)
        .await
        .unwrap_or_else(|e| panic!("NATS required for trade tests at {url}: {e}"))
}

async fn app_state_with_nats(chain: Option<Arc<dyn backend::TradingChain>>) -> AppState {
    AppState {
        pool: pool().await,
        auth: AuthStore::new(),
        funding: funding_local(),
        chain,
        nats: Some(connect_nats().await),
        rain: None,
        pending_funding: backend::PendingFundingStore::new(),
        settle_poll: None,
    }
}

async fn create_agent_via_http(app: &axum::Router, session: &str) -> CreateAgentResponse {
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/agents")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {session}"))
                .body(Body::from(serde_json::to_vec(&json!({})).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    serde_json::from_value(json_body(res).await).unwrap()
}

async fn get_bankroll(app: &axum::Router, api_key: &str, agent_id: &str) -> i64 {
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/agents/{agent_id}/bankroll"))
                .header("authorization", format!("Bearer {api_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bankroll: BankrollResponse = serde_json::from_value(json_body(res).await).unwrap();
    bankroll.available_usdc
}

async fn json_body(res: axum::response::Response) -> serde_json::Value {
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes)
        .unwrap_or_else(|_| json!({"raw": String::from_utf8_lossy(&bytes)}))
}

async fn seed_market(pool: &Pool<AsyncPgConnection>, mid: &str) {
    use db::schema::markets::dsl;
    let mut conn = pool.get().await.expect("conn");
    diesel::insert_into(dsl::markets)
        .values((
            dsl::market_id.eq(mid),
            dsl::dedupe_key.eq(format!("d-{mid}-{}", Uuid::new_v4())),
            dsl::trip_id.eq("t1"),
            dsl::route_id.eq("A"),
            dsl::train_id.eq(Some("A1")),
            dsl::stop_id.eq("A99"),
            dsl::frozen_eta_unix.eq(1_700_000_000_i64),
            dsl::liquidity_b.eq("100000000"),
            dsl::q_early.eq("0"),
            dsl::q_late.eq("0"),
            dsl::outcome.eq("unresolved"),
            dsl::opened_event_id.eq(Option::<String>::None),
            dsl::resolved_event_id.eq(Option::<String>::None),
            dsl::chain_create_tx.eq(Some("0xopen")),
            dsl::chain_resolve_tx.eq(Option::<String>::None),
            dsl::opened_at.eq(Utc::now()),
            dsl::resolved_at.eq(Option::<chrono::DateTime<Utc>>::None),
        ))
        .on_conflict(dsl::market_id)
        .do_nothing()
        .execute(&mut conn)
        .await
        .expect("seed market");
}

async fn seed_pnl(pool: &Pool<AsyncPgConnection>, aid: &str, mid: &str) {
    use db::schema::agent_market_pnl::dsl;
    let mut conn = pool.get().await.expect("conn");
    diesel::insert_into(dsl::agent_market_pnl)
        .values((
            dsl::agent_id.eq(aid),
            dsl::market_id.eq(mid),
            dsl::early_shares.eq("1000000"),
            dsl::late_shares.eq("0"),
            dsl::cost_basis_usdc.eq(500_000_i64),
            dsl::realized_pnl_usdc.eq(0_i64),
            dsl::unrealized_pnl_usdc.eq(50_000_i64),
            dsl::total_pnl_usdc.eq(50_000_i64),
            dsl::last_mark_wad.eq(Some("500000000000000000")),
            dsl::resolved.eq(false),
            dsl::updated_at.eq(Utc::now()),
        ))
        .on_conflict((dsl::agent_id, dsl::market_id))
        .do_update()
        .set(dsl::total_pnl_usdc.eq(50_000_i64))
        .execute(&mut conn)
        .await
        .expect("seed pnl");
}

async fn human_session(app: &axum::Router) -> (PrivateKeySigner, String) {
    let signer = PrivateKeySigner::random();
    let address = format!("{:#x}", signer.address());

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/challenge")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({ "address": address })).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let challenge: AuthChallengeResponse = serde_json::from_value(json_body(res).await).unwrap();

    let sig = signer
        .sign_message(challenge.message.as_bytes())
        .await
        .unwrap();
    let sig_hex = format!("0x{}", hex::encode(sig.as_bytes()));

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/verify")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "challenge_id": challenge.challenge_id,
                        "address": address,
                        "signature": sig_hex,
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let verified: AuthVerifyResponse = serde_json::from_value(json_body(res).await).unwrap();
    (signer, verified.session_token)
}

#[tokio::test]
async fn auth_reject_protected_routes() {
    let state = app_state(None).await;
    let app = router(state);

    let res = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/agents")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    let body = json_body(res).await;
    assert_eq!(body["code"], "unauthorized");
}

#[tokio::test]
async fn auth_accept_human_and_agent() {
    let state = app_state(None).await;
    let app = router(state);

    let (_signer, session) = human_session(&app).await;

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/agents")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {session}"))
                .body(Body::from(
                    serde_json::to_vec(&json!({ "display_name": "bot" })).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let created: CreateAgentResponse = serde_json::from_value(json_body(res).await).unwrap();
    assert!(created.api_key.starts_with("smk_"));

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/agents")
                .header("authorization", format!("Bearer {}", created.api_key))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let res = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/agents")
                .header("authorization", "Bearer smk_deadbeef")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn buy_returns_trade_and_debits_bankroll() {
    let mock = Arc::new(MockTradingChain::balanced());
    let state = app_state_with_nats(Some(mock)).await;
    let pool = state.pool.clone();
    let market_id = format!("{}", (Uuid::new_v4().as_u128() % 1_000_000) + 1);
    seed_market(&pool, &market_id).await;

    let app = router(state);
    let (_signer, session) = human_session(&app).await;
    let created = create_agent_via_http(&app, &session).await;

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/buy")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", created.api_key))
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "market_id": market_id,
                        "side": "early",
                        "shares": "1000000"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = res.status();
    let body = json_body(res).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let trade: TradeResponse = serde_json::from_value(body).unwrap();
    assert_eq!(trade.trade.action, dto::TradeAction::Buy);
    assert_eq!(trade.trade.usdc_amount, 500_000);
    assert_eq!(trade.trade.side, MarketSide::Early);

    let bankroll = get_bankroll(&app, &created.api_key, created.agent_id.as_ref()).await;
    assert_eq!(bankroll, 1_000_000_000 - 500_000);

    let res = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/sell")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", created.api_key))
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "market_id": market_id,
                        "side": "early",
                        "shares": "1000000"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = res.status();
    let body = json_body(res).await;
    assert_eq!(status, StatusCode::OK, "{body}");
}

#[tokio::test]
async fn buy_without_nats_returns_503() {
    let mock = Arc::new(MockTradingChain::balanced());
    let state = app_state(Some(mock.clone())).await;
    let pool = state.pool.clone();
    let market_id = format!("{}", (Uuid::new_v4().as_u128() % 1_000_000) + 10);
    seed_market(&pool, &market_id).await;

    let app = router(state);
    let (_signer, session) = human_session(&app).await;
    let created = create_agent_via_http(&app, &session).await;

    let res = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/buy")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", created.api_key))
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "market_id": market_id,
                        "side": "early",
                        "shares": "1000000"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = res.status();
    let body = json_body(res).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert_eq!(body["code"], "unavailable");
    assert_eq!(mock.buy_call_count(), 0);
}

#[tokio::test]
async fn insufficient_bankroll_skips_chain_buy() {
    let mock = Arc::new(MockTradingChain {
        buy_cost: 9_999_999_999_999,
        ..MockTradingChain::balanced()
    });
    let state = app_state_with_nats(Some(mock.clone())).await;
    let pool = state.pool.clone();
    let market_id = format!("{}", (Uuid::new_v4().as_u128() % 1_000_000) + 20);
    seed_market(&pool, &market_id).await;

    let app = router(state);
    let (_signer, session) = human_session(&app).await;
    let created = create_agent_via_http(&app, &session).await;
    let before = get_bankroll(&app, &created.api_key, created.agent_id.as_ref()).await;

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/buy")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", created.api_key))
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "market_id": market_id,
                        "side": "early",
                        "shares": "1000000"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = res.status();
    let body = json_body(res).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["code"], "insufficient_bankroll");
    assert_eq!(mock.buy_call_count(), 0);
    let after = get_bankroll(&app, &created.api_key, created.agent_id.as_ref()).await;
    assert_eq!(after, before);
}

#[tokio::test]
async fn chain_fail_restores_bankroll() {
    let mock = Arc::new(MockTradingChain::balanced());
    mock.fail_tx
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let state = app_state_with_nats(Some(mock.clone())).await;
    let pool = state.pool.clone();
    let market_id = format!("{}", (Uuid::new_v4().as_u128() % 1_000_000) + 30);
    seed_market(&pool, &market_id).await;

    let app = router(state);
    let (_signer, session) = human_session(&app).await;
    let created = create_agent_via_http(&app, &session).await;
    let before = get_bankroll(&app, &created.api_key, created.agent_id.as_ref()).await;

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/buy")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", created.api_key))
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "market_id": market_id,
                        "side": "early",
                        "shares": "1000000"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = res.status();
    let body = json_body(res).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{body}");
    assert_eq!(mock.buy_call_count(), 1);
    let after = get_bankroll(&app, &created.api_key, created.agent_id.as_ref()).await;
    assert_eq!(after, before, "bankroll must restore after chain fail");
}

#[tokio::test]
async fn pnl_read_returns_projection_shape() {
    let state = app_state(None).await;
    let pool = state.pool.clone();
    let app = router(state);
    let (_signer, session) = human_session(&app).await;

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/agents")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {session}"))
                .body(Body::from(serde_json::to_vec(&json!({})).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    let created: CreateAgentResponse = serde_json::from_value(json_body(res).await).unwrap();

    let market_id = format!("{}", (Uuid::new_v4().as_u128() % 1_000_000) + 2);
    seed_market(&pool, &market_id).await;
    seed_pnl(&pool, created.agent_id.as_ref(), &market_id).await;

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/pnl")
                .header("authorization", format!("Bearer {}", created.api_key))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let pnl: AgentPnlResponse = serde_json::from_value(json_body(res).await).unwrap();
    assert!(!pnl.rows.is_empty());
    let row: &AgentMarketPnl = &pnl.rows[0];
    assert_eq!(row.total_pnl_usdc, 50_000);
    assert!(!row.resolved);
    assert!(row.last_mark_wad.is_some());

    let res = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/pnl")
                .header("authorization", format!("Bearer {session}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let pnl: AgentPnlResponse = serde_json::from_value(json_body(res).await).unwrap();
    assert!(!pnl.rows.is_empty());
}

#[tokio::test]
async fn llms_txt_served() {
    let state = app_state(None).await;
    let app = router(state);
    let res = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/llms.txt")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8_lossy(&bytes);
    assert!(text.contains("Authorization: Bearer"));
    assert!(text.contains("/buy"));
    assert!(text.contains("insufficient_bankroll"));
}
