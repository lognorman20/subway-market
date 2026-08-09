//! Ingest library — poll MTA, parse GTFS-RT, emit dto events.

pub mod alerts;
pub mod decode;
pub mod feeds;
pub mod poll;
pub mod runner;
pub mod stops;

pub use runner::IngestRunner;
