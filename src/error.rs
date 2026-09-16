//! SDK error types and HTTP error mapping.

use std::collections::HashMap;
use std::fmt;

use httpdate::parse_http_date;
use serde_json::Value;

use crate::constants::{MAX_ERROR_BODY_LENGTH, REQUEST_ID_HEADER};

/// Base error for SDK failures.
#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub struct TypeSafeError {
    message: String,
}

impl TypeSafeError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

/// An unsuccessful HTTP response with body and request metadata.
#[derive(Debug)]
pub struct TypeSafeApiError {
    pub status: u16,
    pub body: Option<Value>,
    pub headers: HashMap<String, String>,
    message: String,
    pub endpoint: Option<String>,
}

impl TypeSafeApiError {
    pub fn new(
        status: u16,
        body: Option<Value>,
        headers: HashMap<String, String>,
        message: Option<String>,
        endpoint: Option<String>,
    ) -> Self {
        let message = message.unwrap_or_else(|| extract_message(&body).unwrap_or_else(|| {
            match &body {
                None => "status code (no body)".to_string(),
                Some(Value::String(s)) => truncate(s.clone()),
                Some(v) => truncate(v.to_string()),
            }
        }));
        Self {
            status,
            body,
            headers,
            message,
            endpoint,
        }
    }

    pub fn request_id(&self) -> Option<&str> {
        self.headers
            .get(REQUEST_ID_HEADER)
            .map(|s| s.as_str())
            .or_else(|| {
                self.headers
                    .iter()
                    .find(|(k, _)| k.eq_ignore_ascii_case(REQUEST_ID_HEADER))
                    .map(|(_, v)| v.as_str())
            })
    }
}

impl fmt::Display for TypeSafeApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut message = format!("{} {}", self.status, self.message);
        if let Some(endpoint) = &self.endpoint {
            message = format!("{}: {}", endpoint, message);
        }
        if let Some(id) = self.request_id() {
            message = format!("{} (request_id={})", message, id);
        }
        f.write_str(&message)
    }
}

impl std::error::Error for TypeSafeApiError {}

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct TypeSafeBadRequestError(pub TypeSafeApiError);

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct TypeSafeAuthenticationError(pub TypeSafeApiError);

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct TypeSafePermissionDeniedError(pub TypeSafeApiError);

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct TypeSafeNotFoundError(pub TypeSafeApiError);

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct TypeSafeUnprocessableEntityError(pub TypeSafeApiError);

#[derive(Debug)]
pub struct TypeSafeRateLimitError {
    pub inner: TypeSafeApiError,
    pub retry_after_ms: Option<f64>,
}

impl fmt::Display for TypeSafeRateLimitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner.fmt(f)
    }
}

impl std::error::Error for TypeSafeRateLimitError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.inner)
    }
}

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct TypeSafeInternalServerError(pub TypeSafeApiError);

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct TypeSafeApiConnectionError(pub String);

#[derive(Debug, thiserror::Error)]
#[error("Request timed out (timeout={timeout_secs}s).")]
pub struct TypeSafeApiTimeoutError {
    pub timeout_secs: f64,
}

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct TypeSafeApiResponseValidationError(pub TypeSafeApiError);

impl TypeSafeApiResponseValidationError {
    pub fn field_path(&self) -> &str {
        &self.0.message
    }
}

/// Internal error wrapper for retry logic.
#[derive(Debug)]
pub enum ApiFailure {
    Api(TypeSafeApiError),
    Connection(TypeSafeApiConnectionError),
    Timeout(TypeSafeApiTimeoutError),
    Validation(TypeSafeApiResponseValidationError),
    Sdk(TypeSafeError),
}

impl From<TypeSafeError> for ApiFailure {
    fn from(e: TypeSafeError) -> Self {
        ApiFailure::Sdk(e)
    }
}

pub fn api_error(
    status: u16,
    body: Option<Value>,
    headers: HashMap<String, String>,
    endpoint: Option<String>,
) -> TypeSafeApiError {
    let err = TypeSafeApiError::new(status, body, headers, None, endpoint);
    err
}

