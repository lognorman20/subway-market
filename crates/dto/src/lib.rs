//! Shared data transfer objects for the subway-market stack.
//!
//! All bus payloads carry [`EventMeta::event_id`]. Extend carefully — serde stable.

pub mod api;
pub mod events;
pub mod ids;
pub mod subjects;

pub use api::*;
pub use events::*;
pub use ids::*;
pub use subjects::*;
