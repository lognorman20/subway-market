//! Full-stack E2E harness for subway-market (slice 11).
//!
//! # Unit vs full-stack
//! - `cargo test -p e2e` — unit pieces (wait helpers, outcome rules, rain gate). No stack.
//! - Full stack: `just e2e` (or `E2E_FULL=1 cargo test -p e2e --test e2e1_lifecycle --test e2e2_human_trade --test e2e3_rain_agent -- --nocapture`)
//!
//! Stack order: `just anvil` → `just deploy-local` → `just up` → seed/Rain → `just start` / `just start-with-frontend` → `just e2e`.
//!
//! Ready waits use ready-files / HTTP health / port open — **no arbitrary sleeps**.

pub mod auth;
pub mod config;
pub mod fund;
pub mod http;
pub mod lifecycle_fixture;
pub mod outcome;
pub mod prices;
pub mod rain;
pub mod wait;

pub use auth::{establish_human_session, HumanSession};
pub use config::{BackendConfig, E2eEnv, RainCreds};
pub use fund::{ensure_platform_funded, mint_platform_usdc};
pub use http::BackendClient;
pub use lifecycle_fixture::{
    emit_open_requested, emit_resolve_requested, open_market_and_wait, wait_for_market_by_trip,
    wait_for_resolved, OpenFixture,
};
pub use outcome::{outcome_from_arrival, is_valid_resolve_outcome, HALF_WAD};
pub use prices::{assert_near_half, parse_wad, price_moved_up, price_moved_down};
pub use rain::{rain_creds_present, rain_skip_reason};
pub use wait::{
    wait_for_condition, wait_for_file, wait_for_http_ok, wait_for_port, WaitError, WaitOpts,
};
