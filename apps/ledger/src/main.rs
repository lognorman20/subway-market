//! Ledger binary — subscribe subway.> → Postgres (trains + trade/PnL projections).

use std::sync::Arc;

use anyhow::Context;
use diesel_async::pooled_connection::deadpool::Pool;
use diesel_async::pooled_connection::AsyncDieselConnectionManager;
use diesel_async::AsyncPgConnection;
use ledger::handler::LedgerHandler;
use ledger::writer::DbWriter;
use tracing::info;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    transport::init_tracing_stdout("ledger");
    info!("boot");

    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://subway:subway@127.0.0.1:5433/subway_ledger".into());
    let nats_url = std::env::var("NATS_URL").unwrap_or_else(|_| "nats://127.0.0.1:4223".into());

    info!("connecting postgres");
    let config = AsyncDieselConnectionManager::<AsyncPgConnection>::new(database_url);
    let pool = Pool::builder(config)
        .max_size(5)
        .build()
        .context("build postgres pool")?;
    info!("postgres connected");

    let writer = Arc::new(DbWriter::new(pool));
    let handler = Arc::new(LedgerHandler { writer });
    let bus_client = transport::connect(&nats_url)
        .await
        .context("connect NATS")?;

    let subjects = [dto::ALL_SUBWAY];
    info!(?subjects, "starting handler");

    transport::run_handler(bus_client, &subjects, handler)
        .await
        .context("ledger handler")?;

    Ok(())
}
