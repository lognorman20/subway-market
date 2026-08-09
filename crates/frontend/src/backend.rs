use serde::Deserialize;
use serde_json::json;

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

#[derive(Debug, Clone)]
pub enum BackendFetch<T> {
    Ok(T),
    Empty,
    Unreachable(String),
    HttpError(u16, String),
    ParseError(String),
}

#[derive(Debug, Clone)]
pub struct TradeResult {
    pub ok: bool,
    pub message: String,
}

impl AppState {
    pub fn new(backend_url: String) -> Self {
        Self {
            backend_url,
            http: reqwest::Client::new(),
        }
    }

    fn url(&self, path: &str) -> String {
        format!(
            "{}{}",
            self.backend_url.trim_end_matches('/'),
            path
        )
    }

    pub async fn fetch_trains(&self) -> TrainsFetch {
        let url = self.url("/trains?limit=200");

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

    pub async fn fetch_markets(&self) -> BackendFetch<dto::MarketsListResponse> {
        self.get_json("/markets").await
    }

    pub async fn fetch_market(&self, market_id: &str) -> BackendFetch<dto::MarketDetailResponse> {
        self.get_json(&format!("/markets/{market_id}")).await
    }

    pub async fn fetch_market_trades(
        &self,
        market_id: &str,
        limit: i64,
    ) -> BackendFetch<dto::MarketTradesResponse> {
        self.get_json(&format!("/markets/{market_id}/trades?limit={limit}"))
            .await
    }

    pub async fn auth_challenge(
        &self,
        address: &str,
    ) -> BackendFetch<dto::AuthChallengeResponse> {
        self.post_json("/auth/challenge", &json!({ "address": address }), None)
            .await
    }

    pub async fn auth_verify(
        &self,
        challenge_id: &str,
        address: &str,
        signature: &str,
    ) -> BackendFetch<dto::AuthVerifyResponse> {
        self.post_json(
            "/auth/verify",
            &json!({
                "challenge_id": challenge_id,
                "address": address,
                "signature": signature,
            }),
            None,
        )
        .await
    }

    pub async fn list_agents(
        &self,
        session: &str,
    ) -> BackendFetch<dto::AgentsListResponse> {
        self.get_json_auth("/agents", session).await
    }

    pub async fn create_agent(
        &self,
        session: &str,
        display_name: Option<&str>,
    ) -> BackendFetch<dto::CreateAgentResponse> {
        let body = match display_name {
            Some(name) if !name.is_empty() => json!({ "display_name": name }),
            _ => json!({}),
        };
        self.post_json("/agents", &body, Some(session)).await
    }

    pub async fn list_pnl(&self, session: &str) -> BackendFetch<dto::AgentPnlResponse> {
        self.get_json_auth("/pnl", session).await
    }

    pub async fn buy(
        &self,
        session: &str,
        market_id: &str,
        side: &str,
        shares: &str,
        agent_id: &str,
    ) -> TradeResult {
        self.trade(session, "/buy", market_id, side, shares, agent_id)
            .await
    }

    pub async fn sell(
        &self,
        session: &str,
        market_id: &str,
        side: &str,
        shares: &str,
        agent_id: &str,
    ) -> TradeResult {
        self.trade(session, "/sell", market_id, side, shares, agent_id)
            .await
    }

    async fn trade(
        &self,
        session: &str,
        path: &str,
        market_id: &str,
        side: &str,
        shares: &str,
        agent_id: &str,
    ) -> TradeResult {
        let body = json!({
            "market_id": market_id,
            "side": side,
            "shares": shares,
            "agent_id": agent_id,
        });
        match self.post_json::<dto::TradeResponse>(path, &body, Some(session)).await {
            BackendFetch::Ok(resp) => TradeResult {
                ok: true,
                message: format!(
                    "{} {} · {} shares · trade {}",
                    resp.trade.action.as_str(),
                    resp.trade.side.as_str(),
                    resp.trade.shares,
                    resp.trade.trade_id.0
                ),
            },
            BackendFetch::HttpError(status, msg) => TradeResult {
                ok: false,
                message: format!("HTTP {status}: {msg}"),
            },
            BackendFetch::Unreachable(err) => TradeResult {
                ok: false,
                message: format!("backend unreachable: {err}"),
            },
            BackendFetch::ParseError(err) => TradeResult {
                ok: false,
                message: format!("parse error: {err}"),
            },
            BackendFetch::Empty => TradeResult {
                ok: false,
                message: "empty response".into(),
            },
        }
    }

    async fn get_json<T: serde::de::DeserializeOwned>(&self, path: &str) -> BackendFetch<T> {
        let url = self.url(path);
        let response = match self.http.get(&url).send().await {
            Ok(resp) => resp,
            Err(err) => return BackendFetch::Unreachable(err.to_string()),
        };
        self.decode_response(response).await
    }

    async fn get_json_auth<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        session: &str,
    ) -> BackendFetch<T> {
        let url = self.url(path);
        let response = match self
            .http
            .get(&url)
            .header("Authorization", format!("Bearer {session}"))
            .send()
            .await
        {
            Ok(resp) => resp,
            Err(err) => return BackendFetch::Unreachable(err.to_string()),
        };
        self.decode_response(response).await
    }

    async fn post_json<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        body: &serde_json::Value,
        session: Option<&str>,
    ) -> BackendFetch<T> {
        let url = self.url(path);
        let mut req = self.http.post(&url).json(body);
        if let Some(token) = session {
            req = req.header("Authorization", format!("Bearer {token}"));
        }
        let response = match req.send().await {
            Ok(resp) => resp,
            Err(err) => return BackendFetch::Unreachable(err.to_string()),
        };
        self.decode_response(response).await
    }

    async fn decode_response<T: serde::de::DeserializeOwned>(
        &self,
        response: reqwest::Response,
    ) -> BackendFetch<T> {
        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            let msg = serde_json::from_str::<dto::ApiErrorResponse>(&body)
                .map(|e| {
                    e.code
                        .map(|c| format!("{c}: {}", e.error))
                        .unwrap_or(e.error)
                })
                .unwrap_or(body);
            return BackendFetch::HttpError(status.as_u16(), msg);
        }
        match response.json::<T>().await {
            Ok(body) => BackendFetch::Ok(body),
            Err(err) => BackendFetch::ParseError(err.to_string()),
        }
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
