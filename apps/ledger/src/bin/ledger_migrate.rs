//! Apply ledger schema. Invoked by `just migrate-db`.

use anyhow::Context;
use tracing::info;

fn main() -> anyhow::Result<()> {
    transport::init_tracing_stdout("ledger-migrate");
    info!("boot");

    let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgres://subway:subway@127.0.0.1:5433/subway_ledger".into()
    });

    info!("migrating schema");
    db::run_migrations(&database_url).context("migrate")?;
    info!("migrate done");
    transport::log_healthy("schema applied");

    Ok(())
}
