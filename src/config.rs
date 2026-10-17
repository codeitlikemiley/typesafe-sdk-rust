use std::time::Duration;

use http::HeaderMap;

use crate::constants::{BASE_URL_ENV, DEFAULT_MODEL_ENV, DEFAULT_TIMEOUT_SECS};
use crate::error::Error;
use crate::logging::RedactedHeaders;
use crate::provider::Provider;

/// How the client compresses request bodies.
///
/// Set it with [`ClientBuilder::compression`](crate::ClientBuilder::compression).
/// The default is `Zstd` when the crate's `zstd` feature is on, which it is by
/// default, and `None` without it.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Compression {
    /// Compress request bodies of 1024 bytes or more with zstd at the library's
    /// default level, and send `Content-Encoding: zstd`. Needs the `zstd`
    /// feature: without it, `build` fails.
    Zstd,
    /// Send request bodies uncompressed.
    None,
}

#[derive(Clone)]
pub(crate) struct Config {
    pub provider: Provider,
    pub api_key: String,
    pub base_url: String,
    pub default_model: String,
    pub timeout: Duration,
    pub default_headers: HeaderMap,
    pub compression: Compression,
}

impl std::fmt::Debug for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Config")
            .field("provider", &self.provider)
            .field("api_key", &"***")
            .field("base_url", &self.base_url)
            .field("default_model", &self.default_model)
            .field("timeout", &self.timeout)
            .field("default_headers", &RedactedHeaders(&self.default_headers))
            .field("compression", &self.compression)
            .finish()
    }
}

impl Config {
    /// `TYPESAFE_BASE_URL` and `TYPESAFE_DEFAULT_MODEL` apply to the TypeSafe
    /// provider only. Other providers use their own defaults, and never
    /// compress request bodies.
    #[allow(clippy::too_many_arguments)]
    pub fn resolve(
        api_key: &str,
        provider: Provider,
        base_url: Option<String>,
        default_model: Option<String>,
        timeout: Option<Duration>,
        default_headers: HeaderMap,
        compression: Option<Compression>,
    ) -> Result<Self, Error> {
        let api_key = validate_api_key(api_key, provider.key_env())?;
        let provider = provider.validated()?;
        let typesafe = provider == Provider::TypeSafe;
        let resolve = |value: Option<String>, env: &str, default: &str| {
            if typesafe {
                resolve_env(value, env, Some(default.to_string()))
            } else {
                Some(value.unwrap_or_else(|| default.to_string()))
            }
        };
        let base_url = normalize_base_url(
            resolve(base_url, BASE_URL_ENV, provider.default_base_url())
                .expect("default base URL is present"),
        )?;
        let default_model = resolve(default_model, DEFAULT_MODEL_ENV, provider.default_model())
            .expect("default model is present");
        let compression = resolve_compression(compression)?;
        Ok(Self {
            compression: if typesafe {
                compression
            } else {
                Compression::None
            },
            provider,
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

/// Strips the key and requires printable ASCII without whitespace. The error
/// never contains the key.
fn validate_api_key(api_key: &str, key_env: &str) -> Result<String, Error> {
    let key = api_key.trim();
    if key.is_empty() {
        return Err(Error::sdk(format!(
            "No API key was provided. Pass api_key or set the {key_env} environment variable."
        )));
    }
    if !key.bytes().all(|byte| matches!(byte, 0x21..=0x7e)) {
        return Err(Error::sdk(
            "API key must contain only printable ASCII characters without whitespace.",
        ));
    }
    Ok(key.to_string())
}

fn resolve_compression(compression: Option<Compression>) -> Result<Compression, Error> {
    match compression {
        None if cfg!(feature = "zstd") => Ok(Compression::Zstd),
        None => Ok(Compression::None),
        Some(Compression::Zstd) if !cfg!(feature = "zstd") => Err(Error::sdk(
            "zstd compression requires the `zstd` feature of typesafe-sdk.",
        )),
        Some(compression) => Ok(compression),
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
            "k",
            Provider::TypeSafe,
            base_url.map(str::to_string),
            None,
            None,
            HeaderMap::new(),
            None,
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
