//! Human session cookie helpers (frontend ↔ browser only).
//!
//! Token is a backend `sms_*` session — never an agent API key.

use axum::http::{header, HeaderMap, HeaderValue};

pub const SESSION_COOKIE: &str = "subway_session";

/// Max-Age for session cookie (24h). Backend may expire sooner.
const COOKIE_MAX_AGE_SECS: u64 = 86_400;

pub fn session_token_from_headers(headers: &HeaderMap) -> Option<String> {
    let cookie = headers.get(header::COOKIE)?.to_str().ok()?;
    for part in cookie.split(';') {
        let part = part.trim();
        let Some(value) = part.strip_prefix(&format!("{SESSION_COOKIE}=")) else {
            continue;
        };
        let value = value.trim();
        if !value.is_empty() {
            return Some(value.to_string());
        }
    }
    None
}

pub fn set_session_cookie_header(token: &str) -> HeaderValue {
    let value = format!(
        "{SESSION_COOKIE}={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={COOKIE_MAX_AGE_SECS}"
    );
    HeaderValue::from_str(&value).expect("session cookie header")
}

pub fn clear_session_cookie_header() -> HeaderValue {
    let value = format!("{SESSION_COOKIE}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0");
    HeaderValue::from_str(&value).expect("clear session cookie header")
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderMap;

    #[test]
    fn parses_session_cookie() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            HeaderValue::from_static("foo=bar; subway_session=sms_abc; other=1"),
        );
        assert_eq!(
            session_token_from_headers(&headers).as_deref(),
            Some("sms_abc")
        );
    }

    #[test]
    fn missing_cookie_returns_none() {
        let mut headers = HeaderMap::new();
        headers.insert(header::COOKIE, HeaderValue::from_static("foo=bar"));
        assert!(session_token_from_headers(&headers).is_none());
    }
}
