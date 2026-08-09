//! Ledger read-model projections in the same transaction as `bus_events` (`tracked_trains`).

use chrono::{DateTime, TimeZone, Utc};
use diesel::prelude::*;
use diesel_async::{AsyncPgConnection, RunQueryDsl};
use dto::{remaining_stops_to_target, BusEvent, DiffEvent, FreezeEvent, TrainPhase, TripSnapshot};
use tracing::{debug, info};

use crate::writer::WriterError;

/// Frozen rows with `expected_arrival_unix` older than this many seconds are deleted.
pub const STALE_FROZEN_SECS: i64 = 3 * 60 * 60;

fn watch_dedupe_key(trip_id: &str) -> String {
    format!("watch:{trip_id}")
}

/// Same semantic as `FreezeEvent.remaining_stops`.
fn trip_remaining_stops_to_target(trip: &TripSnapshot, target_stop_id: &str) -> i32 {
    i32::try_from(remaining_stops_to_target(&trip.remaining, target_stop_id)).unwrap_or(i32::MAX)
}

fn unix_to_ts(secs: i64) -> DateTime<Utc> {
    Utc.timestamp_opt(secs, 0)
        .single()
        .unwrap_or_else(Utc::now)
}

#[derive(Debug, Default)]
struct RowIdentity {
    nyct_train_id: Option<String>,
    start_stop_id: Option<String>,
    frozen_at: Option<DateTime<Utc>>,
    freeze_event_id: Option<String>,
}

async fn fetch_identity(
    conn: &mut AsyncPgConnection,
    trip_id_param: &str,
    dedupe_key_param: Option<&str>,
) -> Result<RowIdentity, WriterError> {
    use db::schema::tracked_trains::dsl::*;

    let mut query = tracked_trains
        .select((nyct_train_id, start_stop_id, frozen_at, freeze_event_id))
        .into_boxed();

    if let Some(dk) = dedupe_key_param {
        query = query.filter(trip_id.eq(trip_id_param).or(dedupe_key.eq(dk)));
    } else {
        query = query.filter(trip_id.eq(trip_id_param));
    }

    let row: Option<(
        Option<String>,
        Option<String>,
        Option<DateTime<Utc>>,
        Option<String>,
    )> = query
        .order((
            diesel::dsl::sql::<diesel::sql_types::Integer>(
                "CASE phase \
                 WHEN 'acted' THEN 4 \
                 WHEN 'arrived' THEN 3 \
                 WHEN 'frozen' THEN 2 \
                 ELSE 1 END DESC",
            ),
            updated_at.desc(),
        ))
        .first(conn)
        .await
        .optional()?;

    Ok(match row {
        Some((nyct, start, frozen_at_val, freeze_event_id_val)) => RowIdentity {
            nyct_train_id: nyct,
            start_stop_id: start,
            frozen_at: frozen_at_val,
            freeze_event_id: freeze_event_id_val,
        },
        None => RowIdentity::default(),
    })
}

fn trip_start_stop_id(trip: &TripSnapshot, prior: &RowIdentity) -> Option<String> {
    <[_]>::first(&trip.remaining)
        .map(|s| s.stop_id.clone())
        .or_else(|| prior.start_stop_id.clone())
}

fn trip_nyct_train_id<'a>(trip: &'a TripSnapshot, prior: &'a RowIdentity) -> Option<&'a str> {
    trip.nyct_train_id
        .as_deref()
        .or(prior.nyct_train_id.as_deref())
}

/// Delete `frozen` rows whose expected arrival is more than [`STALE_FROZEN_SECS`] in the past.
pub async fn expire_stale_frozen_rows(
    conn: &mut AsyncPgConnection,
) -> Result<u64, WriterError> {
    use db::schema::tracked_trains::dsl::*;

    let cutoff = Utc::now().timestamp() - STALE_FROZEN_SECS;
    let result = diesel::delete(
        tracked_trains
            .filter(phase.eq(TrainPhase::Frozen.as_str()))
            .filter(expected_arrival_unix.is_not_null())
            .filter(expected_arrival_unix.lt(cutoff)),
    )
    .execute(conn)
    .await?;

    let deleted = result as u64;
    if deleted > 0 {
        info!(
            deleted,
            cutoff_unix = cutoff,
            stale_after_secs = STALE_FROZEN_SECS,
            "expired stale frozen tracked_trains rows"
        );
    }
    Ok(deleted)
}

