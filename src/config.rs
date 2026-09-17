use std::time::Duration;

use http::HeaderMap;

use crate::constants::{
    BASE_URL_ENV, DEFAULT_BASE_URL, DEFAULT_MODEL, DEFAULT_MODEL_ENV, DEFAULT_TIMEOUT_SECS,
};
use crate::error::Error;

#[derive(Clone, Debug)]
pub(crate) struct Config {
    pub api_key: String,
    pub base_url: String,
    pub default_model: String,
    pub timeout: Duration,
    pub default_headers: HeaderMap,
}

impl Config {
    pub fn resolve(
        api_key: String,
        base_url: Option<String>,
        default_model: Option<String>,
        timeout: Option<Duration>,
        default_headers: HeaderMap,
    ) -> Result<Self, Error> {
        let base_url = resolve_env(base_url, BASE_URL_ENV, Some(DEFAULT_BASE_URL.to_string()))
            .expect("default base URL is present")
            .trim_end_matches('/')
            .to_string();
        let default_model = resolve_env(
            default_model,
            DEFAULT_MODEL_ENV,
            Some(DEFAULT_MODEL.to_string()),
        )
        .expect("default model is present");
        Ok(Self {
            api_key,
            base_url,
            default_model,
            timeout: resolve_timeout(
                timeout.unwrap_or(Duration::from_secs_f64(DEFAULT_TIMEOUT_SECS)),
            )?,
            default_headers,
        })
    }
}

pub(crate) fn resolve_env(
    value: Option<String>,
    env: &str,
    default: Option<String>,
) -> Option<String> {
    if let Some(value) = value {
        return Some(value);
    }
    match std::env::var(env) {
        Ok(raw) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                default
            } else {
                Some(trimmed.to_string())
            }
        }
        Err(_) => default,
    }
}

pub(crate) fn resolve_timeout(timeout: Duration) -> Result<Duration, Error> {
    if timeout.is_zero() {
        return Err(Error::sdk(
            "timeout must be a positive, finite number of seconds.",
        ));
    }
    Ok(timeout)
}
