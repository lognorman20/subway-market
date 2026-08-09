//! Ledger replay rebuild.
//!
//! Order: subscribe+buffer → preload DiffEvent keys → replay ingest emit=false → drain live.

use diesel_async::pooled_connection::deadpool::Pool;
use diesel_async::AsyncPgConnection;
use dto::{BusEvent, INGEST_TRIP, INGEST_VEHICLE, INGEST_WATCHABLE};
use serde_json::Value;
use thiserror::Error;
use tracing::{debug, error, warn};

use crate::engine::{apply, DomainEmit, EmitMode};
use crate::state::TripStore;

#[derive(Debug, Error)]
pub enum ReplayError {
    #[error("db: {0}")]
    Db(#[from] db::QueryError),
    #[error("pool: {0}")]
    Pool(#[from] diesel_async::pooled_connection::deadpool::PoolError),
    #[error("connection: {0}")]
    Connection(#[from] diesel_async::pooled_connection::deadpool::BuildError),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
}

/// Decode a ledger `bus_events.payload` jsonb value into a [`BusEvent`].
pub fn decode_ledger_payload(payload: &Value) -> Result<BusEvent, ReplayError> {
    Ok(serde_json::from_value(payload.clone())?)
}

/// Extract dedupe key from a stored DiffEvent payload (tagged `BusEvent` JSON).
pub fn extract_dedupe_key(payload: &Value) -> Option<String> {
    payload
        .get("data")
        .and_then(|d| d.get("dedupe_key"))
        .and_then(|v| v.as_str())
        .map(str::to_string)
}

/// Feed-time sort key matching ledger replay SQL ordering (feed_header_unix, else observed_at).
pub fn ingest_feed_unix(payload: &Value) -> i64 {
    if let Some(ts) = payload
        .get("data")
        .and_then(|d| d.get("feed_header_unix"))
        .and_then(|v| v.as_i64())
    {
        return ts;
    }
    payload
        .get("data")
        .and_then(|d| d.get("observed_at"))
        .and_then(|v| v.as_str())
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.timestamp())
        .unwrap_or(0)
}

/// Apply dedupe-key rows from preload query into the store done-set.
pub fn apply_preloaded_done_keys(store: &mut TripStore, rows: &[(Option<String>,)]) {
    for (key,) in rows {
        if let Some(key) = key {
            store.mark_done(key);
        }
    }
}

/// Replay ordered events through the engine (tests + ledger rebuild).
pub fn replay_events(store: &mut TripStore, events: &[(String, BusEvent)], mode: EmitMode) {
    for (subject, event) in events {
        let event_id = event.event_id().as_ref().to_string();
        for emit in apply(store, event, mode) {
            match emit {
                DomainEmit::Freeze(freeze) => {
                    debug!(
                        subject,
                        event_id = %event_id,
                        trip_id = %freeze.trip_id,
                        dedupe_key = %freeze.dedupe_key,
                        "replay suppressed freeze emit"
                    );
                }
                DomainEmit::Diff(diff) => {
                    debug!(
                        subject,
                        event_id = %event_id,
                        diff_id = %diff.diff_id.as_ref(),
                        trip_id = %diff.trip_id,
                        "replay suppressed diff emit"
                    );
                }
                DomainEmit::MarketOpenRequested(req) => {
                    debug!(
                        subject,
                        event_id = %event_id,
                        dedupe_key = %req.dedupe_key,
                        "replay suppressed market.open_requested"
                    );
                }
                DomainEmit::MarketResolveRequested(req) => {
                    debug!(
                        subject,
                        event_id = %event_id,
                        dedupe_key = %req.dedupe_key,
                        "replay suppressed market.resolve_requested"
                    );
                }
            }
        }
    }
}

/// Load completed dedupe keys from ledger `tracked_trains` projection.
pub async fn preload_done_keys(
    pool: &Pool<AsyncPgConnection>,
    store: &mut TripStore,
) -> Result<(), ReplayError> {
    let mut conn = pool.get().await?;
    let rows = db::preload_done_keys(&mut conn).await?;
    let tuples: Vec<(Option<String>,)> = rows.into_iter().map(|k| (k,)).collect();
    apply_preloaded_done_keys(store, &tuples);
    Ok(())
}

/// Fetch ingest events ordered by feed time, then ledger receive order.
pub async fn fetch_ingest_events(
    pool: &Pool<AsyncPgConnection>,
) -> Result<Vec<(String, BusEvent)>, ReplayError> {
    let mut conn = pool.get().await?;
    let subjects = &[INGEST_WATCHABLE, INGEST_TRIP, INGEST_VEHICLE];
    let rows = db::fetch_ingest_events(&mut conn, subjects).await?;

    let mut out = Vec::with_capacity(rows.len());
    let mut skipped = 0usize;
    for (subject, payload) in rows {
        match decode_ledger_payload(&payload) {
            Ok(event) => out.push((subject, event)),
            Err(err) => {
                skipped += 1;
                warn!(
                    error = %err,
                    %subject,
                    "skipping corrupt ledger row during replay"
                );
            }
        }
    }
    if skipped > 0 {
        warn!(
            skipped,
            loaded = out.len(),
            "replay skipped corrupt ledger rows"
        );
    }
    Ok(out)
}

/// True when ledger `tracked_trains` already recorded a diff for this dedupe key.
pub async fn ledger_has_dedupe_key(
    pool: &Pool<AsyncPgConnection>,
    dedupe_key: &str,
) -> Result<bool, ReplayError> {
    let mut conn = pool.get().await?;
    let result = db::ledger_has_dedupe_key(&mut conn, dedupe_key).await?;
    Ok(result)
}

/// Run startup rebuild against Postgres.
pub async fn rebuild_from_ledger(
    database_url: &str,
    store: &mut TripStore,
) -> Result<(), ReplayError> {
    let pool = connect_pool(database_url).await.map_err(|err| {
        error!(error = %err, "ledger replay postgres connect failed");
        err
    })?;

    preload_done_keys(&pool, store).await.map_err(|err| {
        error!(error = %err, "ledger replay preload done keys failed");
        err
    })?;
    let events = fetch_ingest_events(&pool).await.map_err(|err| {
        error!(error = %err, "ledger replay fetch ingest events failed");
        err
    })?;
    replay_events(store, &events, EmitMode::Silent);
    Ok(())
}

/// Connect pool for live dedupe checks (optional).
pub async fn connect_pool(database_url: &str) -> Result<Pool<AsyncPgConnection>, ReplayError> {
    use diesel_async::pooled_connection::deadpool::Pool;
    use diesel_async::pooled_connection::AsyncDieselConnectionManager;

    let config = AsyncDieselConnectionManager::<AsyncPgConnection>::new(database_url);
    let pool = Pool::builder(config).max_size(2).build()?;
    Ok(pool)
}
