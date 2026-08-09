//! Read-only Diesel query helpers for domain and backend lanes.

use chrono::{DateTime, Utc};
use diesel::prelude::*;
use diesel_async::{AsyncPgConnection, RunQueryDsl};
use serde_json::Value;
use thiserror::Error;

use crate::schema::{bus_events, tracked_trains};

/// Full `tracked_trains` row for HTTP read models.
#[derive(Debug, Clone, Queryable, Selectable)]
#[diesel(table_name = tracked_trains)]
pub struct TrackedTrainRow {
    pub dedupe_key: String,
    pub updated_at: DateTime<Utc>,
    pub phase: String,
    pub route_id: String,
    pub trip_id: String,
    pub nyct_train_id: Option<String>,
    pub first_seen_at: Option<DateTime<Utc>>,
    pub start_stop_id: Option<String>,
    pub target_stop_id: Option<String>,
    pub frozen_at: Option<DateTime<Utc>>,
    pub expected_arrival_unix: Option<i64>,
    pub remaining_stops: Option<i32>,
    pub arrival_unix: Option<i64>,
    pub diff_seconds: Option<i64>,
    pub actual_source: Option<String>,
    pub action: Option<String>,
    pub side: Option<String>,
    pub reason: Option<String>,
    pub diff_id: Option<String>,
    pub freeze_event_id: Option<String>,
    pub diff_event_id: Option<String>,
    pub status_event_id: Option<String>,
}

#[derive(Debug, Error)]
pub enum QueryError {
    #[error("diesel: {0}")]
    Diesel(#[from] diesel::result::Error),
}

/// Load dedupe keys from tracked_trains where diff completed or phase terminal.
pub async fn preload_done_keys(
    conn: &mut AsyncPgConnection,
) -> Result<Vec<Option<String>>, QueryError> {
    let rows = tracked_trains::table
        .select(tracked_trains::dedupe_key)
        .filter(
            tracked_trains::diff_event_id
                .is_not_null()
                .or(tracked_trains::phase.eq_any(&["arrived", "acted"])),
        )
        .load::<String>(conn)
        .await?;
    Ok(rows.into_iter().map(Some).collect())
}

/// Fetch ingest events ordered by feed time, then received_at, then id.
pub async fn fetch_ingest_events(
    conn: &mut AsyncPgConnection,
    subjects: &[&str],
) -> Result<Vec<(String, Value)>, QueryError> {
    use diesel::dsl::sql;
    use diesel::sql_types::{BigInt, Nullable};

    let rows = bus_events::table
        .select((bus_events::subject, bus_events::payload))
        .filter(bus_events::subject.eq_any(subjects))
        .order((
            sql::<Nullable<BigInt>>(
                "COALESCE(
                    (payload->'data'->>'feed_header_unix')::bigint,
                    EXTRACT(EPOCH FROM (payload->'data'->>'observed_at')::timestamptz)::bigint,
                    0
                )",
            ),
            bus_events::received_at,
            bus_events::id,
        ))
        .load::<(String, Value)>(conn)
        .await?;
    Ok(rows)
}

/// Check if ledger tracked_trains has a diff for this dedupe_key.
pub async fn ledger_has_dedupe_key(
    conn: &mut AsyncPgConnection,
    dedupe_key: &str,
) -> Result<bool, QueryError> {
    use diesel::dsl::exists;
    use diesel::select;

    let exists_query = tracked_trains::table
        .filter(tracked_trains::dedupe_key.eq(dedupe_key))
        .filter(
            tracked_trains::diff_event_id
                .is_not_null()
                .or(tracked_trains::phase.eq_any(&["arrived", "acted"])),
        );
    let result = select(exists(exists_query)).get_result::<bool>(conn).await?;
    Ok(result)
}

/// List tracked trains ordered by most recently updated.
pub async fn list_tracked_trains(
    conn: &mut AsyncPgConnection,
    limit: i64,
) -> Result<Vec<TrackedTrainRow>, QueryError> {
    tracked_trains::table
        .order(tracked_trains::updated_at.desc())
        .limit(limit)
        .select(TrackedTrainRow::as_select())
        .load(conn)
        .await
        .map_err(Into::into)
}
