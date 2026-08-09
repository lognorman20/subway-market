//! Alerts → watchable routes.

use std::collections::HashSet;

use chrono::Utc;
use dto::{EventMeta, WatchableRoutes};
use thiserror::Error;

use crate::decode::{proto::alert::Effect, DecodeError, decode_alerts};

/// All NYC subway `route_id` values from static GTFS (v1 universe).
pub const ALL_SUBWAY_ROUTE_IDS: &[&str] = &[
    "1", "2", "3", "4", "5", "6", "7", "A", "C", "E", "B", "D", "F", "M", "G", "J", "Z", "L", "N",
    "Q", "R", "W", "GS", "SI", "H", "S", "SIR",
];

/// GTFS routes present in feeds but excluded from the trading pipeline.
pub const UNTRACKED_ROUTE_IDS: &[&str] = &[
    "GS", // 42 St Shuttle (Times Sq ↔ Grand Central) — intentionally untracked
    "H",  // Rockaway Park Shuttle — GTFS route_id H; not a mainline letter route
];

/// Subway routes eligible for watchable/trip emit (GTFS universe minus denylist).
pub fn trackable_route_ids() -> Vec<String> {
    ALL_SUBWAY_ROUTE_IDS
        .iter()
        .filter(|route| !UNTRACKED_ROUTE_IDS.contains(route))
        .map(|route| (*route).to_string())
        .collect()
}

#[derive(Debug, Error)]
pub enum AlertsError {
    #[error("decode alerts feed: {0}")]
    Decode(#[from] DecodeError),
}

/// Build watchable route set from an alerts feed body.
pub fn watchable_from_alerts(alert_bytes: &[u8]) -> Result<WatchableRoutes, AlertsError> {
    let alerts = decode_alerts(alert_bytes)?;
    let suspended = suspended_route_ids(&alerts);
    let route_ids = trackable_route_ids()
        .into_iter()
        .filter(|route| !suspended.contains(route.as_str()))
        .collect();

    Ok(WatchableRoutes {
        meta: EventMeta::new(),
        route_ids,
        observed_at: Utc::now(),
    })
}

fn suspended_route_ids(alerts: &[crate::decode::proto::Alert]) -> HashSet<String> {
    let mut suspended = HashSet::new();
    for alert in alerts {
        if !is_suspension_effect(alert.effect()) {
            continue;
        }
        for entity in &alert.informed_entity {
            if let Some(route_id) = &entity.route_id {
                suspended.insert(route_id.clone());
            }
        }
    }
    suspended
}

fn is_suspension_effect(effect: Effect) -> bool {
    matches!(effect, Effect::NoService)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::proto::{
        Alert, EntitySelector, FeedEntity, FeedHeader, FeedMessage, alert::Effect,
    };
    use prost::Message;

    fn alerts_feed(alerts: Vec<Alert>) -> Vec<u8> {
        FeedMessage {
            header: FeedHeader {
                gtfs_realtime_version: "2.0".into(),
                incrementality: None,
                timestamp: Some(1_700_000_000),
                feed_version: None,
                nyct_feed_header: None,
            },
            entity: alerts
                .into_iter()
                .enumerate()
                .map(|(idx, alert)| FeedEntity {
                    id: idx.to_string(),
                    is_deleted: None,
                    trip_update: None,
                    vehicle: None,
                    alert: Some(alert),
                    shape: None,
                    stop: None,
                    trip_modifications: None,
                })
                .collect(),
        }
        .encode_to_vec()
    }

    #[test]
    fn no_alerts_means_all_routes_watchable() {
        let watchable = watchable_from_alerts(&alerts_feed(vec![])).unwrap();
        assert_eq!(watchable.route_ids.len(), trackable_route_ids().len());
        assert!(watchable.route_ids.iter().any(|route| route == "A"));
    }

    #[test]
    fn gs_always_excluded_from_watchable() {
        let watchable = watchable_from_alerts(&alerts_feed(vec![])).unwrap();
        assert!(
            !watchable.route_ids.iter().any(|route| route == "GS"),
            "42 St Shuttle (GS) must never be watchable"
        );
        assert!(
            !watchable.route_ids.iter().any(|route| route == "H"),
            "Rockaway Park Shuttle (H) must never be watchable"
        );
        // Other shuttles stay tracked (Franklin Av `S`, SI `SI`/`SIR`).
        assert!(watchable.route_ids.iter().any(|route| route == "S"));
    }

    #[test]
    fn suspended_route_excluded() {
        let watchable = watchable_from_alerts(&alerts_feed(vec![Alert {
            informed_entity: vec![EntitySelector {
                route_id: Some("A".into()),
                ..Default::default()
            }],
            effect: Some(Effect::NoService as i32),
            ..Default::default()
        }]))
        .unwrap();
        assert!(!watchable.route_ids.iter().any(|route| route == "A"));
        assert!(watchable.route_ids.iter().any(|route| route == "C"));
    }

    #[test]
    fn alert_without_route_id_is_ignored() {
        let watchable = watchable_from_alerts(&alerts_feed(vec![Alert {
            informed_entity: vec![EntitySelector {
                agency_id: Some("MTA".into()),
                ..Default::default()
            }],
            effect: Some(Effect::NoService as i32),
            ..Default::default()
        }]))
        .unwrap();
        assert_eq!(watchable.route_ids.len(), trackable_route_ids().len());
    }
}
