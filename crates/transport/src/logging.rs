//! Shared JSON/pretty tracing init + ready-file gate.
//!
//! ## Levels (filter with `RUST_LOG` / lnav)
//! - **error** — failed ops (handler, decode, ready-file write, DB)
//! - **warn** — unexpected end / degraded / scaffold gaps
//! - **info** — lifecycle + business outcomes (`healthy`, connect, subscribe, freeze/diff)
//! - **debug** — per-event bus/DB traffic, emit/persist echoes (noise under load)
//!
//! Stable messages for grep/jq/lnav: `"healthy"`, `"nats connected"`, `"subscribed"`,
//! `"ready"`, `"freeze"`, `"diff"`.
//!
//! Human prose lives in structured field `detail` (never a second `message` key).
//!
//! ## Format
//! - Default / `LOG_FORMAT=json` — JSON lines (lnav-friendly; used by `just start`).
//! - `LOG_FORMAT=pretty` — compact text for solo foreground runs.

use std::io;
use std::path::Path;

use tracing_subscriber::{fmt, prelude::*, EnvFilter};

/// Build `RUST_LOG` filter, hushing chatty deps unless explicitly named.
pub fn tracing_env_filter() -> EnvFilter {
    let raw = std::env::var("RUST_LOG")
        .unwrap_or_else(|_| "info,async_nats=warn,diesel=warn".to_owned());
    let mut filter = EnvFilter::new(&raw);
    let lower = raw.to_ascii_lowercase();
    if !directive_names_target(&lower, "diesel") {
        filter = filter.add_directive(
            "diesel=warn"
                .parse()
                .expect("diesel=warn is a valid EnvFilter directive"),
        );
    }
    if !directive_names_target(&lower, "async_nats") {
        filter = filter.add_directive(
            "async_nats=warn"
                .parse()
                .expect("async_nats=warn is a valid EnvFilter directive"),
        );
    }
    filter
}

fn directive_names_target(rust_log: &str, target: &str) -> bool {
    rust_log.split(',').any(|part| {
        let name = part.split('=').next().unwrap_or("").trim();
        name == target || name.starts_with(&format!("{target}::"))
    })
}

fn want_pretty() -> bool {
    std::env::var("LOG_FORMAT")
        .map(|v| v.eq_ignore_ascii_case("pretty") || v.eq_ignore_ascii_case("text"))
        .unwrap_or(false)
}

/// Init tracing to **stdout**. Call once from each app `main`.
pub fn init_tracing_stdout(service: &str) {
    let filter = tracing_env_filter();
    if want_pretty() {
        tracing_subscriber::registry()
            .with(filter)
            .with(
                fmt::layer()
                    .with_target(true)
                    .with_level(true)
                    .with_ansi(true)
                    .with_writer(io::stdout),
            )
            .init();
    } else {
        tracing_subscriber::registry()
            .with(filter)
            .with(
                fmt::layer()
                    .json()
                    .with_current_span(false)
                    .with_span_list(false)
                    .with_writer(io::stdout),
            )
            .init();
    }
    tracing::info!(service, "tracing ready");
}

/// Init tracing to **stderr** (TUI owns stdout).
pub fn init_tracing_stderr(service: &str) {
    let filter = tracing_env_filter();
    if want_pretty() {
        tracing_subscriber::registry()
            .with(filter)
            .with(
                fmt::layer()
                    .with_target(true)
                    .with_level(true)
                    .with_ansi(true)
                    .with_writer(io::stderr),
            )
            .init();
    } else {
        tracing_subscriber::registry()
            .with(filter)
            .with(
                fmt::layer()
                    .json()
                    .with_current_span(false)
                    .with_span_list(false)
                    .with_writer(io::stderr),
            )
            .init();
    }
    tracing::info!(service, "tracing ready");
}

/// Info-level “app is serving” marker. Message always `"healthy"` for filters.
pub fn log_healthy(detail: &str) {
    tracing::info!(detail, "healthy");
}

/// If `READY_FILE` is set, write it so `just wait-ready` can unblock.
pub fn mark_ready() {
    let Ok(path) = std::env::var("READY_FILE") else {
        return;
    };
    if path.is_empty() {
        return;
    }
    let path = Path::new(&path);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match std::fs::write(path, b"ready\n") {
        Ok(()) => tracing::info!(ready_file = %path.display(), "ready"),
        Err(err) => {
            tracing::error!(error = %err, ready_file = %path.display(), "ready file write failed")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_explicit_dep_directives() {
        assert!(directive_names_target("info,diesel=debug", "diesel"));
        assert!(directive_names_target("async_nats=info", "async_nats"));
        assert!(!directive_names_target("info,async_nats=warn", "diesel"));
    }
}
