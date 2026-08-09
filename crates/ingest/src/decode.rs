//! Protobuf decode (GTFS-RT + NYCT).

use dto::StopEta;
use prost::Message;
use thiserror::Error;

pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/transit_realtime.rs"));
}

use proto::{
    trip_descriptor::ScheduleRelationship as TripScheduleRelationship, Alert, FeedMessage,
    TripUpdate, VehiclePosition,
};

#[derive(Debug, Error)]
pub enum DecodeError {
    #[error("protobuf decode failed: {0}")]
    Protobuf(#[from] prost::DecodeError),
    #[error("feed missing header timestamp")]
    MissingTimestamp,
}

/// Parsed trip update ready for dto mapping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedTrip {
    pub trip_id: String,
    pub route_id: String,
    pub service_date: Option<String>,
    pub remaining: Vec<StopEta>,
    pub destination_stop_id: String,
    pub nyct_train_id: Option<String>,
}

/// Parsed vehicle position ready for dto mapping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedVehicle {
    pub trip_id: String,
    pub route_id: String,
    pub stop_id: Option<String>,
    pub vehicle_unix: Option<i64>,
}

/// Parsed GTFS-RT feed body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedFeed {
    pub header_unix: i64,
    pub trips: Vec<DecodedTrip>,
    pub vehicles: Vec<DecodedVehicle>,
}

/// Decode a raw trip/vehicle feed body.
pub fn decode_feed(bytes: &[u8]) -> Result<DecodedFeed, DecodeError> {
    let message = FeedMessage::decode(bytes)?;
    decode_feed_message(&message)
}

fn decode_feed_message(message: &FeedMessage) -> Result<DecodedFeed, DecodeError> {
    let header_unix = message
        .header
        .timestamp
        .map(|ts| ts as i64)
        .ok_or(DecodeError::MissingTimestamp)?;

    let mut trips = Vec::new();
    let mut vehicles = Vec::new();

    for entity in &message.entity {
        if let Some(trip_update) = &entity.trip_update {
            if let Some(decoded) = decode_trip_update(trip_update) {
                trips.push(decoded);
            }
        }
        if let Some(vehicle) = &entity.vehicle {
            if let Some(decoded) = decode_vehicle(vehicle) {
                vehicles.push(decoded);
            }
        }
    }

    Ok(DecodedFeed {
        header_unix,
        trips,
        vehicles,
    })
}

fn decode_trip_update(trip_update: &TripUpdate) -> Option<DecodedTrip> {
    let trip = &trip_update.trip;
    let trip_id = trip.trip_id.clone()?;
    let route_id = trip.route_id.clone()?;

    if matches!(
        trip.schedule_relationship(),
        TripScheduleRelationship::Canceled
    ) {
        return None;
    }

    let service_date = trip.start_date.clone();
    let nyct_train_id = trip
        .nyct_trip_descriptor
        .as_ref()
        .and_then(|nyct| nyct.train_id.clone());

    let remaining = remaining_stop_etas(&trip_update.stop_time_update);
    let destination_stop_id = terminus_stop_id(&trip_update.stop_time_update)?;

    Some(DecodedTrip {
        trip_id,
        route_id,
        service_date,
        remaining,
        destination_stop_id,
        nyct_train_id,
    })
}

/// Last stop in the full schedule list (includes past stops when present).
fn terminus_stop_id(
    stop_time_updates: &[proto::trip_update::StopTimeUpdate],
) -> Option<String> {
    stop_time_updates
        .iter()
        .rev()
        .find_map(|stop| stop.stop_id.clone())
}

fn remaining_stop_etas(
    stop_time_updates: &[proto::trip_update::StopTimeUpdate],
) -> Vec<StopEta> {
    stop_time_updates
        .iter()
        .filter_map(|stop| {
            let stop_id = stop.stop_id.clone()?;
            let eta_unix = stop
                .arrival
                .as_ref()
                .and_then(|event| event.time)
                .or_else(|| stop.departure.as_ref().and_then(|event| event.time))?;
            Some(StopEta { stop_id, eta_unix })
        })
        .collect()
}

fn decode_vehicle(vehicle: &VehiclePosition) -> Option<DecodedVehicle> {
    let trip = vehicle.trip.as_ref()?;
    let trip_id = trip.trip_id.clone()?;
    let route_id = trip.route_id.clone()?;
    let stop_id = vehicle.stop_id.clone();
    let vehicle_unix = vehicle.timestamp.map(|ts| ts as i64);

    Some(DecodedVehicle {
        trip_id,
        route_id,
        stop_id,
        vehicle_unix,
    })
}

