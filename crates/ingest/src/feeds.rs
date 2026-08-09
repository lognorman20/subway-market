//! Feed URL constants (subway-only).

pub const BASE: &str = "https://api-endpoint.mta.info/Dataservice/mtagtfsfeeds";

/// Known subway trip feed path suffixes (URL path after base; use `%2F` for `/`).
pub const TRIP_FEED_PATHS: &[&str] = &[
    "nyct%2Fgtfs-ace",
    "nyct%2Fgtfs-bdfm",
    "nyct%2Fgtfs-g",
    "nyct%2Fgtfs-jz",
    "nyct%2Fgtfs-nqrw",
    "nyct%2Fgtfs-l",
    "nyct%2Fgtfs",
    "nyct%2Fgtfs-si",
];

pub const ALERTS_PATH: &str = "camsys%2Fsubway-alerts";

pub fn trip_feed_url(path: &str) -> String {
    format!("{BASE}/{path}")
}

pub fn alerts_url() -> String {
    format!("{BASE}/{ALERTS_PATH}")
}