pub fn map_status_error(err: TypeSafeApiError) -> Box<dyn std::error::Error + Send + Sync> {
    let status = err.status;
    if status == 400 {
        return Box::new(TypeSafeBadRequestError(err));
    }
    if status == 401 {
        return Box::new(TypeSafeAuthenticationError(err));
    }
    if status == 403 {
        return Box::new(TypeSafePermissionDeniedError(err));
    }
    if status == 404 {
        return Box::new(TypeSafeNotFoundError(err));
    }
    if status == 422 {
        return Box::new(TypeSafeUnprocessableEntityError(err));
    }
    if status == 429 {
        let retry_after_ms = parse_retry_after(&err.headers);
        return Box::new(TypeSafeRateLimitError {
            inner: err,
            retry_after_ms,
        });
    }
    if status >= 500 {
        return Box::new(TypeSafeInternalServerError(err));
    }
    Box::new(err)
}

pub fn parse_retry_after(headers: &HashMap<String, String>) -> Option<f64> {
    let get = |name: &str| -> Option<String> {
        headers
            .get(name)
            .cloned()
            .or_else(|| {
                headers
                    .iter()
                    .find(|(k, _)| k.eq_ignore_ascii_case(name))
                    .map(|(_, v)| v.clone())
            })
    };

    for (name, multiplier) in [(RETRY_AFTER_MS_HEADER, 1.0), (RETRY_AFTER_HEADER, 1000.0)] {
        let raw = get(name)?;
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Ok(value) = trimmed.parse::<f64>() {
            if value.is_finite() {
                if value >= 0.0 {
                    let delay = value * multiplier;
                    if delay.is_finite() {
                        return Some(delay);
                    }
                } else if name == RETRY_AFTER_HEADER {
                    return None;
                }
            }
            continue;
        }
        if name == RETRY_AFTER_HEADER {
            if let Ok(dt) = parse_http_date(trimmed) {
                let now = std::time::SystemTime::now();
                let delay_ms = dt
                    .duration_since(now)
                    .unwrap_or_default()
                    .as_secs_f64()
                    * 1000.0;
                return Some(delay_ms.max(0.0));
            }
        }
    }
    None
}

use crate::constants::{RETRY_AFTER_HEADER, RETRY_AFTER_MS_HEADER};

fn extract_message(body: &Option<Value>) -> Option<String> {
    let body = body.as_ref()?;
    if let Value::String(s) = body {
        return if s.is_empty() { None } else { Some(s.clone()) };
    }
    let obj = body.as_object()?;
    if let Some(Value::String(s)) = obj.get("error") {
        return Some(s.clone());
    }
    if let Some(Value::Object(err)) = obj.get("error") {
        if let Some(Value::String(s)) = err.get("message") {
            return Some(s.clone());
        }
    }
    if let Some(Value::String(s)) = obj.get("message") {
        return Some(s.clone());
    }
    if let Some(Value::String(s)) = obj.get("detail") {
        return Some(s.clone());
    }
    if let Some(Value::Object(detail)) = obj.get("detail") {
        if let Some(Value::String(s)) = detail.get("message") {
            return Some(s.clone());
        }
    }
    if let Some(Value::Array(detail)) = obj.get("detail") {
        let mut parts = Vec::new();
        for entry in detail {
            let entry = entry.as_object()?;
            let msg = entry.get("msg")?.as_str()?;
            let path = match entry.get("loc").and_then(|l| l.as_array()) {
                Some(loc) => loc
                    .iter()
                    .filter(|item| item.as_str() != Some("body"))
                    .map(|item| item.to_string())
                    .collect::<Vec<_>>()
                    .join("."),
                None => String::new(),
            };
            parts.push(if path.is_empty() {
                msg.to_string()
            } else {
                format!("{}: {}", path, msg)
            });
        }
        if !parts.is_empty() {
            return Some(parts.join("; "));
        }
    }
    None
}

fn truncate(raw: String) -> String {
    if raw.len() > MAX_ERROR_BODY_LENGTH {
        format!("{}…", raw.chars().take(MAX_ERROR_BODY_LENGTH).collect::<String>())
    } else {
        raw
    }
}
