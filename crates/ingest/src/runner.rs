//! Poll loop orchestration and bus emit.

use std::collections::{HashMap, HashSet};

use dto::{
    BusEvent, EventMeta, TripSnapshot, VehicleUpdate, INGEST_TRIP, INGEST_VEHICLE,
    INGEST_WATCHABLE,
};
use tracing::{debug, error, info, warn};

use crate::alerts::{trackable_route_ids, watchable_from_alerts};
use crate::decode::{decode_feed, DecodedTrip, DecodedVehicle};
use crate::feeds::{self, alerts_url};
use crate::poll::{FeedPoller, PollResult, POLL_INTERVAL};
use crate::stops::format_stop;

/// Last-seen trip state for vanished-trip detection per feed URL.
#[derive(Debug, Clone)]
struct TrackedTrip {
    route_id: String,
    service_date: Option<String>,
    destination_stop_id: String,
    nyct_train_id: Option<String>,
}

/// Shared ingest runtime state.
pub struct IngestRunner {
    alerts_poller: FeedPoller,
    trip_pollers: Vec<(String, FeedPoller)>,
    watchable: HashSet<String>,
    last_watchable_signature: Option<Vec<String>>,
    last_trips_by_feed: HashMap<String, HashMap<String, TrackedTrip>>,
    #[cfg(test)]
    test_alerts_url: Option<String>,
}

impl IngestRunner {
    pub fn new() -> Self {
        let client = reqwest::Client::new();
        let trip_pollers = feeds::TRIP_FEED_PATHS
            .iter()
            .map(|path| (feeds::trip_feed_url(path), FeedPoller::new(client.clone())))
            .collect();
        Self {
            alerts_poller: FeedPoller::new(client),
            trip_pollers,
            watchable: all_watchable_set(),
            last_watchable_signature: None,
            last_trips_by_feed: HashMap::new(),
            #[cfg(test)]
            test_alerts_url: None,
        }
    }

    /// One poll cycle: alerts + trip feeds.
    pub async fn poll_once(&mut self, transport: &transport::TransportClient) -> anyhow::Result<()> {
        self.poll_alerts(transport).await?;
        let watchable = self.watchable.clone();
        let mut changed_feeds = Vec::new();
        for (url, poller) in &mut self.trip_pollers {
            match poller.poll(url).await {
                Ok(Some(result)) => changed_feeds.push((url.clone(), result)),
                Ok(None) => {}
                Err(err) => {
                    warn!(%url, error = %err, "trip feed poll failed");
                    return Err(err.into());
                }
            }
        }
        let mut trips_emitted = 0usize;
        let mut vehicles_emitted = 0usize;
        for (url, result) in changed_feeds {
            let (trips, vehicles) = self.emit_trip_feed(transport, &url, &result, &watchable).await?;
            trips_emitted += trips;
            vehicles_emitted += vehicles;
        }
        if trips_emitted > 0 || vehicles_emitted > 0 {
            info!(
                message = format!(
                    "poll cycle: emitted {trips_emitted} trips, {vehicles_emitted} vehicles, watchable={}",
                    watchable.len()
                ),
                trips = trips_emitted,
                vehicles = vehicles_emitted,
                watchable = watchable.len(),
            );
        }
        Ok(())
    }

    /// Run the 15s poll loop until the process is stopped.
    pub async fn run(&mut self, transport: transport::TransportClient) -> anyhow::Result<()> {
        info!(
            message = format!("polling MTA feeds (interval {}s)", POLL_INTERVAL.as_secs()),
            interval_secs = POLL_INTERVAL.as_secs(),
        );
        loop {
            if let Err(err) = self.poll_once(&transport).await {
                if is_nats_failure(&err) {
                    error!(error = %err, "poll cycle failed (NATS emit)");
                } else {
                    warn!(error = %err, "poll cycle failed (transient)");
                }
            }
            tokio::time::sleep(POLL_INTERVAL).await;
        }
    }

