//! HTTP request preparation and header merging.

use std::collections::HashMap;

use serde_json::Value;

use crate::config::{resolve_timeout, Config};
use crate::constants::{
    ACCEPT_HEADER, AUTHORIZATION_HEADER, CONTENT_TYPE_HEADER, JSON_CONTENT_TYPE, RETRY_COUNT_HEADER,
    RUNTIME_HEADER, SDK_HEADER, SDK_NAME, USER_AGENT_HEADER,
};
use crate::error::TypeSafeError;

const VERSION: &str = env!("CARGO_PKG_VERSION");

pub struct PreparedRequest {
    pub method: String,
    pub url: String,
    pub headers: HashMap<String, String>,
    pub body: Option<Vec<u8>>,
    pub timeout_secs: f64,
}

impl PreparedRequest {
    pub fn clone_for_attempt(&self, attempt: u32) -> Self {
        Self {
            method: self.method.clone(),
            url: self.url.clone(),
            headers: with_retry_count(&self.headers, attempt),
            body: self.body.clone(),
            timeout_secs: self.timeout_secs,
        }
    }
}

pub fn runtime_header() -> String {
    format!(
        "rust/{} ({}; {})",
        rustc_version(),
        std::env::consts::OS,
        std::env::consts::ARCH
    )
}

fn rustc_version() -> &'static str {
    option_env!("RUSTC_VERSION").unwrap_or("unknown")
}

pub fn prepare(
    config: &Config,
    method: &str,
    path: &str,
    body: Option<Value>,
    timeout_secs: Option<f64>,
    extra_headers: Option<&HashMap<String, String>>,
) -> Result<PreparedRequest, TypeSafeError> {
    let mut headers = config.default_headers.clone();
    if let Some(extra) = extra_headers {
        headers.extend(extra.clone());
    }
    headers.remove(RETRY_COUNT_HEADER);
    headers.insert(
        AUTHORIZATION_HEADER.to_string(),
        format!("Bearer {}", config.api_key),
    );
    headers.insert(ACCEPT_HEADER.to_string(), JSON_CONTENT_TYPE.to_string());
    headers.insert(
        USER_AGENT_HEADER.to_string(),
        format!("{}/{}", SDK_NAME, VERSION),
    );
    headers.insert(
        SDK_HEADER.to_string(),
        format!("{}/{}", SDK_NAME, VERSION),
    );
    headers.insert(RUNTIME_HEADER.to_string(), runtime_header());

    let content = match body {
        None => None,
        Some(v) => {
            headers.insert(CONTENT_TYPE_HEADER.to_string(), JSON_CONTENT_TYPE.to_string());
            Some(serde_json::to_vec(&v).map_err(|_| {
                TypeSafeError::new("The request body could not be encoded as JSON")
            })?)
        }
    };

    let timeout = resolve_timeout(
        timeout_secs.unwrap_or(config.timeout_secs),
    )?;

    Ok(PreparedRequest {
        method: method.to_string(),
        url: format!("{}{}", config.base_url, path),
        headers,
        body: content,
        timeout_secs: timeout,
    })
}

pub fn with_retry_count(headers: &HashMap<String, String>, attempt: u32) -> HashMap<String, String> {
    let mut h = headers.clone();
    if attempt > 0 {
        h.insert(RETRY_COUNT_HEADER.to_string(), attempt.to_string());
    }
    h
}

pub fn endpoint(method: &str, url: &str) -> String {
    format!("{} {}", method, url)
}
