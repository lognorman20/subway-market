//! HTTP pollers.

use std::time::Duration;

use prost::Message;
use reqwest::StatusCode;
use thiserror::Error;
use tracing::{debug, warn};

use crate::decode::proto::FeedMessage;

pub const POLL_INTERVAL: Duration = Duration::from_secs(15);
const MAX_RETRIES: u32 = 3;
const RETRY_BACKOFF: Duration = Duration::from_millis(250);

#[derive(Debug, Error)]
pub enum PollError {
    #[error("http request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("http status {0}")]
    Status(StatusCode),
    #[error("feed missing header timestamp")]
    MissingTimestamp,
    #[error("protobuf decode failed: {0}")]
    Protobuf(#[from] prost::DecodeError),
}

/// Result of a successful poll when the feed timestamp changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PollResult {
    pub bytes: Vec<u8>,
    pub header_unix: u64,
}

/// Tracks last-seen header timestamp per feed URL.
#[derive(Debug, Clone)]
pub struct FeedPoller {
    client: reqwest::Client,
    last_timestamp: Option<u64>,
}

impl FeedPoller {
    pub fn new(client: reqwest::Client) -> Self {
        Self {
            client,
            last_timestamp: None,
        }
    }

    pub fn last_timestamp(&self) -> Option<u64> {
        self.last_timestamp
    }

    /// GET `url`, retry 5xx, skip when header timestamp unchanged.
    pub async fn poll(&mut self, url: &str) -> Result<Option<PollResult>, PollError> {
        let bytes = fetch_with_retry(&self.client, url).await?;
        let header_unix = header_timestamp(&bytes)?;
        if self.last_timestamp == Some(header_unix) {
            debug!(%url, header_unix, "feed unchanged");
            return Ok(None);
        }
        self.last_timestamp = Some(header_unix);
        Ok(Some(PollResult { bytes, header_unix }))
    }
}

async fn fetch_with_retry(client: &reqwest::Client, url: &str) -> Result<Vec<u8>, PollError> {
    let mut attempt = 0;
    loop {
        attempt += 1;
        let response = client.get(url).send().await.map_err(|err| {
            warn!(%url, attempt, error = %err, "feed fetch request failed");
            err
        })?;
        let status = response.status();
        if status.is_success() {
            return Ok(response.bytes().await?.to_vec());
        }
        if status.is_server_error() && attempt < MAX_RETRIES {
            warn!(%url, %status, attempt, "retrying feed fetch");
            tokio::time::sleep(RETRY_BACKOFF).await;
            continue;
        }
        warn!(%url, %status, attempt, "feed fetch failed");
        return Err(PollError::Status(status));
    }
}

fn header_timestamp(bytes: &[u8]) -> Result<u64, PollError> {
    let message = FeedMessage::decode(bytes)?;
    message
        .header
        .timestamp
        .ok_or(PollError::MissingTimestamp)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::proto::{FeedEntity, FeedHeader, FeedMessage};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn feed_bytes(timestamp: u64) -> Vec<u8> {
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

    #[tokio::test]
    async fn successful_poll_returns_changed_feed() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/feed"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(feed_bytes(100)))
            .mount(&server)
            .await;

        let client = reqwest::Client::new();
        let mut poller = FeedPoller::new(client);
        let url = format!("{}/feed", server.uri());
        let first = poller.poll(&url).await.unwrap();
        assert!(first.is_some());
        assert_eq!(first.unwrap().header_unix, 100);
    }

    #[tokio::test]
    async fn same_timestamp_skips_emit() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/feed"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(feed_bytes(200)))
            .expect(2)
            .mount(&server)
            .await;

        let client = reqwest::Client::new();
        let mut poller = FeedPoller::new(client);
        let url = format!("{}/feed", server.uri());
        assert!(poller.poll(&url).await.unwrap().is_some());
        assert!(poller.poll(&url).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn retries_server_errors() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/feed"))
            .respond_with(ResponseTemplate::new(503))
            .up_to_n_times(2)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/feed"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(feed_bytes(300)))
            .mount(&server)
            .await;

        let client = reqwest::Client::new();
        let mut poller = FeedPoller::new(client);
        let url = format!("{}/feed", server.uri());
        let result = poller.poll(&url).await.unwrap();
        assert_eq!(result.unwrap().header_unix, 300);
    }
}