    async fn emit_transport(
        &self,
        transport: &transport::TransportClient,
        subject: &str,
        event: &BusEvent,
    ) -> anyhow::Result<()> {
        transport.emit(subject, event).await.map_err(|err| {
            error!(
                error = %err,
                subject,
                event_id = %event.event_id().as_ref(),
                event_type = event.event_type_name(),
                "NATS emit failed"
            );
            err
        })?;
        Ok(())
    }

    async fn poll_alerts(&mut self, transport: &transport::TransportClient) -> anyhow::Result<()> {
        let url = self.alerts_poll_url();
        let result = match self.alerts_poller.poll(&url).await {
            Ok(result) => result,
            Err(err) => {
                warn!(%url, error = %err, "alerts feed poll failed");
                return Ok(());
            }
        };
        let Some(result) = result else {
            return Ok(());
        };
        let watchable = watchable_from_alerts(&result.bytes)?;
        self.watchable = watchable.route_ids.iter().cloned().collect();
        let signature = watchable.route_ids.clone();
        if self.last_watchable_signature.as_ref() != Some(&signature) {
            let suspended = suspended_routes(&watchable.route_ids);
            let suspended_label = format_suspended_list(&suspended);
            info!(
                message = format!(
                    "watchable routes: {} lines active (suspended: {suspended_label})",
                    watchable.route_ids.len()
                ),
                active = watchable.route_ids.len(),
                suspended = suspended.join(","),
                subject = INGEST_WATCHABLE,
            );
            transport.emit(INGEST_WATCHABLE, &BusEvent::Watchable(watchable))
                .await
                .map_err(|err| {
                    error!(
                        error = %err,
                        subject = INGEST_WATCHABLE,
                        "NATS emit failed for watchable routes"
                    );
                    err
                })?;
            self.last_watchable_signature = Some(signature);
        }
        Ok(())
    }

    async fn emit_trip_feed(
        &mut self,
        transport: &transport::TransportClient,
        feed_url: &str,
        result: &PollResult,
        watchable: &HashSet<String>,
    ) -> anyhow::Result<(usize, usize)> {
        let feed = decode_feed(&result.bytes)?;
        debug!(
            message = "decoded trip feed",
            header_unix = feed.header_unix,
            trips = feed.trips.len(),
            vehicles = feed.vehicles.len(),
        );
        let mut trips_emitted = 0usize;
        let mut vehicles_emitted = 0usize;
        let mut current_trips = HashMap::new();

        for trip in feed.trips {
            if !watchable.contains(&trip.route_id) {
                debug!(
                    message = "dropping non-watchable trip",
                    route_id = %trip.route_id,
                    trip_id = %trip.trip_id,
                );
                continue;
            }
            track_trip(&mut current_trips, &trip);
            let event = trip_snapshot_from_decoded(&trip, feed.header_unix);
            log_trip_emit(&trip, &event, false);
            self.emit_transport(transport, INGEST_TRIP, &BusEvent::Trip(event)).await?;
            trips_emitted += 1;
        }

        if let Some(previous) = self.last_trips_by_feed.get(feed_url) {
            for (trip_id, tracked) in previous {
                if current_trips.contains_key(trip_id) {
                    continue;
                }
                if !watchable.contains(&tracked.route_id) {
                    continue;
                }
                let vanished = DecodedTrip {
                    trip_id: trip_id.clone(),
                    route_id: tracked.route_id.clone(),
                    service_date: tracked.service_date.clone(),
                    remaining: Vec::new(),
                    destination_stop_id: tracked.destination_stop_id.clone(),
                    nyct_train_id: tracked.nyct_train_id.clone(),
                };
                let event = trip_snapshot_from_decoded(&vanished, feed.header_unix);
                log_trip_emit(&vanished, &event, true);
                self.emit_transport(transport, INGEST_TRIP, &BusEvent::Trip(event)).await?;
                trips_emitted += 1;
            }
        }

        self.last_trips_by_feed
            .insert(feed_url.to_string(), current_trips);

        for vehicle in feed.vehicles {
            if !watchable.contains(&vehicle.route_id) {
                debug!(
                    message = "dropping non-watchable vehicle",
                    route_id = %vehicle.route_id,
                    trip_id = %vehicle.trip_id,
                );
                continue;
            }
            let event = vehicle_update_from_decoded(&vehicle, feed.header_unix);
            debug!(
                message = "emitted vehicle update",
                event_id = %event.meta.event_id.0,
                trip_id = %vehicle.trip_id,
                route_id = %vehicle.route_id,
                subject = INGEST_VEHICLE,
            );
            self.emit_transport(transport, INGEST_VEHICLE, &BusEvent::Vehicle(event))
                .await?;
            vehicles_emitted += 1;
        }
        Ok((trips_emitted, vehicles_emitted))
    }

