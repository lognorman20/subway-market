//! HTTP error envelope matching `dto::ApiErrorResponse`.

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use db::QueryError;
use dto::ApiErrorResponse;
use tracing::error;

#[derive(Debug)]
pub enum ApiError {
    Unauthorized(String),
    Forbidden(String),
    BadRequest(String),
    NotFound(String),
    Conflict(String),
    InsufficientBankroll,
    Unavailable(String),
    Internal(String),
}

impl ApiError {
    pub fn code(&self) -> Option<&'static str> {
        match self {
            Self::Unauthorized(_) => Some("unauthorized"),
            Self::Forbidden(_) => Some("forbidden"),
            Self::BadRequest(_) => Some("bad_request"),
            Self::NotFound(_) => Some("not_found"),
            Self::Conflict(_) => Some("conflict"),
            Self::InsufficientBankroll => Some("insufficient_bankroll"),
            Self::Unavailable(_) => Some("unavailable"),
            Self::Internal(_) => Some("internal"),
        }
    }

    pub fn status(&self) -> StatusCode {
        match self {
            Self::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            Self::Forbidden(_) => StatusCode::FORBIDDEN,
            Self::BadRequest(_) | Self::InsufficientBankroll => StatusCode::BAD_REQUEST,
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::Conflict(_) => StatusCode::CONFLICT,
            Self::Unavailable(_) => StatusCode::SERVICE_UNAVAILABLE,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub fn message(&self) -> &str {
        match self {
            Self::Unauthorized(m)
            | Self::Forbidden(m)
            | Self::BadRequest(m)
            | Self::NotFound(m)
            | Self::Conflict(m)
            | Self::Unavailable(m)
            | Self::Internal(m) => m.as_str(),
            Self::InsufficientBankroll => "insufficient bankroll",
        }
    }
}

impl From<QueryError> for ApiError {
    fn from(err: QueryError) -> Self {
        Self::Internal(err.to_string())
    }
}

impl From<diesel::result::Error> for ApiError {
    fn from(err: diesel::result::Error) -> Self {
        Self::Internal(err.to_string())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = self.status();
        if matches!(self, Self::Internal(_)) {
            error!(error = %self.message(), "api error");
        }
        let body = ApiErrorResponse {
            error: self.message().to_string(),
            code: self.code().map(str::to_string),
        };
        (status, Json(body)).into_response()
    }
}
