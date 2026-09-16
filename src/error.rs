use std::fmt;
use std::time::Duration;

use http::HeaderMap;
use serde_json::Value;

use crate::constants::MAX_ERROR_BODY_LENGTH;

/// A client, transport, or API failure.
#[derive(Debug)]
pub enum Error {
    Sdk(String),
    Connection { message: String },
    Timeout { timeout: Duration },
    Api(Box<ApiError>),
}

impl Error {
    pub(crate) fn sdk(message: impl Into<String>) -> Self {
        Self::Sdk(message.into())
    }

    pub fn api(&self) -> Option<&ApiError> {
        match self {
            Self::Api(error) => Some(error),
            _ => None,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Sdk(message) => f.write_str(message),
            Self::Connection { message } => write!(f, "Connection error: {message}"),
            Self::Timeout { timeout } => {
                write!(
                    f,
                    "Request timed out (timeout={:.1}).",
                    timeout.as_secs_f64()
                )
            }
            Self::Api(error) => fmt::Display::fmt(error, f),
        }
    }
}

impl std::error::Error for Error {}

/// An unsuccessful HTTP response, or a successful response that failed to decode.
#[derive(Debug, Clone)]
pub struct ApiError {
    pub status: u16,
    pub kind: ApiErrorKind,
    pub body: Option<ErrorBody>,
    pub headers: HeaderMap,
    pub endpoint: Option<String>,
    pub field_path: Option<String>,
    pub retry_after: Option<Duration>,
    message: String,
}

impl ApiError {
    pub fn request_id(&self) -> Option<&str> {
        header_str(&self.headers, crate::constants::REQUEST_ID_HEADER)
    }

