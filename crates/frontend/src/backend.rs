use serde::Deserialize;

use crate::trains::TrainView;

#[derive(Debug, Clone)]
pub struct AppState {
    pub backend_url: String,
    pub http: reqwest::Client,
}

#[derive(Debug, Deserialize)]
pub struct TrainsListResponse {
    #[serde(default)]
    pub trains: Vec<TrainRow>,
}

#[derive(Debug, Deserialize)]
pub struct TrainRow {
    pub dedupe_key: String,
    pub phase: String,
    pub route_id: String,
    pub trip_id: String,
    pub nyct_train_id: Option<String>,
    pub first_seen_at: Option<String>,
    pub start_stop_id: Option<String>,
    pub target_stop_id: Option<String>,
    pub frozen_at: Option<String>,
    pub expected_arrival_unix: Option<i64>,
    pub remaining_stops: Option<i32>,
    pub arrival_unix: Option<i64>,
    pub diff_seconds: Option<i64>,
    pub actual_source: Option<String>,
    pub action: Option<String>,
    pub side: Option<String>,
    pub reason: Option<String>,
    pub diff_id: Option<String>,
    pub freeze_event_id: Option<String>,
    pub diff_event_id: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone)]
pub enum TrainsFetch {
    Ok(Vec<TrainView>),
    Empty,
    Unreachable(String),
    HttpError(u16),
    ParseError(String),
}

impl AppState {
    pub fn new(backend_url: String) -> Self {
        Self {
            backend_url,
            http: reqwest::Client::new(),
        }
    }

    pub async fn fetch_trains(&self) -> TrainsFetch {
        let url = format!(
            "{}/trains?limit=200",
            self.backend_url.trim_end_matches('/')
        );

        let response = match self.http.get(&url).send().await {
            Ok(resp) => resp,
            Err(err) => return TrainsFetch::Unreachable(err.to_string()),
        };

        if !response.status().is_success() {
            return TrainsFetch::HttpError(response.status().as_u16());
        }

        let body = match response.json::<TrainsListResponse>().await {
            Ok(body) => body,
            Err(err) => return TrainsFetch::ParseError(err.to_string()),
        };

        if body.trains.is_empty() {
            return TrainsFetch::Empty;
        }

        TrainsFetch::Ok(body.trains.into_iter().map(TrainView::from).collect())
    }
}

impl From<TrainRow> for TrainView {
    fn from(row: TrainRow) -> Self {
        let train_label = row
            .nyct_train_id
            .clone()
            .filter(|id| !id.is_empty())
            .unwrap_or_else(|| row.trip_id.clone());

        let start = opt_str(row.start_stop_id.as_deref());
        let target = opt_str(row.target_stop_id.as_deref());
        let journey_label = match (row.start_stop_id.as_deref(), row.target_stop_id.as_deref()) {
            (Some(s), Some(t)) if !s.is_empty() && !t.is_empty() => format!("{s} → {t}"),
            (_, Some(t)) if !t.is_empty() => t.to_string(),
            (Some(s), _) if !s.is_empty() => s.to_string(),
            _ => "—".into(),
        };

        let remaining_stops = row.remaining_stops;
        let remaining_label = match remaining_stops {
            Some(n) => format!("{n}"),
            None => "—".into(),
        };

        Self {
            dedupe_key: row.dedupe_key,
            phase: row.phase,
            route_id: row.route_id,
            trip_id: row.trip_id,
            train_label,
            remaining_stops,
            remaining_label,
            diff_label: format_diff(row.diff_seconds),
            updated_at: row.updated_at.unwrap_or_else(|| "—".into()),
            target_stop_id: target,
            start_stop_id: start,
            journey_label,
            expected_eta_label: format_unix(row.expected_arrival_unix),
            frozen_at_label: opt_str(row.frozen_at.as_deref()),
            arrival_label: format_unix(row.arrival_unix),
            first_seen_label: opt_str(row.first_seen_at.as_deref()),
            actual_source: opt_str(row.actual_source.as_deref()),
            diff_id: opt_str(row.diff_id.as_deref()),
            freeze_event_id: opt_str(row.freeze_event_id.as_deref()),
            diff_event_id: opt_str(row.diff_event_id.as_deref()),
            action: opt_str(row.action.as_deref()),
            side: opt_str(row.side.as_deref()),
            reason: opt_str(row.reason.as_deref()),
        }
    }
}

fn opt_str(value: Option<&str>) -> String {
    value
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("—")
        .to_string()
}

fn format_unix(unix: Option<i64>) -> String {
    match unix {
        Some(ts) => ts.to_string(),
        None => "—".into(),
    }
}

fn format_diff(seconds: Option<i64>) -> String {
    match seconds {
        Some(0) => "on time".into(),
        Some(s) if s > 0 => format!("+{s}s late"),
        Some(s) => format!("{s}s early"),
        None => "—".into(),
    }
}