/// Decode an alerts feed body into raw alert entities.
pub fn decode_alerts(bytes: &[u8]) -> Result<Vec<Alert>, DecodeError> {
    let message = FeedMessage::decode(bytes)?;
    Ok(message
        .entity
        .into_iter()
        .filter_map(|entity| entity.alert)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use proto::{
        FeedEntity, FeedHeader, TripDescriptor, TripUpdate, trip_update::StopTimeUpdate,
    };

    fn sample_feed_bytes() -> Vec<u8> {
        let message = FeedMessage {
            header: FeedHeader {
                gtfs_realtime_version: "2.0".into(),
                incrementality: None,
                timestamp: Some(1_700_000_000),
                feed_version: None,
                nyct_feed_header: Some(proto::NyctFeedHeader {
                    nyct_subway_version: "1.0".into(),
                    trip_replacement_period: vec![],
                }),
            },
            entity: vec![FeedEntity {
                id: "1".into(),
                is_deleted: None,
                trip_update: Some(TripUpdate {
                    trip: TripDescriptor {
                        trip_id: Some("ACE..N".into()),
                        route_id: Some("A".into()),
                        start_date: Some("20260725".into()),
                        ..Default::default()
                    },
                    stop_time_update: vec![
                        StopTimeUpdate {
                            stop_id: Some("A24".into()),
                            arrival: Some(proto::trip_update::StopTimeEvent {
                                time: Some(1_700_000_100),
                                ..Default::default()
                            }),
                            ..Default::default()
                        },
                        StopTimeUpdate {
                            stop_id: Some("A32".into()),
                            arrival: Some(proto::trip_update::StopTimeEvent {
                                time: Some(1_700_000_500),
                                ..Default::default()
                            }),
                            ..Default::default()
                        },
                    ],
                    ..Default::default()
                }),
                vehicle: None,
                alert: None,
                shape: None,
                stop: None,
                trip_modifications: None,
            }],
        };
        message.encode_to_vec()
    }

    #[test]
    fn decodes_trip_with_stop_etas() {
        let feed = decode_feed(&sample_feed_bytes()).unwrap();
        assert_eq!(feed.header_unix, 1_700_000_000);
        assert_eq!(feed.trips.len(), 1);
        let trip = &feed.trips[0];
        assert_eq!(trip.trip_id, "ACE..N");
        assert_eq!(trip.route_id, "A");
        assert_eq!(trip.service_date.as_deref(), Some("20260725"));
        assert_eq!(trip.remaining.len(), 2);
        assert_eq!(trip.remaining[0].stop_id, "A24");
        assert_eq!(trip.remaining[0].eta_unix, 1_700_000_100);
        assert_eq!(trip.destination_stop_id, "A32");
    }

    #[test]
    fn reads_nyct_train_id_when_present() {
        let mut message = FeedMessage::decode(sample_feed_bytes().as_slice()).unwrap();
        if let Some(trip) = message.entity[0]
            .trip_update
            .as_mut()
            .map(|trip_update| &mut trip_update.trip)
        {
            trip.nyct_trip_descriptor = Some(proto::NyctTripDescriptor {
                train_id: Some("06 1234+ PEL/BBR".into()),
                is_assigned: Some(true),
                direction: None,
            });
        }
        let feed = decode_feed_message(&message).unwrap();
        assert_eq!(
            feed.trips[0].nyct_train_id.as_deref(),
            Some("06 1234+ PEL/BBR")
        );
    }

    #[test]
    fn decodes_trip_with_empty_remaining_at_terminus() {
        let message = FeedMessage {
            header: FeedHeader {
                gtfs_realtime_version: "2.0".into(),
                incrementality: None,
                timestamp: Some(1_700_000_000),
                feed_version: None,
                nyct_feed_header: None,
            },
            entity: vec![FeedEntity {
                id: "2".into(),
                is_deleted: None,
                trip_update: Some(TripUpdate {
                    trip: TripDescriptor {
                        trip_id: Some("ACE..N".into()),
                        route_id: Some("A".into()),
                        start_date: Some("20260725".into()),
                        ..Default::default()
                    },
                    stop_time_update: vec![
                        StopTimeUpdate {
                            stop_id: Some("A24".into()),
                            ..Default::default()
                        },
                        StopTimeUpdate {
                            stop_id: Some("A32".into()),
                            ..Default::default()
                        },
                    ],
                    ..Default::default()
                }),
                vehicle: None,
                alert: None,
                shape: None,
                stop: None,
                trip_modifications: None,
            }],
        };
        let feed = decode_feed_message(&message).unwrap();
        assert_eq!(feed.trips.len(), 1);
        let trip = &feed.trips[0];
        assert!(trip.remaining.is_empty());
        assert_eq!(trip.destination_stop_id, "A32");
    }

    #[test]
    fn destination_from_full_schedule_not_remaining_only() {
        let message = FeedMessage {
            header: FeedHeader {
                gtfs_realtime_version: "2.0".into(),
                incrementality: None,
                timestamp: Some(1_700_000_000),
                feed_version: None,
                nyct_feed_header: None,
            },
            entity: vec![FeedEntity {
                id: "3".into(),
                is_deleted: None,
                trip_update: Some(TripUpdate {
                    trip: TripDescriptor {
                        trip_id: Some("ACE..N".into()),
                        route_id: Some("A".into()),
                        ..Default::default()
                    },
                    stop_time_update: vec![
                        StopTimeUpdate {
                            stop_id: Some("A24".into()),
                            arrival: Some(proto::trip_update::StopTimeEvent {
                                time: Some(1_700_000_100),
                                ..Default::default()
                            }),
                            ..Default::default()
                        },
                        StopTimeUpdate {
                            stop_id: Some("A32".into()),
                            ..Default::default()
                        },
                    ],
                    ..Default::default()
                }),
                vehicle: None,
                alert: None,
                shape: None,
                stop: None,
                trip_modifications: None,
            }],
        };
        let feed = decode_feed_message(&message).unwrap();
        assert_eq!(feed.trips[0].remaining.len(), 1);
        assert_eq!(feed.trips[0].destination_stop_id, "A32");
    }
}