    pub fn retry_after_ms(&self) -> Option<f64> {
        self.retry_after.map(|delay| delay.as_secs_f64() * 1000.0)
    }
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut message = if self.message.is_empty() {
            self.status.to_string()
        } else {
            format!("{} {}", self.status, self.message)
        };
        if let Some(endpoint) = &self.endpoint {
            message = format!("{endpoint}: {message}");
        }
        if let Some(request_id) = self.request_id() {
            message.push_str(" (request_id=");
            message.push_str(request_id);
            message.push(')');
        }
        f.write_str(&message)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApiErrorKind {
    BadRequest,
    Authentication,
    PermissionDenied,
    NotFound,
    UnprocessableEntity,
    RateLimited,
    Internal,
    ResponseValidation,
    Other,
}

impl ApiErrorKind {
    pub fn from_status(status: u16) -> Self {
        match status {
            400 => Self::BadRequest,
            401 => Self::Authentication,
            403 => Self::PermissionDenied,
            404 => Self::NotFound,
            422 => Self::UnprocessableEntity,
            429 => Self::RateLimited,
            code if code >= 500 => Self::Internal,
            _ => Self::Other,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ErrorBody {
    Json(Value),
    Text(String),
}

pub(crate) fn deserialize_body(content: &[u8]) -> Option<ErrorBody> {
    if content.is_empty() {
        return None;
    }
    match serde_json::from_slice::<Value>(content) {
        Ok(Value::Null) => None,
        Ok(value) => Some(ErrorBody::Json(value)),
        Err(_) => Some(ErrorBody::Text(
            String::from_utf8_lossy(content).into_owned(),
        )),
    }
}

pub(crate) fn api_error(
    status: u16,
    body: Option<ErrorBody>,
    headers: HeaderMap,
    endpoint: Option<String>,
) -> Error {
    let kind = ApiErrorKind::from_status(status);
    let retry_after = parse_retry_after(&headers);
    let message = error_message(&body);
    Error::Api(Box::new(ApiError {
        status,
        kind,
        body,
        headers,
        endpoint,
        field_path: None,
        retry_after,
        message,
    }))
}

pub(crate) fn validation_error(
    status: u16,
    body: Option<ErrorBody>,
    headers: HeaderMap,
    endpoint: Option<String>,
    field_path: String,
) -> Error {
    Error::Api(Box::new(ApiError {
        status,
        kind: ApiErrorKind::ResponseValidation,
        body,
        headers,
        endpoint,
        field_path: Some(field_path.clone()),
        retry_after: None,
        message: format!("Invalid response data at '{field_path}'."),
    }))
}

fn error_message(body: &Option<ErrorBody>) -> String {
    match body {
        None => "status code (no body)".to_string(),
        Some(ErrorBody::Text(text)) => truncate(text),
        Some(ErrorBody::Json(value)) => match extract_message(value) {
            Some(detail) if !detail.is_empty() => detail,
            _ => truncate(&compact_json(value)),
        },
    }
}

pub(crate) fn extract_message(body: &Value) -> Option<String> {
    if let Value::String(text) = body {
        return if text.is_empty() {
            None
        } else {
            Some(text.clone())
        };
    }
    let object = body.as_object()?;
    match object.get("error") {
        Some(Value::String(error)) => return Some(error.clone()),
        Some(Value::Object(error)) => {
            if let Some(Value::String(message)) = error.get("message") {
                return Some(message.clone());
            }
        }
        _ => {}
    }
    if let Some(Value::String(message)) = object.get("message") {
        return Some(message.clone());
    }
    match object.get("detail") {
        Some(Value::String(detail)) => Some(detail.clone()),
        Some(Value::Object(detail)) => detail
            .get("message")
            .and_then(Value::as_str)
            .map(str::to_string),
        Some(Value::Array(entries)) => {
            let parts: Vec<String> = entries
                .iter()
                .filter_map(|entry| {
                    let entry = entry.as_object()?;
                    let msg = entry.get("msg")?.as_str()?;
                    let path = match entry.get("loc") {
                        Some(Value::Array(loc)) => loc
                            .iter()
                            .filter_map(|item| match item {
                                Value::String(text) if text == "body" => None,
                                Value::String(text) => Some(text.clone()),
                                Value::Number(number) => Some(number.to_string()),
                                _ => None,
                            })
                            .collect::<Vec<_>>()
                            .join("."),
                        _ => String::new(),
                    };
                    if path.is_empty() {
                        Some(msg.to_string())
                    } else {
                        Some(format!("{path}: {msg}"))
                    }
                })
                .collect();
            if parts.is_empty() {
                None
            } else {
                Some(parts.join("; "))
            }
        }
        _ => None,
    }
}

fn compact_json(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| value.to_string())
}

fn truncate(raw: &str) -> String {
    if raw.chars().count() > MAX_ERROR_BODY_LENGTH {
        let clipped: String = raw.chars().take(MAX_ERROR_BODY_LENGTH).collect();
        format!("{clipped}…")
    } else {
        raw.to_string()
    }
}

/// Server-requested wait. Matches the Python SDK's millisecond parser, then converts to a duration.
pub(crate) fn parse_retry_after(headers: &HeaderMap) -> Option<Duration> {
    if let Some(raw) = header_str(headers, crate::constants::RETRY_AFTER_MS_HEADER) {
        if let Some(millis) = parse_numeric_millis(raw, 1.0, false) {
            return Some(Duration::from_secs_f64(millis / 1000.0));
        }
    }
    if let Some(raw) = header_str(headers, crate::constants::RETRY_AFTER_HEADER) {
        if let Some(millis) = parse_numeric_millis(raw, 1000.0, true) {
            return Some(Duration::from_secs_f64(millis / 1000.0));
        }
        if let Some(delay) = parse_http_date_delay(raw) {
            return Some(delay);
        }
    }
    None
}

fn parse_numeric_millis(raw: &str, multiplier: f64, empty_is_zero: bool) -> Option<f64> {
    let trimmed = raw.trim();
    let value = if trimmed.is_empty() {
        if empty_is_zero {
            0.0
        } else {
            return None;
        }
    } else {
        trimmed.parse::<f64>().ok()?
    };
    if !value.is_finite() {
        return None;
    }
    if value < 0.0 {
        return None;
    }
    let delay = value * multiplier;
    if !delay.is_finite() {
        return None;
    }
    Some(delay)
}

fn parse_http_date_delay(raw: &str) -> Option<Duration> {
    let parsed = httpdate::parse_http_date(raw).ok()?;
    let now = std::time::SystemTime::now();
    match parsed.duration_since(now) {
        Ok(delay) => Some(delay),
        Err(_) => Some(Duration::ZERO),
    }
}

pub(crate) fn header_str<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name).and_then(|value| value.to_str().ok())
}

pub(crate) fn format_endpoint(method: &str, url: &str) -> String {
    let without_fragment = url.split('#').next().unwrap_or(url);
    let without_query = without_fragment
        .split('?')
        .next()
        .unwrap_or(without_fragment);
    format!("{method} {}", strip_userinfo(without_query))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn extract_message_order() {
        assert_eq!(
            extract_message(&json!({"error": "error", "message": "message", "detail": "detail"})),
            Some("error".to_string())
        );
        assert_eq!(
            extract_message(&json!({"error": {"message": "nested error"}, "message": "message"})),
            Some("nested error".to_string())
        );
        assert_eq!(
            extract_message(&json!({"message": "message", "detail": "detail"})),
            Some("message".to_string())
        );
        assert_eq!(
            extract_message(&json!({"detail": "detail"})),
            Some("detail".to_string())
        );
        assert_eq!(
            extract_message(&json!({"detail": {"message": "nested detail"}})),
            Some("nested detail".to_string())
        );
        assert_eq!(
            extract_message(&json!({
                "detail": [
                    {"loc": ["body", "questions", "q", "score", "criteria", 0], "msg": "Invalid"},
                    {"msg": "Missing"},
                    {}
                ]
            })),
            Some("questions.q.score.criteria.0: Invalid; Missing".to_string())
        );
    }

    #[test]
    fn endpoint_drops_userinfo_and_query() {
        assert_eq!(
            format_endpoint(
                "GET",
                "https://user:password@example.test/v1/models?token=secret#fragment"
            ),
            "GET https://example.test/v1/models"
        );
    }
}

fn strip_userinfo(url: &str) -> String {
    match url.split_once("://") {
        Some((scheme, rest)) => {
            if let Some(at) = rest.find('@') {
                format!("{scheme}://{}", &rest[at + 1..])
            } else {
                url.to_string()
            }
        }
        None => url.to_string(),
    }
}
