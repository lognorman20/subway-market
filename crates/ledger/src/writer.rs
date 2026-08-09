//! DB writer — UNIQUE event_id, extract diff_id, ledger projections.

use diesel::prelude::*;
use diesel_async::pooled_connection::deadpool::Pool;
use diesel_async::{AsyncPgConnection, RunQueryDsl};
use dto::BusEvent;
use serde_json::Value;
use thiserror::Error;
use tracing::error;

use crate::projection::project_event;

#[derive(Debug, Error)]
pub enum WriterError {
    #[error("diesel: {0}")]
    Diesel(#[from] diesel::result::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("pool: {0}")]
    Pool(String),
    #[error("projection: {0}")]
    Projection(String),
}

pub type DbPool = Pool<AsyncPgConnection>;

pub struct DbWriter {
    pool: DbPool,
}

impl DbWriter {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    pub async fn insert(&self, subject: &str, event: &BusEvent) -> Result<(), WriterError> {
        let event_id_str = event.event_id().as_ref();
        let diff_id_str = event.diff_id().map(|d| d.as_ref());
        let event_type = event.event_type();
        let event_type_str = event_type.as_str();
        let payload: Value = serde_json::to_value(event)?;
        let actual_unix_val = match event {
            BusEvent::Diff(diff) => Some(diff.actual_unix),
            _ => None,
        };

        let mut conn = self
            .pool
            .get()
            .await
            .map_err(|e| WriterError::Pool(e.to_string()))?;

        conn.build_transaction()
            .run::<(), WriterError, _>(|conn| {
                Box::pin(async move {
                    use db::schema::bus_events;

                    diesel::insert_into(bus_events::table)
                        .values((
                            bus_events::event_id.eq(event_id_str),
                            bus_events::diff_id.eq(diff_id_str),
                            bus_events::subject.eq(subject),
                            bus_events::payload.eq(&payload),
                            bus_events::event_type.eq(event_type_str),
                            bus_events::actual_unix.eq(actual_unix_val),
                        ))
                        .on_conflict(bus_events::event_id)
                        .do_nothing()
                        .execute(conn)
                        .await
                        .map_err(|err| {
                            error!(
                                error = %err,
                                event_id = event_id_str,
                                subject,
                                event_type = event_type_str,
                                "bus_events insert failed"
                            );
                            WriterError::Diesel(err)
                        })?;

                    project_event(conn, event).await.map_err(|err| {
                        error!(
                            error = %err,
                            event_id = event_id_str,
                            subject,
                            event_type = event_type_str,
                            "ledger projection failed"
                        );
                        err
                    })?;

                    Ok(())
                })
            })
            .await
            .map_err(|err| {
                error!(
                    error = %err,
                    event_id = event_id_str,
                    subject,
                    "ledger transaction commit failed"
                );
                err
            })?;

        Ok(())
    }
}
