//! Rain fund → settle → bankroll credit (wiremock).

use std::sync::Arc;

use alloy::signers::local::PrivateKeySigner;
use alloy::signers::Signer;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use backend::{
    router, AppState, AuthStore, FundingConfig, FundingMode, HttpRainClient, MockTradingChain,
    PendingFundingStore, RainApi, RainClientError, RainConfig, SettlePollConfig,
};
use diesel_async::pooled_connection::deadpool::Pool;
use diesel_async::pooled_connection::AsyncDieselConnectionManager;
use diesel_async::AsyncPgConnection;
use dto::{AuthChallengeResponse, AuthVerifyResponse, CreateAgentResponse};
use http_body_util::BodyExt;
use serde_json::json;
use tower::ServiceExt;
use wiremock::matchers::{header, method, path, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

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

fn rain_cfg(base: &str) -> RainConfig {
    RainConfig {
        api_base_url: base.to_string(),
        api_key: "test-rain-key".into(),
        user_id: "11111111-1111-1111-1111-111111111111".into(),
        team_id: None,
        destination_address: "0x1234567890abcdef1234567890abcdef12345678".into(),
        destination_rail: "base".into(),
    }
}

async fn json_body(res: axum::response::Response) -> serde_json::Value {
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes)
        .unwrap_or_else(|_| json!({"raw": String::from_utf8_lossy(&bytes)}))
}

async fn human_session(app: &axum::Router) -> String {
    let signer: PrivateKeySigner = PrivateKeySigner::random();
    let address = format!("{}", signer.address());

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
    let signature = format!("0x{}", hex::encode(sig.as_bytes()));

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
                        "signature": signature,
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let verified: AuthVerifyResponse = serde_json::from_value(json_body(res).await).unwrap();
    verified.session_token
}

