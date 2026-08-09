//! Domain-local types (trip key, freeze record).

use dto::StopEta;

/// In-memory trip working-set entry.
#[derive(Debug, Clone, Default)]
pub struct TripState {
    pub trip_id: String,
    pub route_id: String,
    pub service_date: Option<String>,
    /// NYCT train id when known (market open context).
    pub train_id: Option<String>,
    pub destination_stop_id: String,
    pub remaining: Vec<StopEta>,
    pub frozen_eta_unix: Option<i64>,
    pub freeze_epoch_unix: Option<i64>,
    /// Last ETA for target while still in `remaining` (stop_time source).
    pub last_target_eta_unix: Option<i64>,
    /// Vehicle reported at target stop.
    pub vehicle_at_stop_unix: Option<i64>,
}