/// Apply projection side-effects for one bus event inside an open transaction.
pub async fn project_event(
    conn: &mut AsyncPgConnection,
    event: &BusEvent,
) -> Result<(), WriterError> {
    match event {
        BusEvent::Trip(trip) => project_trip_snapshot(conn, trip).await?,
        BusEvent::Freeze(freeze) => project_freeze(conn, freeze).await?,
        BusEvent::Diff(diff) => project_diff(conn, diff).await?,
        other => {
            debug!(
                event_id = %other.event_id().as_ref(),
                event_type = other.event_type_name(),
                "projection skipped unsupported event"
            );
        }
    }
    expire_stale_frozen_rows(conn).await?;
    Ok(())
}

async fn project_trip_snapshot(
    conn: &mut AsyncPgConnection,
    trip: &TripSnapshot,
) -> Result<(), WriterError> {
    use db::schema::tracked_trains::dsl::*;

    let dedupe_key_val = watch_dedupe_key(&trip.trip_id);
    let prior = fetch_identity(conn, &trip.trip_id, None).await?;
    let start_stop_id_val = trip_start_stop_id(trip, &prior);
    let nyct_train_id_val = trip_nyct_train_id(trip, &prior);
    let target_stop_id_val = <[_]>::first(&trip.remaining)
        .map(|s| s.stop_id.as_str())
        .unwrap_or(trip.destination_stop_id.as_str());

    let acted_exists: bool = diesel::select(diesel::dsl::exists(
        tracked_trains
            .filter(trip_id.eq(&trip.trip_id))
            .filter(phase.eq(TrainPhase::Acted.as_str())),
    ))
    .get_result(conn)
    .await?;

    if !acted_exists {
        diesel::sql_query(
            r#"
            INSERT INTO tracked_trains (
                dedupe_key, updated_at, phase, route_id, trip_id, nyct_train_id,
                first_seen_at, start_stop_id, target_stop_id
            )
            VALUES ($1, NOW(), $2, $3, $4, $5, NOW(), $6, $7)
            ON CONFLICT (dedupe_key) DO UPDATE SET
                updated_at = NOW(),
                route_id = EXCLUDED.route_id,
                nyct_train_id = COALESCE(EXCLUDED.nyct_train_id, tracked_trains.nyct_train_id),
                start_stop_id = COALESCE(tracked_trains.start_stop_id, EXCLUDED.start_stop_id),
                target_stop_id = EXCLUDED.target_stop_id
            WHERE tracked_trains.phase = $2
            "#,
        )
        .bind::<diesel::sql_types::Text, _>(&dedupe_key_val)
        .bind::<diesel::sql_types::Text, _>(TrainPhase::Watching.as_str())
        .bind::<diesel::sql_types::Text, _>(&trip.route_id)
        .bind::<diesel::sql_types::Text, _>(&trip.trip_id)
        .bind::<diesel::sql_types::Nullable<diesel::sql_types::Text>, _>(nyct_train_id_val)
        .bind::<diesel::sql_types::Nullable<diesel::sql_types::Text>, _>(
            start_stop_id_val.as_deref(),
        )
        .bind::<diesel::sql_types::Text, _>(target_stop_id_val)
        .execute(conn)
        .await?;
    }

    let frozen_targets: Vec<Option<String>> = tracked_trains
        .select(target_stop_id)
        .filter(trip_id.eq(&trip.trip_id))
        .filter(phase.eq(TrainPhase::Frozen.as_str()))
        .filter(arrival_unix.is_null())
        .filter(target_stop_id.is_not_null())
        .load(conn)
        .await?;

    for frozen_target_opt in frozen_targets {
        if let Some(frozen_target) = frozen_target_opt {
            let remaining_val = trip_remaining_stops_to_target(trip, &frozen_target);
        diesel::sql_query(
            r#"
            UPDATE tracked_trains
            SET
                updated_at = NOW(),
                remaining_stops = $1,
                route_id = $2,
                nyct_train_id = COALESCE($3, nyct_train_id),
                start_stop_id = COALESCE(start_stop_id, $4),
                target_stop_id = COALESCE(target_stop_id, $5)
            WHERE trip_id = $6
              AND phase = $7
              AND target_stop_id = $8
              AND arrival_unix IS NULL
            "#,
        )
        .bind::<diesel::sql_types::Integer, _>(remaining_val)
        .bind::<diesel::sql_types::Text, _>(&trip.route_id)
        .bind::<diesel::sql_types::Nullable<diesel::sql_types::Text>, _>(nyct_train_id_val)
        .bind::<diesel::sql_types::Nullable<diesel::sql_types::Text>, _>(
            start_stop_id_val.as_deref(),
        )
        .bind::<diesel::sql_types::Text, _>(target_stop_id_val)
        .bind::<diesel::sql_types::Text, _>(&trip.trip_id)
        .bind::<diesel::sql_types::Text, _>(TrainPhase::Frozen.as_str())
        .bind::<diesel::sql_types::Text, _>(&frozen_target)
        .execute(conn)
        .await?;
        }
    }

    Ok(())
}