    fn alerts_poll_url(&self) -> String {
        #[cfg(test)]
        if let Some(url) = &self.test_alerts_url {
            return url.clone();
        }
        alerts_url()
    }
}

impl Default for IngestRunner {
    fn default() -> Self {
        Self::new()
    }
}

fn track_trip(current: &mut HashMap<String, TrackedTrip>, trip: &DecodedTrip) {
    current.insert(
        trip.trip_id.clone(),
        TrackedTrip {
            route_id: trip.route_id.clone(),
            service_date: trip.service_date.clone(),
            destination_stop_id: trip.destination_stop_id.clone(),
            nyct_train_id: trip.nyct_train_id.clone(),
        },
    );
}

fn log_trip_emit(trip: &DecodedTrip, event: &TripSnapshot, vanished: bool) {
    let train = trip
        .nyct_train_id
        .as_deref()
        .map(|id| format!(" ({id})"))
        .unwrap_or_default();
    let verb = if vanished { "vanished trip" } else { "trip snapshot" };
    debug!(
        message = format!(
            "emitted {verb} for {} train {}{} — {} stops remaining, destination {}",
            trip.route_id,
            trip.trip_id,
            train,
            trip.remaining.len(),
            format_stop(&trip.destination_stop_id),
        ),
        event_id = %event.meta.event_id.0,
        trip_id = %trip.trip_id,
        route_id = %trip.route_id,
        subject = INGEST_TRIP,
        nyct_train_id = trip.nyct_train_id.as_deref(),
        vanished,
    );
}

fn trip_snapshot_from_decoded(trip: &DecodedTrip, feed_header_unix: i64) -> TripSnapshot {
    TripSnapshot {
        meta: EventMeta::new(),
        trip_id: trip.trip_id.clone(),
        route_id: trip.route_id.clone(),
        service_date: trip.service_date.clone(),
        remaining: trip.remaining.clone(),
        destination_stop_id: trip.destination_stop_id.clone(),
        feed_header_unix,
        nyct_train_id: trip.nyct_train_id.clone(),
    }
}

fn vehicle_update_from_decoded(vehicle: &DecodedVehicle, feed_header_unix: i64) -> VehicleUpdate {
    VehicleUpdate {
        meta: EventMeta::new(),
        trip_id: vehicle.trip_id.clone(),
        route_id: vehicle.route_id.clone(),
        stop_id: vehicle.stop_id.clone(),
        vehicle_unix: vehicle.vehicle_unix,
        feed_header_unix,
    }
}

fn all_watchable_set() -> HashSet<String> {
    trackable_route_ids().into_iter().collect()
}

fn suspended_routes(active: &[String]) -> Vec<String> {
    let active: HashSet<&str> = active.iter().map(String::as_str).collect();
    trackable_route_ids()
        .into_iter()
        .filter(|route| !active.contains(route.as_str()))
        .collect()
}

fn is_nats_failure(err: &anyhow::Error) -> bool {
    err.to_string().contains("nats:")
        || err.to_string().contains("serialize:")
        || err.downcast_ref::<transport::TransportError>().is_some()
}

