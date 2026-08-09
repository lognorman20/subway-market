use askama::Template;
use axum::{
    http::StatusCode,
    response::{Html, IntoResponse, Response},
};

use crate::markets::{
    AgentOption, BannerKind, MarketDetailView, MarketListItem, MarketsPresentation, PnlRowView,
};
use crate::trains::{BannerKind as TrainBannerKind, TrainView, TrainsPresentation};

pub struct TemplateResponse<T>(pub T);

impl<T> IntoResponse for TemplateResponse<T>
where
    T: Template,
{
    fn into_response(self) -> Response {
        match self.0.render() {
            Ok(html) => Html(html).into_response(),
            Err(err) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("template render failed: {err}"),
            )
                .into_response(),
        }
    }
}

#[derive(Template)]
#[template(path = "home.html")]
pub struct HomeTemplate;

#[derive(Template)]
#[template(path = "trains.html")]
pub struct TrainsPageTemplate {
    pub upcoming: Vec<TrainView>,
    pub history: Vec<TrainView>,
    pub banner_class: Option<&'static str>,
    pub banner_message: Option<String>,
}

impl From<TrainsPresentation> for TrainsPageTemplate {
    fn from(presentation: TrainsPresentation) -> Self {
        let (banner_class, banner_message) = train_banner_fields(presentation.banner);
        Self {
            upcoming: presentation.upcoming,
            history: presentation.history,
            banner_class,
            banner_message,
        }
    }
}

#[derive(Template)]
#[template(path = "trains_rows.html")]
pub struct TrainsRowsTemplate {
    pub upcoming: Vec<TrainView>,
    pub history: Vec<TrainView>,
    pub banner_class: Option<&'static str>,
    pub banner_message: Option<String>,
}

impl From<TrainsPresentation> for TrainsRowsTemplate {
    fn from(presentation: TrainsPresentation) -> Self {
        let (banner_class, banner_message) = train_banner_fields(presentation.banner);
        Self {
            upcoming: presentation.upcoming,
            history: presentation.history,
            banner_class,
            banner_message,
        }
    }
}

#[derive(Template)]
#[template(path = "markets.html")]
pub struct MarketsPageTemplate {
    pub active: Option<MarketListItem>,
    pub open: Vec<MarketListItem>,
    pub resolved: Vec<MarketListItem>,
    pub banner_class: Option<&'static str>,
    pub banner_message: Option<String>,
}

impl From<MarketsPresentation> for MarketsPageTemplate {
    fn from(presentation: MarketsPresentation) -> Self {
        let (banner_class, banner_message) = market_banner_fields(presentation.banner);
        Self {
            active: presentation.active,
            open: presentation.open,
            resolved: presentation.resolved,
            banner_class,
            banner_message,
        }
    }
}

#[derive(Template)]
#[template(path = "market.html")]
pub struct MarketPageTemplate {
    pub detail: MarketDetailView,
    pub agents: Vec<AgentOption>,
    pub session_present: bool,
    pub trade_enabled: bool,
    pub agents_banner: Option<String>,
    pub trade_flash: Option<String>,
    pub trade_ok: Option<bool>,
}

#[derive(Template)]
#[template(path = "market_live.html")]
pub struct MarketLiveTemplate {
    pub detail: MarketDetailView,
}

#[derive(Template)]
#[template(path = "market_pnl.html")]
pub struct MarketPnlTemplate {
    pub rows: Vec<PnlRowView>,
    pub banner_class: Option<&'static str>,
    pub banner_message: Option<String>,
}

#[derive(Template)]
#[template(path = "trade_flash.html")]
pub struct TradeFlashTemplate {
    pub ok: bool,
    pub message: String,
}

fn train_banner_fields(
    banner: Option<(TrainBannerKind, String)>,
) -> (Option<&'static str>, Option<String>) {
    match banner {
        Some((kind, message)) => (Some(train_banner_class(kind)), Some(message)),
        None => (None, None),
    }
}

fn train_banner_class(kind: TrainBannerKind) -> &'static str {
    match kind {
        TrainBannerKind::Info => "banner banner-info",
        TrainBannerKind::Warn => "banner banner-warn",
        TrainBannerKind::Error => "banner banner-error",
    }
}

fn market_banner_fields(
    banner: Option<(BannerKind, String)>,
) -> (Option<&'static str>, Option<String>) {
    match banner {
        Some((kind, message)) => (Some(market_banner_class(kind)), Some(message)),
        None => (None, None),
    }
}

fn market_banner_class(kind: BannerKind) -> &'static str {
    match kind {
        BannerKind::Info => "banner banner-info",
        BannerKind::Warn => "banner banner-warn",
        BannerKind::Error => "banner banner-error",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::markets::{MarketDetailView, MarketsPresentation};
    use dto::MarketSummary;

    #[test]
    fn markets_page_template_renders() {
        let presentation = MarketsPresentation::from_summaries(vec![MarketSummary {
            market_id: "1".into(),
            route_id: "A".into(),
            trip_id: "trip-1".into(),
            stop_id: "stop-1".into(),
            outcome: "unresolved".into(),
            price_early_wad: crate::markets::HALF_WAD.into(),
            price_late_wad: crate::markets::HALF_WAD.into(),
        }]);
        let html = MarketsPageTemplate::from(presentation)
            .render()
            .expect("render markets");
        assert!(html.contains("Active market"));
        assert!(html.contains("50.0%"));
        assert!(!html.to_lowercase().contains("polymarket"));
    }

    #[test]
    fn market_live_template_renders_chart() {
        let detail = MarketDetailView {
            market_id: "1".into(),
            route_id: "A".into(),
            trip_id: "t".into(),
            stop_id: "s".into(),
            train_id: "—".into(),
            frozen_eta_label: "1".into(),
            outcome: "unresolved".into(),
            outcome_label: "Open".into(),
            is_open: true,
            price_early_pct: "50.0%".into(),
            price_late_pct: "50.0%".into(),
            price_early_wad: crate::markets::HALF_WAD.into(),
            price_late_wad: crate::markets::HALF_WAD.into(),
            opened_at: "2026-01-01T00:00:00Z".into(),
            chart_points: "0,20 100,20".into(),
            chart_empty: true,
            trades: Vec::new(),
        };
        let html = MarketLiveTemplate { detail }.render().expect("render live");
        assert!(html.contains("Early"));
        assert!(html.contains("Late"));
        assert!(html.contains("polyline"));
    }

    #[test]
    fn trade_flash_renders() {
        let html = TradeFlashTemplate {
            ok: true,
            message: "buy early ok".into(),
        }
        .render()
        .expect("flash");
        assert!(html.contains("buy early ok"));
    }
}
