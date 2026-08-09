//! Plan: alerts → watchable route set; suspended / no-service excluded.

use ingest::alerts::{trackable_route_ids, watchable_from_alerts};
use ingest::decode::proto::{
    Alert, EntitySelector, FeedEntity, FeedHeader, FeedMessage, alert::Effect,
};
use prost::Message;

#[test]
fn suspended_routes_not_watchable() {
    let bytes = alerts_fixture_suspend_a();
    let watchable = watchable_from_alerts(&bytes).expect("alerts should decode");
    assert!(!watchable.route_ids.iter().any(|route| route == "A"));
    assert!(watchable.route_ids.iter().any(|route| route == "C"));
}

#[test]
fn gs_shuttle_never_watchable() {
    let watchable = watchable_from_alerts(&alerts_fixture_suspend_a()).expect("alerts decode");
    assert!(
        !watchable.route_ids.iter().any(|route| route == "GS"),
        "GS (42 St Shuttle) must not enter watchable set or trip pipeline"
    );
    assert_eq!(watchable.route_ids.len(), trackable_route_ids().len() - 1);
}

#[test]
fn h_shuttle_never_watchable() {
    let watchable = watchable_from_alerts(&alerts_fixture_suspend_a()).expect("alerts decode");
    assert!(
        !watchable.route_ids.iter().any(|route| route == "H"),
        "H (Rockaway Park Shuttle) must not enter watchable set or trip pipeline"
    );
}

fn alerts_fixture_suspend_a() -> Vec<u8> {
    FeedMessage {
        header: FeedHeader {
            gtfs_realtime_version: "2.0".into(),
            incrementality: None,
            timestamp: Some(1_700_000_000),
            feed_version: None,
            nyct_feed_header: None,
        },
        entity: vec![FeedEntity {
            id: "alert-1".into(),
            is_deleted: None,
            trip_update: None,
            vehicle: None,
            alert: Some(Alert {
                informed_entity: vec![EntitySelector {
                    route_id: Some("A".into()),
                    ..Default::default()
                }],
                effect: Some(Effect::NoService as i32),
                ..Default::default()
            }),
            shape: None,
            stop: None,
            trip_modifications: None,
        }],
    }
    .encode_to_vec()
}