async fn project_freeze(
    conn: &mut AsyncPgConnection,
    freeze: &FreezeEvent,
) -> Result<(), WriterError> {
    use db::schema::tracked_trains::dsl::*;

    let frozen_at_val = unix_to_ts(freeze.freeze_epoch_unix);
    let watch_key = watch_dedupe_key(&freeze.trip_id);
    let prior = fetch_identity(conn, &freeze.trip_id, Some(&freeze.dedupe_key)).await?;

    let nyct_train_id_val = prior.nyct_train_id.as_deref();
    let start_stop_id_val = if freeze.from_stop_id.is_empty() {
        prior.start_stop_id.as_deref()
    } else {
        Some(freeze.from_stop_id.as_str())
    };

    diesel::sql_query(
        r#"
        INSERT INTO tracked_trains (
            dedupe_key, updated_at, phase, route_id, trip_id,
            nyct_train_id, start_stop_id,
            target_stop_id, frozen_at, expected_arrival_unix, remaining_stops,
            freeze_event_id, first_seen_at
        )
        VALUES ($1, NOW(), $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, COALESCE($8, NOW()))
        ON CONFLICT (dedupe_key) DO UPDATE SET
            updated_at = NOW(),
            phase = CASE
                WHEN tracked_trains.phase IN ('arrived', 'acted') THEN tracked_trains.phase
                ELSE EXCLUDED.phase
            END,
            route_id = EXCLUDED.route_id,
            trip_id = EXCLUDED.trip_id,
            nyct_train_id = COALESCE(tracked_trains.nyct_train_id, EXCLUDED.nyct_train_id),
            start_stop_id = COALESCE(EXCLUDED.start_stop_id, tracked_trains.start_stop_id),
            target_stop_id = EXCLUDED.target_stop_id,
            frozen_at = COALESCE(tracked_trains.frozen_at, EXCLUDED.frozen_at),
            expected_arrival_unix = COALESCE(
                tracked_trains.expected_arrival_unix,
                EXCLUDED.expected_arrival_unix
            ),
            remaining_stops = COALESCE(tracked_trains.remaining_stops, EXCLUDED.remaining_stops),
            freeze_event_id = COALESCE(tracked_trains.freeze_event_id, EXCLUDED.freeze_event_id)
        WHERE tracked_trains.phase IN ($2, 'watching', 'arrived', 'acted')
        "#,
    )
    .bind::<diesel::sql_types::Text, _>(&freeze.dedupe_key)
    .bind::<diesel::sql_types::Text, _>(TrainPhase::Frozen.as_str())
    .bind::<diesel::sql_types::Text, _>(&freeze.route_id)
    .bind::<diesel::sql_types::Text, _>(&freeze.trip_id)
    .bind::<diesel::sql_types::Nullable<diesel::sql_types::Text>, _>(nyct_train_id_val)
    .bind::<diesel::sql_types::Nullable<diesel::sql_types::Text>, _>(start_stop_id_val)
    .bind::<diesel::sql_types::Text, _>(&freeze.target_stop_id)
    .bind::<diesel::sql_types::Timestamptz, _>(frozen_at_val)
    .bind::<diesel::sql_types::BigInt, _>(freeze.frozen_eta_unix)
    .bind::<diesel::sql_types::Nullable<diesel::sql_types::Integer>, _>(
        i32::try_from(freeze.remaining_stops).ok(),
    )
    .bind::<diesel::sql_types::Text, _>(freeze.event_id.as_ref())
    .execute(conn)
    .await?;

    let acted_exists: bool = diesel::select(diesel::dsl::exists(
        tracked_trains
            .filter(trip_id.eq(&freeze.trip_id))
            .filter(phase.eq(TrainPhase::Acted.as_str())),
    ))
    .get_result(conn)
    .await?;

    if !acted_exists {
        diesel::delete(
            tracked_trains
                .filter(dedupe_key.eq(&watch_key))
                .filter(phase.eq(TrainPhase::Watching.as_str())),
        )
        .execute(conn)
        .await?;
    }

    Ok(())
}

