use askama::Template;
use axum::{
    http::StatusCode,
    response::{Html, IntoResponse, Response},
};

use crate::trains::{BannerKind, TrainView, TrainsPresentation};

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
        let (banner_class, banner_message) = banner_fields(presentation.banner);
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
        let (banner_class, banner_message) = banner_fields(presentation.banner);
        Self {
            upcoming: presentation.upcoming,
            history: presentation.history,
            banner_class,
            banner_message,
        }
    }
}

fn banner_fields(banner: Option<(BannerKind, String)>) -> (Option<&'static str>, Option<String>) {
    match banner {
        Some((kind, message)) => (Some(banner_class(kind)), Some(message)),
        None => (None, None),
    }
}

fn banner_class(kind: BannerKind) -> &'static str {
    match kind {
        BannerKind::Info => "banner banner-info",
        BannerKind::Warn => "banner banner-warn",
        BannerKind::Error => "banner banner-error",
    }
}
