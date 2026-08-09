//! Read-only `tracked_trains` queries for the observe TUI.

use db::queries::{QueryError, TrackedTrainRow};
use db::schema::tracked_trains;
use diesel::prelude::*;
use diesel_async::{AsyncPgConnection, RunQueryDsl};

const UPCOMING_PHASES: &[&str] = &["watching", "frozen"];
const HISTORY_PHASES: &[&str] = &["arrived", "acted"];

/// Trains still being watched or frozen (not yet scored).
pub async fn load_upcoming(
    conn: &mut AsyncPgConnection,
    limit: i64,
) -> Result<Vec<TrackedTrainRow>, QueryError> {
    tracked_trains::table
        .filter(tracked_trains::phase.eq_any(UPCOMING_PHASES))
        .order(tracked_trains::updated_at.desc())
        .limit(limit)
        .select(TrackedTrainRow::as_select())
        .load(conn)
        .await
        .map_err(Into::into)
}

/// Scored or terminal rows — phase `arrived`/`acted` or any row with `diff_seconds`.
pub async fn load_history(
    conn: &mut AsyncPgConnection,
    limit: i64,
) -> Result<Vec<TrackedTrainRow>, QueryError> {
    tracked_trains::table
        .filter(
            tracked_trains::phase
                .eq_any(HISTORY_PHASES)
                .or(tracked_trains::diff_seconds.is_not_null()),
        )
        .order(tracked_trains::updated_at.desc())
        .limit(limit)
        .select(TrackedTrainRow::as_select())
        .load(conn)
        .await
        .map_err(Into::into)
}
