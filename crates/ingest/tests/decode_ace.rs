//! Plan: decode GTFS-RT ACE (or similar) fixture into trip/vehicle events.

use ingest::decode::{decode_feed, DecodedFeed};

#[test]
fn decode_ace_feed_fixture() {
    let bytes = ace_fixture_bytes();
    let feed = decode_feed(&bytes).expect("fixture should decode");
    assert_ace_fixture(&feed);
}

fn assert_ace_fixture(feed: &DecodedFeed) {
    assert_eq!(feed.header_unix, 1_700_000_000);
    assert_eq!(feed.trips.len(), 1);
    let trip = &feed.trips[0];
    assert_eq!(trip.trip_id, "ACE..N");
    assert_eq!(trip.route_id, "A");
    assert_eq!(trip.remaining.len(), 2);
    assert_eq!(trip.remaining[0].stop_id, "A24");
    assert!(trip.remaining[0].eta_unix > 0);
    assert_eq!(
        trip.nyct_train_id.as_deref(),
        Some("06 1234+ PEL/BBR")
    );
}

fn ace_fixture_bytes() -> Vec<u8> {
    use ingest::decode::proto::{
        FeedEntity, FeedHeader, FeedMessage, NyctTripDescriptor, TripDescriptor, TripUpdate,
        trip_update::StopTimeUpdate,
    };
    use prost::Message;

    FeedMessage {
        header: FeedHeader {
            gtfs_realtime_version: "2.0".into(),
            incrementality: None,
            timestamp: Some(1_700_000_000),
            feed_version: None,
            nyct_feed_header: None,
        },
        entity: vec![FeedEntity {
            id: "ace-trip".into(),
            is_deleted: None,
            trip_update: Some(TripUpdate {
                trip: TripDescriptor {
                    trip_id: Some("ACE..N".into()),
                    route_id: Some("A".into()),
                    start_date: Some("20260725".into()),
                    nyct_trip_descriptor: Some(NyctTripDescriptor {
                        train_id: Some("06 1234+ PEL/BBR".into()),
                        is_assigned: Some(true),
                        direction: None,
                    }),
                    ..Default::default()
                },
                stop_time_update: vec![
                    StopTimeUpdate {
                        stop_id: Some("A24".into()),
                        arrival: Some(ingest::decode::proto::trip_update::StopTimeEvent {
                            time: Some(1_700_000_100),
                            ..Default::default()
                        }),
                        ..Default::default()
                    },
                    StopTimeUpdate {
                        stop_id: Some("A32".into()),
                        arrival: Some(ingest::decode::proto::trip_update::StopTimeEvent {
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
    }
    .encode_to_vec()
}
