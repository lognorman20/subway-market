//! Ingest binary — poll MTA GTFS-RT → emit.

use anyhow::Context;
use tracing::info;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    transport::init_tracing_stdout("ingest");
    info!("boot");

    let nats_url = std::env::var("NATS_URL").unwrap_or_else(|_| "nats://127.0.0.1:4223".into());
    let transport = transport::connect(&nats_url)
        .await
        .context("connect NATS")?;

    transport::mark_ready();
    transport::log_healthy("nats connected; starting poll loop");

    let mut runner = ingest::IngestRunner::new();
    runner.run(transport).await
}
