//! Observe binary — read-only TUI over ledger Postgres.

use anyhow::Context;
use diesel_async::pooled_connection::deadpool::Pool;
use diesel_async::pooled_connection::AsyncDieselConnectionManager;
use diesel_async::AsyncPgConnection;
use tracing::info;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    transport::init_tracing_stderr("observe");
    info!("boot lane=observe");

    let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgres://subway:subway@127.0.0.1:5433/subway_ledger".into()
    });

    info!("connecting postgres");
    let config = AsyncDieselConnectionManager::<AsyncPgConnection>::new(database_url);
    let pool = Pool::builder(config)
        .max_size(2)
        .build()
        .context("build postgres pool")?;
    info!("postgres connected");

    transport::log_healthy("observe TUI starting");

    observe::ui::run(pool).await?;

    Ok(())
}
