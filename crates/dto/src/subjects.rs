//! NATS subject constants (v1).

/// Watchable route set from alerts filter.
pub const INGEST_WATCHABLE: &str = "subway.ingest.watchable";

/// Trip stop-time snapshot from GTFS-RT TripUpdate.
pub const INGEST_TRIP: &str = "subway.ingest.trip";

/// Vehicle position update.
pub const INGEST_VEHICLE: &str = "subway.ingest.vehicle";

/// Domain freeze locked (≤3 stops to target).
pub const DOMAIN_FREEZE: &str = "subway.domain.freeze";

/// Domain prediction-diff signal.
pub const DOMAIN_DIFF: &str = "subway.domain.diff";

/// Ledger (and any global observer) subscribe pattern.
pub const ALL_SUBWAY: &str = "subway.>";
