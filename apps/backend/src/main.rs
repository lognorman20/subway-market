//! Backend binary — HTTP API for agents and frontend + market lifecycle bus.

use std::sync::Arc;

use anyhow::Context;
use backend::{
    router, trading_chain_connect, AppState, AuthStore, FundingConfig, LifecycleHandler,
    LIFECYCLE_SUBJECTS,
};
use diesel_async::pooled_connection::deadpool::Pool;
use diesel_async::pooled_connection::AsyncDieselConnectionManager;
use diesel_async::AsyncPgConnection;
use tracing::{error, info, warn};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    transport::init_tracing_stdout("backend");
    info!("boot lane={}", backend::name());

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(8080);
    let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgres://subway:subway@127.0.0.1:5433/subway_ledger".into()
    });
    let nats_url =
        std::env::var("NATS_URL").unwrap_or_else(|_| "nats://127.0.0.1:4223".into());

    info!("connecting postgres");
    let config = AsyncDieselConnectionManager::<AsyncPgConnection>::new(database_url);
    let pool = Pool::builder(config)
        .max_size(5)
        .build()
        .context("build postgres pool")?;
    info!("postgres connected");

    // Boot continues without NATS (health/reads ok); buy/sell return 503 until bus is up.
    let nats = match transport::connect(&nats_url).await {
        Ok(client) => {
            info!(%nats_url, "nats connected");
            Some(client)
        }
        Err(err) => {
            warn!(
                error = %err,
                "nats unavailable — buy/sell + lifecycle disabled until bus connects"
            );
            None
        }
    };

    let chain = trading_chain_connect();
    if chain.is_some() {
        info!("chain client ready");
    } else {
        warn!("chain not configured (SUBWAY_MARKET_ADDRESS/USDC_ADDRESS) — buy/sell/lifecycle unavailable");
    }

    // Domain intents → chain create/resolve → market opened/resolved facts.
    if let (Some(nats_lc), Some(chain_lc)) = (nats.clone(), chain.clone()) {
        let handler = Arc::new(LifecycleHandler::new(chain_lc, nats_lc.clone()));
        tokio::spawn(async move {
            if let Err(err) =
                transport::run_handler(nats_lc, LIFECYCLE_SUBJECTS, handler).await
            {
                error!(error = %err, "lifecycle bus handler stopped");
            }
        });
        info!("lifecycle handler subscribed");
    } else {
        warn!("lifecycle handler not started (need nats + chain)");
    }

    let funding = FundingConfig::from_env();
    info!(?funding.mode, seed = funding.seed_bankroll_usdc, "funding");

    let app = router(AppState {
        pool,
        auth: AuthStore::new(),
        funding,
        chain,
        nats,
    });

    transport::mark_ready();
    transport::log_healthy(&format!("backend listening on :{port}"));

    let addr = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .with_context(|| format!("bind {addr}"))?;
    axum::serve(listener, app).await.context("serve")?;

    Ok(())
}
