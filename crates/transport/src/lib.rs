//! Event transport helpers (NATS v1).
//!
//! Apps call [`emit`] and [`run_handler`]. Business logic stays in lane handlers.

mod client;
mod error;
mod handler;
pub mod logging;

pub use client::{connect, run_handler, TransportClient};
pub use error::TransportError;
pub use handler::EventHandler;
pub use logging::{
    init_tracing_stderr, init_tracing_stdout, log_healthy, mark_ready, tracing_env_filter,
};
