//! Backend binary — HTTP API for agents and frontend.

use anyhow::Context;
use backend::{router, AppState};
use diesel_async::pooled_connection::deadpool::Pool;
use diesel_async::pooled_connection::AsyncDieselConnectionManager;
use diesel_async::AsyncPgConnection;
use tracing::info;

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

    info!("connecting postgres");
    let config = AsyncDieselConnectionManager::<AsyncPgConnection>::new(database_url);
    let pool = Pool::builder(config)
        .max_size(5)
        .build()
        .context("build postgres pool")?;
    info!("postgres connected");

    let app = router(AppState { pool });

    transport::mark_ready();
    transport::log_healthy(&format!("backend listening on :{port}"));

    let addr = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .with_context(|| format!("bind {addr}"))?;
    axum::serve(listener, app).await.context("serve")?;

    Ok(())
}
