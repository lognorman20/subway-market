use anyhow::Context;
use diesel::pg::PgConnection;
use diesel::Connection;
use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};
use tracing::info;

pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations");

/// Apply embedded migrations synchronously (invoked by `ledger-migrate`).
pub fn run_migrations(database_url: &str) -> anyhow::Result<()> {
    let mut conn = PgConnection::establish(database_url)
        .with_context(|| format!("connect postgres at {database_url}"))?;
    info!("running diesel migrations");
    conn.run_pending_migrations(MIGRATIONS)
        .map_err(|e| anyhow::anyhow!("migration failed: {e}"))?;
    Ok(())
}
