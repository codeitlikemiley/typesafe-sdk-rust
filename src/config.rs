//! Configuration resolution from explicit options and environment.

use std::collections::HashMap;
use std::env;

use crate::constants::{
    API_KEY_ENV, BASE_URL_ENV, DEFAULT_BASE_URL, DEFAULT_MODEL, DEFAULT_MODEL_ENV,
    DEFAULT_TIMEOUT_SECS,
};
use crate::error::TypeSafeError;

#[derive(Clone, Debug)]
pub struct Config {
    pub api_key: String,
    pub base_url: String,
    pub default_model: String,
    pub timeout_secs: f64,
    pub default_headers: HashMap<String, String>,
}

impl Config {
    pub fn resolve(
        api_key: Option<String>,
        base_url: Option<String>,
        default_model: Option<String>,
        timeout_secs: Option<f64>,
        default_headers: Option<HashMap<String, String>>,
    ) -> Result<Self, TypeSafeError> {
        let key = resolve_env(api_key, API_KEY_ENV, None);
        let key = key.ok_or_else(|| {
            TypeSafeError::new(format!(
                "No API key was provided. Pass api_key or set the {} environment variable.",
                API_KEY_ENV
            ))
        })?;
        let resolved_base = resolve_env(base_url, BASE_URL_ENV, Some(DEFAULT_BASE_URL))
            .unwrap_or(DEFAULT_BASE_URL.to_string());
        let resolved_model = resolve_env(default_model, DEFAULT_MODEL_ENV, Some(DEFAULT_MODEL))
            .unwrap_or(DEFAULT_MODEL.to_string());
        let timeout = resolve_timeout(
            timeout_secs.unwrap_or(DEFAULT_TIMEOUT_SECS),
        )?;
        Ok(Self {
            api_key: key,
            base_url: resolved_base.trim_end_matches('/').to_string(),
            default_model: resolved_model,
            timeout_secs: timeout,
            default_headers: default_headers.unwrap_or_default(),
        })
    }
}

fn resolve_env(value: Option<String>, env: &str, default: Option<&str>) -> Option<String> {
    if let Some(v) = value {
        let trimmed = v.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    let from_env = env::var(env).unwrap_or_default();
    let trimmed = from_env.trim();
    if !trimmed.is_empty() {
        return Some(trimmed.to_string());
    }
    default.map(|s| s.to_string())
}

pub fn resolve_timeout(timeout_secs: f64) -> Result<f64, TypeSafeError> {
    if !timeout_secs.is_finite() || timeout_secs <= 0.0 {
        return Err(TypeSafeError::new(
            "timeout must be a positive, finite number of seconds.",
        ));
    }
    Ok(timeout_secs)
}
