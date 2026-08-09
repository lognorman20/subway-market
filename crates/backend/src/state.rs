//! Shared Axum state.

use diesel_async::pooled_connection::deadpool::Pool;
use diesel_async::AsyncPgConnection;

#[derive(Clone)]
pub struct AppState {
    pub pool: Pool<AsyncPgConnection>,
}