fn format_suspended_list(suspended: &[String]) -> String {
    if suspended.is_empty() {
        "none".to_string()
    } else {
        suspended.join(", ")
    }
}

#[cfg(test)]
impl IngestRunner {
    fn with_test_urls(alerts_url: String, trip_urls: Vec<String>) -> Self {
        let client = reqwest::Client::new();
        let trip_pollers = trip_urls
            .into_iter()
            .map(|url| (url.clone(), FeedPoller::new(client.clone())))
            .collect();
        Self {
            alerts_poller: FeedPoller::new(client),
            trip_pollers,
            watchable: all_watchable_set(),
            last_watchable_signature: None,
            last_trips_by_feed: HashMap::new(),
            test_alerts_url: Some(alerts_url),
        }
    }

    async fn prime_trip_feed(&mut self, index: usize) {
        let (url, poller) = &mut self.trip_pollers[index];
        let url = url.clone();
        poller.poll(&url).await.expect("prime trip feed");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prost::Message;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use crate::decode::proto::{FeedEntity, FeedHeader, FeedMessage};

    fn trip_feed_bytes(timestamp: u64) -> Vec<u8> {
        FeedMessage {
            header: FeedHeader {
                gtfs_realtime_version: "2.0".into(),
                incrementality: None,
                timestamp: Some(timestamp),
                feed_version: None,
                nyct_feed_header: None,
            },
            entity: vec![FeedEntity {
                id: "1".into(),
                is_deleted: None,
                trip_update: None,
                vehicle: None,
                alert: None,
                shape: None,
                stop: None,
                trip_modifications: None,
            }],
        }
        .encode_to_vec()
    }

    async fn test_transport() -> Option<transport::TransportClient> {
        transport::connect("nats://127.0.0.1:4223").await.ok()
    }

    #[tokio::test]
    async fn alerts_http_failure_retains_watchable() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/alerts"))
            .respond_with(ResponseTemplate::new(503))
            .mount(&server)
            .await;

        let alerts_url = format!("{}/alerts", server.uri());
        let mut runner = IngestRunner::with_test_urls(alerts_url, vec![]);
        let prior_watchable = runner.watchable.clone();

        let Some(transport) = test_transport().await else {
            return;
        };

        runner.poll_alerts(&transport).await.expect("alerts failure is non-fatal");
        assert_eq!(runner.watchable, prior_watchable);
    }

    #[tokio::test]
    async fn alerts_failure_does_not_abort_poll_cycle() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/alerts"))
            .respond_with(ResponseTemplate::new(503))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/trip"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(trip_feed_bytes(42)))
            .expect(2)
            .mount(&server)
            .await;

        let alerts_url = format!("{}/alerts", server.uri());
        let trip_url = format!("{}/trip", server.uri());
        let mut runner = IngestRunner::with_test_urls(alerts_url, vec![trip_url]);
        let prior_watchable = runner.watchable.clone();
        runner.prime_trip_feed(0).await;

        let Some(transport) = test_transport().await else {
            return;
        };

        runner
            .poll_once(&transport)
            .await
            .expect("poll cycle should continue when alerts fail");
        assert_eq!(runner.watchable, prior_watchable);
    }

    #[tokio::test]
    async fn trip_feed_failure_still_aborts_poll_cycle() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/alerts"))
            .respond_with(ResponseTemplate::new(503))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/trip"))
            .respond_with(ResponseTemplate::new(503))
            .mount(&server)
            .await;

        let alerts_url = format!("{}/alerts", server.uri());
        let trip_url = format!("{}/trip", server.uri());
        let mut runner = IngestRunner::with_test_urls(alerts_url, vec![trip_url]);

        let Some(transport) = test_transport().await else {
            return;
        };

        assert!(
            runner.poll_once(&transport).await.is_err(),
            "trip feed failure should still abort the poll cycle"
        );
    }
}
