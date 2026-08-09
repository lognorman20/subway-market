//! Frontend binary — HTMX UI over the backend HTTP API.

use anyhow::Context;
use tracing::info;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    transport::init_tracing_stdout("frontend");
    info!("boot lane={}", frontend::name());

    let config = frontend::Config::from_env();
    info!(%config.bind_addr, backend = %config.backend_url, "config");

    let state = frontend::AppState::new(config.backend_url.clone());
    let app = frontend::router(state);

    transport::mark_ready();
    transport::log_healthy("frontend listening");

    let listener = tokio::net::TcpListener::bind(config.bind_addr)
        .await
        .with_context(|| format!("bind {}", config.bind_addr))?;
    axum::serve(listener, app).await.context("serve")?;

    Ok(())
}