async fn project_diff(
    conn: &mut AsyncPgConnection,
    diff: &DiffEvent,
) -> Result<(), WriterError> {
    use db::schema::tracked_trains::dsl::*;

    let watch_key = watch_dedupe_key(&diff.trip_id);
    let prior = fetch_identity(conn, &diff.trip_id, Some(&diff.dedupe_key)).await?;
    let nyct_train_id_val = prior.nyct_train_id.as_deref();
    let start_stop_id_val = prior.start_stop_id.as_deref();
    let frozen_at_val = prior.frozen_at;
    let freeze_event_id_val = prior.freeze_event_id.as_deref();

    let actual_source_str =
        serde_json::to_string(&diff.actual_source).unwrap_or_else(|_| "null".into());

    diesel::sql_query(
        r#"
        INSERT INTO tracked_trains (
            dedupe_key, updated_at, phase, route_id, trip_id,
            nyct_train_id, start_stop_id,
            target_stop_id, frozen_at, expected_arrival_unix, arrival_unix, remaining_stops,
            freeze_event_id, diff_seconds, actual_source, diff_id, diff_event_id
        )
        VALUES ($1, NOW(), $2, $3, $4, $5, $6, $7, $8, $9, $10, 0, $11, $12, $13, $14, $15)
        ON CONFLICT (dedupe_key) DO UPDATE SET
            updated_at = NOW(),
            phase = $2,
            route_id = EXCLUDED.route_id,
            trip_id = EXCLUDED.trip_id,
            nyct_train_id = COALESCE(tracked_trains.nyct_train_id, EXCLUDED.nyct_train_id),
            start_stop_id = COALESCE(tracked_trains.start_stop_id, EXCLUDED.start_stop_id),
            target_stop_id = EXCLUDED.target_stop_id,
            frozen_at = COALESCE(tracked_trains.frozen_at, EXCLUDED.frozen_at),
            expected_arrival_unix = COALESCE(
                tracked_trains.expected_arrival_unix,
                EXCLUDED.expected_arrival_unix
            ),
            arrival_unix = EXCLUDED.arrival_unix,
            remaining_stops = 0,
            freeze_event_id = COALESCE(tracked_trains.freeze_event_id, EXCLUDED.freeze_event_id),
            diff_seconds = EXCLUDED.diff_seconds,
            actual_source = EXCLUDED.actual_source,
            diff_id = EXCLUDED.diff_id,
            diff_event_id = EXCLUDED.diff_event_id
        WHERE tracked_trains.phase IN ('frozen', $2)
        "#,
    )
    .bind::<diesel::sql_types::Text, _>(&diff.dedupe_key)
    .bind::<diesel::sql_types::Text, _>(TrainPhase::Arrived.as_str())
    .bind::<diesel::sql_types::Text, _>(&diff.route_id)
    .bind::<diesel::sql_types::Text, _>(&diff.trip_id)
    .bind::<diesel::sql_types::Nullable<diesel::sql_types::Text>, _>(nyct_train_id_val)
    .bind::<diesel::sql_types::Nullable<diesel::sql_types::Text>, _>(start_stop_id_val)
    .bind::<diesel::sql_types::Text, _>(&diff.target_stop_id)
    .bind::<diesel::sql_types::Nullable<diesel::sql_types::Timestamptz>, _>(frozen_at_val)
    .bind::<diesel::sql_types::BigInt, _>(diff.frozen_eta_unix)
    .bind::<diesel::sql_types::BigInt, _>(diff.actual_unix)
    .bind::<diesel::sql_types::Nullable<diesel::sql_types::Text>, _>(freeze_event_id_val)
    .bind::<diesel::sql_types::BigInt, _>(diff.diff_seconds)
    .bind::<diesel::sql_types::Text, _>(&actual_source_str)
    .bind::<diesel::sql_types::Text, _>(diff.diff_id.as_ref())
    .bind::<diesel::sql_types::Text, _>(diff.event_id.as_ref())
    .execute(conn)
    .await?;

    diesel::delete(
        tracked_trains
            .filter(dedupe_key.eq(&watch_key))
            .filter(phase.eq(TrainPhase::Watching.as_str())),
    )
    .execute(conn)
    .await?;

    Ok(())
}