async fn create_agent(app: &axum::Router, session: &str) -> CreateAgentResponse {
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

async fn setup_mocks(server: &MockServer, route_id: &str, tx_id: &str) {
    let route_body = json!({
        "id": route_id,
        "userId": "11111111-1111-1111-1111-111111111111",
        "status": "active",
        "source": { "currency": "usd", "rail": "ach" },
        "destination": {
            "currency": "usdc",
            "rail": "base",
            "address": { "type": "onchain", "address": "0x1234567890abcdef1234567890abcdef12345678" }
        },
        "depositAddress": {
            "type": "fiat",
            "beneficiaryName": "RAIN",
            "beneficiaryAddress": "1 St",
            "beneficiaryBankName": "Bank",
            "beneficiaryBankAddress": "2 St",
            "accountNumber": "123",
            "routingNumber": "021000021"
        },
        "createdAt": "2025-01-15T10:00:00Z",
        "updatedAt": "2025-01-15T10:00:00Z"
    });

    Mock::given(method("GET"))
        .and(path("/payment-routes"))
        .and(header("Api-Key", "test-rain-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "paymentRoutes": [],
            "automations": []
        })))
        .up_to_n_times(1)
        .mount(server)
        .await;

    Mock::given(method("GET"))
        .and(path("/payment-routes"))
        .and(header("Api-Key", "test-rain-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "paymentRoutes": [route_body.clone()],
            "automations": [route_body.clone()]
        })))
        .mount(server)
        .await;

    Mock::given(method("POST"))
        .and(path("/payment-routes"))
        .and(header("Api-Key", "test-rain-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(route_body))
        .mount(server)
        .await;

    Mock::given(method("POST"))
        .and(path("/simulate/payment-routes"))
        .and(header("Api-Key", "test-rain-key"))
        .respond_with(ResponseTemplate::new(202).set_body_json(json!({
            "simulationId": "sim_abc",
            "flow": "usd_onramp",
            "status": "accepted"
        })))
        .mount(server)
        .await;

    let tx = json!({
        "id": tx_id,
        "type": "transfer",
        "transfer": {
            "status": "settled",
            "source": { "currency": "usd", "rail": "ach", "amount": "100" },
            "destination": {
                "currency": "usdc",
                "rail": "base",
                "amount": "100",
                "address": { "type": "onchain", "address": "0x1234567890abcdef1234567890abcdef12345678" }
            },
            "exchangeRate": 1.0,
            "fees": {},
            "quoteId": "00000000-0000-0000-0000-000000000001",
            "createdAt": "2025-01-15T10:00:00Z",
            "updatedAt": "2025-01-15T10:01:00Z",
            "expiresAt": "2025-01-16T10:00:00Z"
        }
    });

    Mock::given(method("GET"))
        .and(path("/issuing/transactions"))
        .and(header("Api-Key", "test-rain-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([tx.clone()])))
        .mount(server)
        .await;

    Mock::given(method("GET"))
        .and(path_regex(r"^/issuing/transactions/.+"))
        .and(header("Api-Key", "test-rain-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(tx))
        .mount(server)
        .await;
}

async fn testnet_app(server: &MockServer) -> (axum::Router, Arc<MockTradingChain>) {
    testnet_app_with_credit_on_simulate(server, false).await
}

async fn testnet_app_with_credit_on_simulate(
    server: &MockServer,
    sandbox_credit_on_simulate: bool,
) -> (axum::Router, Arc<MockTradingChain>) {
    let cfg = rain_cfg(&server.uri());
    let client = Arc::new(HttpRainClient::new(cfg.clone())) as Arc<dyn RainApi>;
    let chain = Arc::new(MockTradingChain::balanced());
    let state = AppState {
        pool: pool().await,
        auth: AuthStore::new(),
        funding: FundingConfig {
            mode: FundingMode::Testnet,
            seed_bankroll_usdc: 0,
            rain: Some(cfg),
            sandbox_credit_on_simulate,
        },
        chain: Some(chain.clone() as Arc<dyn backend::TradingChain>),
        nats: None,
        rain: Some(client),
        pending_funding: PendingFundingStore::new(),
        settle_poll: Some(SettlePollConfig::for_tests()),
    };
    (router(state), chain)
}

#[tokio::test]
async fn rain_client_sends_api_key_header() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/payment-routes"))
        .and(header("Api-Key", "test-rain-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "paymentRoutes": [],
            "automations": []
        })))
        .expect(1)
        .mount(&server)
        .await;

    let client = HttpRainClient::new(rain_cfg(&server.uri()));
    let routes = RainApi::list_payment_routes(&client).await.expect("list");
    assert!(routes.is_empty());
}

#[tokio::test]
async fn rain_client_accepts_bare_array_payment_routes() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/payment-routes"))
        .and(header("Api-Key", "test-rain-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .expect(1)
        .mount(&server)
        .await;

    let client = HttpRainClient::new(rain_cfg(&server.uri()));
    let routes = RainApi::list_payment_routes(&client).await.expect("list bare");
    assert!(routes.is_empty());
}

#[tokio::test]
async fn rain_client_auth_error() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/payment-routes"))
        .respond_with(ResponseTemplate::new(401).set_body_json(json!({
            "statusCode": 401,
            "error": "Unauthorized",
            "message": "Invalid or missing API key"
        })))
        .mount(&server)
        .await;

    let client = HttpRainClient::new(rain_cfg(&server.uri()));
    let err = RainApi::list_payment_routes(&client)
        .await
        .expect_err("401");
    match err {
        RainClientError::Api { status, .. } => {
            assert_eq!(status.as_u16(), 401);
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[tokio::test]
async fn fund_then_poll_credits_bankroll() {
    let server = MockServer::start().await;
    let route_id = "a1b2c3d4-e5f6-7890-abcd-ef1234567890";
    let tx_id = "b2c3d4e5-f6a7-8901-bcde-f12345678901";
    setup_mocks(&server, route_id, tx_id).await;

    let (app, chain) = testnet_app(&server).await;
    let session = human_session(&app).await;
    let agent = create_agent(&app, &session).await;

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/funding/rain")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {session}"))
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "agent_id": agent.agent_id.as_ref(),
                        "amount_usdc": 100
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
    assert_eq!(body["status"], "pending");
    assert_eq!(body["simulation_id"], "sim_abc");
    let funding_id = body["funding_id"].as_str().unwrap().to_string();

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/funding/rain/{funding_id}/poll"))
                .header("authorization", format!("Bearer {session}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = res.status();
    let body = json_body(res).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["status"], "settled");
    assert_eq!(body["bankroll_usdc"], 100_000_000);
    assert_eq!(chain.treasury_mirror_call_count(), 1);
    assert_eq!(
        chain.treasury_mirror_micro.load(std::sync::atomic::Ordering::SeqCst),
        100_000_000
    );

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/agents/{}/bankroll", agent.agent_id.as_ref()))
                .header("authorization", format!("Bearer {session}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let br = json_body(res).await;
    assert_eq!(br["available_usdc"], 100_000_000);
}

#[tokio::test]
async fn webhook_settles_and_credits() {
    let server = MockServer::start().await;
    let route_id = "c1b2c3d4-e5f6-7890-abcd-ef1234567890";
    let tx_id = "d2c3d4e5-f6a7-8901-bcde-f12345678901";
    setup_mocks(&server, route_id, tx_id).await;

    let (app, chain) = testnet_app(&server).await;
    let session = human_session(&app).await;
    let agent = create_agent(&app, &session).await;

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/funding/rain")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {session}"))
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "agent_id": agent.agent_id.as_ref(),
                        "amount_usdc": 100
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/webhooks/rain")
                .header("content-type", "application/json")
                .header("Api-Key", "test-rain-key")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "action": "completed",
                        "id": tx_id,
                        "data": {
                            "id": tx_id,
                            "type": "transfer",
                            "transfer": {
                                "status": "settled",
                                "source": { "currency": "usd", "rail": "ach", "amount": "100" },
                                "destination": { "currency": "usdc", "rail": "base", "amount": "100" }
                            }
                        }
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
    assert_eq!(body["handled"], true);
    assert_eq!(body["bankroll_usdc"], 100_000_000);
    assert_eq!(chain.treasury_mirror_call_count(), 1);
}

#[tokio::test]
async fn local_mode_rejects_rain_fund() {
    let state = AppState {
        pool: pool().await,
        auth: AuthStore::new(),
        funding: FundingConfig {
            mode: FundingMode::Local,
            seed_bankroll_usdc: 1_000_000,
            rain: None,
            sandbox_credit_on_simulate: false,
        },
        chain: None,
        nats: None,
        rain: None,
        pending_funding: PendingFundingStore::new(),
        settle_poll: None,
    };
    let app = router(state);
    let session = human_session(&app).await;
    let agent = create_agent(&app, &session).await;

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/funding/rain")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {session}"))
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "agent_id": agent.agent_id.as_ref(),
                        "amount_usdc": 100
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn webhook_rejects_bad_api_key() {
    let server = MockServer::start().await;
    let (app, _) = testnet_app(&server).await;

    let res = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/webhooks/rain")
                .header("content-type", "application/json")
                .header("Api-Key", "wrong-key")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "action": "completed",
                        "id": "x",
                        "data": {
                            "id": "x",
                            "transfer": {
                                "status": "settled",
                                "source": { "amount": "1" }
                            }
                        }
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn webhook_missing_api_key_rejects_and_leaves_bankroll() {
    let server = MockServer::start().await;
    let route_id = "e1b2c3d4-e5f6-7890-abcd-ef1234567890";
    let tx_id = "e2c3d4e5-f6a7-8901-bcde-f12345678901";
    setup_mocks(&server, route_id, tx_id).await;

    let (app, _) = testnet_app(&server).await;
    let session = human_session(&app).await;
    let agent = create_agent(&app, &session).await;

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/funding/rain")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {session}"))
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "agent_id": agent.agent_id.as_ref(),
                        "amount_usdc": 100
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/webhooks/rain")
                .header("content-type", "application/json")
                // no Api-Key
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "action": "completed",
                        "id": tx_id,
                        "data": {
                            "id": tx_id,
                            "type": "transfer",
                            "transfer": {
                                "status": "settled",
                                "source": { "currency": "usd", "rail": "ach", "amount": "100" },
                                "destination": { "currency": "usdc", "rail": "base", "amount": "100" }
                            }
                        }
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/agents/{}/bankroll", agent.agent_id.as_ref()))
                .header("authorization", format!("Bearer {session}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let br = json_body(res).await;
    assert_eq!(br["available_usdc"], 0, "missing Api-Key must not credit");
}

fn settle_webhook_body(tx_id: &str) -> Body {
    Body::from(
        serde_json::to_vec(&json!({
            "action": "completed",
            "id": tx_id,
            "data": {
                "id": tx_id,
                "type": "transfer",
                "transfer": {
                    "status": "settled",
                    "source": { "currency": "usd", "rail": "ach", "amount": "100" },
                    "destination": { "currency": "usdc", "rail": "base", "amount": "100" }
                }
            }
        }))
        .unwrap(),
    )
}

#[tokio::test]
async fn concurrent_webhook_settle_credits_once() {
    let server = MockServer::start().await;
    let route_id = "f1b2c3d4-e5f6-7890-abcd-ef1234567890";
    let tx_id = "f2c3d4e5-f6a7-8901-bcde-f12345678901";
    setup_mocks(&server, route_id, tx_id).await;

    let (app, chain) = testnet_app(&server).await;
    let session = human_session(&app).await;
    let agent = create_agent(&app, &session).await;

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/funding/rain")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {session}"))
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "agent_id": agent.agent_id.as_ref(),
                        "amount_usdc": 100
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let mk = || {
        Request::builder()
            .method("POST")
            .uri("/webhooks/rain")
            .header("content-type", "application/json")
            .header("Api-Key", "test-rain-key")
            .body(settle_webhook_body(tx_id))
            .unwrap()
    };

    let (r1, r2) = tokio::join!(app.clone().oneshot(mk()), app.clone().oneshot(mk()));
    let r1 = r1.unwrap();
    let r2 = r2.unwrap();
    assert_eq!(r1.status(), StatusCode::OK);
    assert_eq!(r2.status(), StatusCode::OK);
    let b1 = json_body(r1).await;
    let b2 = json_body(r2).await;
    assert_eq!(b1["bankroll_usdc"], 100_000_000);
    assert_eq!(b2["bankroll_usdc"], 100_000_000);

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/agents/{}/bankroll", agent.agent_id.as_ref()))
                .header("authorization", format!("Bearer {session}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let br = json_body(res).await;
    assert_eq!(
        br["available_usdc"], 100_000_000,
        "concurrent settle must credit exactly once"
    );
    assert_eq!(
        chain.treasury_mirror_call_count(),
        1,
        "treasury mirror once despite concurrent webhooks"
    );
}

#[tokio::test]
async fn simulate_credit_on_flag_credits_once_webhook_no_double() {
    let server = MockServer::start().await;
    let route_id = "a9b2c3d4-e5f6-7890-abcd-ef1234567890";
    let tx_id = "b9c3d4e5-f6a7-8901-bcde-f12345678901";
    setup_mocks(&server, route_id, tx_id).await;

    let (app, chain) = testnet_app_with_credit_on_simulate(&server, true).await;
    let session = human_session(&app).await;
    let agent = create_agent(&app, &session).await;

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/funding/rain")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {session}"))
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "agent_id": agent.agent_id.as_ref(),
                        "amount_usdc": 100
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = res.status();
    let fund_body = json_body(res).await;
    assert_eq!(status, StatusCode::OK, "{fund_body}");
    assert_eq!(
        fund_body["status"], "settled",
        "credit-on-simulate marks settled"
    );
    assert_eq!(fund_body["bankroll_usdc"], 100_000_000);
    assert!(
        fund_body["credit_note"]
            .as_str()
            .unwrap_or("")
            .contains("NOT real ACH finality"),
        "must label sandbox interim: {fund_body}"
    );
    assert_eq!(chain.treasury_mirror_call_count(), 1);
    let funding_id = fund_body["funding_id"].as_str().unwrap().to_string();

    // Subsequent real webhook settle must not double-credit.
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/webhooks/rain")
                .header("content-type", "application/json")
                .header("Api-Key", "test-rain-key")
                .body(settle_webhook_body(tx_id))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = res.status();
    let wh_body = json_body(res).await;
    assert_eq!(status, StatusCode::OK, "{wh_body}");
    assert_eq!(wh_body["handled"], true);
    assert_eq!(wh_body["bankroll_usdc"], 100_000_000);

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/agents/{}/bankroll", agent.agent_id.as_ref()))
                .header("authorization", format!("Bearer {session}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let br = json_body(res).await;
    assert_eq!(
        br["available_usdc"], 100_000_000,
        "webhook after credit-on-simulate must not double-credit"
    );
    assert_eq!(
        chain.treasury_mirror_call_count(),
        1,
        "treasury mirror once"
    );

    // Poll also idempotent.
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/funding/rain/{funding_id}/poll"))
                .header("authorization", format!("Bearer {session}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = res.status();
    let poll_body = json_body(res).await;
    assert_eq!(status, StatusCode::OK, "{poll_body}");
    assert_eq!(poll_body["bankroll_usdc"], 100_000_000);
    assert_eq!(chain.treasury_mirror_call_count(), 1);
}

#[tokio::test]
async fn fail_webhook_after_credit_on_simulate_keeps_settled() {
    let server = MockServer::start().await;
    let route_id = "e9b2c3d4-e5f6-7890-abcd-ef1234567890";
    let tx_id = "e9c3d4e5-f6a7-8901-bcde-f12345678901";
    setup_mocks(&server, route_id, tx_id).await;

    let (app, _) = testnet_app_with_credit_on_simulate(&server, true).await;
    let session = human_session(&app).await;
    let agent = create_agent(&app, &session).await;

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/funding/rain")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {session}"))
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "agent_id": agent.agent_id.as_ref(),
                        "amount_usdc": 100
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let fund_body = json_body(res).await;
    assert_eq!(fund_body["status"], "settled");
    let funding_id = fund_body["funding_id"].as_str().unwrap().to_string();

    // Late Rain fail after sandbox credit-on-simulate must not flip Settled→Failed.
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/webhooks/rain")
                .header("content-type", "application/json")
                .header("Api-Key", "test-rain-key")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "id": tx_id,
                        "data": {
                            "id": tx_id,
                            "type": "transfer",
                            "transfer": {
                                "status": "failed",
                                "source": { "currency": "usd", "rail": "ach", "amount": "100" },
                                "destination": { "currency": "usdc", "rail": "base", "amount": "100" }
                            }
                        }
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = res.status();
    let wh_body = json_body(res).await;
    assert_eq!(status, StatusCode::OK, "{wh_body}");
    assert_eq!(wh_body["handled"], false);

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/funding/rain/{funding_id}"))
                .header("authorization", format!("Bearer {session}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = res.status();
    let get_body = json_body(res).await;
    assert_eq!(status, StatusCode::OK, "{get_body}");
    assert_eq!(
        get_body["status"], "settled",
        "fail webhook must not flip Settled→Failed after credit-on-simulate"
    );
    assert_eq!(get_body["bankroll_usdc"], 100_000_000);

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/agents/{}/bankroll", agent.agent_id.as_ref()))
                .header("authorization", format!("Bearer {session}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let br = json_body(res).await;
    assert_eq!(br["available_usdc"], 100_000_000);
}

#[tokio::test]
async fn flag_off_simulate_alone_does_not_credit() {
    let server = MockServer::start().await;
    let route_id = "c9b2c3d4-e5f6-7890-abcd-ef1234567890";
    let tx_id = "d9c3d4e5-f6a7-8901-bcde-f12345678901";
    setup_mocks(&server, route_id, tx_id).await;

    let (app, chain) = testnet_app_with_credit_on_simulate(&server, false).await;
    let session = human_session(&app).await;
    let agent = create_agent(&app, &session).await;

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/funding/rain")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {session}"))
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "agent_id": agent.agent_id.as_ref(),
                        "amount_usdc": 100
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
    assert_eq!(body["status"], "pending");
    assert!(body.get("bankroll_usdc").is_none() || body["bankroll_usdc"].is_null());
    assert_eq!(chain.treasury_mirror_call_count(), 0);

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/agents/{}/bankroll", agent.agent_id.as_ref()))
                .header("authorization", format!("Bearer {session}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let br = json_body(res).await;
    assert_eq!(
        br["available_usdc"], 0,
        "flag off: simulate alone must not credit"
    );
}

#[tokio::test]
#[ignore = "live Rain sandbox; needs RAIN_* env"]
async fn live_list_payment_routes_decodes() {
    let base = std::env::var("RAIN_API_BASE_URL").expect("RAIN_API_BASE_URL");
    let key = std::env::var("RAIN_API_KEY").expect("RAIN_API_KEY");
    let user = std::env::var("RAIN_USER_ID").expect("RAIN_USER_ID");
    let client = HttpRainClient::new(RainConfig {
        api_base_url: base,
        api_key: key,
        user_id: user,
        team_id: std::env::var("RAIN_TEAM_ID").ok(),
        destination_address: "0x1234567890abcdef1234567890abcdef12345678".into(),
        destination_rail: "base".into(),
    });
    let routes = RainApi::list_payment_routes(&client)
        .await
        .expect("live list_payment_routes decode");
    eprintln!("live list_payment_routes ok count={}", routes.len());
}
