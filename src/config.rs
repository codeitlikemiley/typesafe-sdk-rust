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
        let base_url = normalize_base_url(
            resolve_env(base_url, BASE_URL_ENV, Some(DEFAULT_BASE_URL.to_string()))
                .expect("default base URL is present"),
        )?;
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

/// Trim, drop trailing slashes, and require an absolute http(s) URL.
///
/// Paths `/v1/systemone` and `/v1/models` are appended to this string.
pub(crate) fn normalize_base_url(raw: String) -> Result<String, Error> {
    let trimmed = raw.trim().trim_end_matches('/').to_string();
    if trimmed.is_empty() {
        return Err(Error::sdk(
            "base_url is empty. Pass an absolute http or https URL, or omit it to use the default.",
        ));
    }
    let parsed = url::Url::parse(&trimmed).map_err(|_| {
        Error::sdk(format!(
            "base_url must be an absolute http or https URL, got {trimmed}."
        ))
    })?;
    match parsed.scheme() {
        "http" | "https" => Ok(trimmed),
        other => Err(Error::sdk(format!(
            "base_url scheme must be http or https, got {other}."
        ))),
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

#[cfg(test)]
mod tests {
    use super::*;
    use http::HeaderMap;

    fn resolved(base_url: Option<&str>) -> Result<Config, Error> {
        Config::resolve(
            "k".to_string(),
            base_url.map(str::to_string),
            None,
            None,
            HeaderMap::new(),
        )
    }

    #[test]
    fn strips_trailing_slashes_and_whitespace() {
        let config = resolved(Some("  http://127.0.0.1:9/gateway///  ")).unwrap();
        assert_eq!(config.base_url, "http://127.0.0.1:9/gateway");
    }

    #[test]
    fn rejects_blank_and_non_http_base_url() {
        let blank = resolved(Some("   /")).unwrap_err();
        assert!(blank.to_string().contains("base_url is empty"));
        let ftp = resolved(Some("ftp://files.example/sdk")).unwrap_err();
        assert!(ftp.to_string().contains("scheme must be http or https"));
        let relative = resolved(Some("/v1")).unwrap_err();
        assert!(relative.to_string().contains("absolute http or https"));
    }
}
