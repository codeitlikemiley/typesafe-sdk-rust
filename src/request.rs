use std::time::Duration;

use http::{HeaderMap, HeaderName, HeaderValue};

use crate::config::Config;
use crate::constants::{
    ACCEPT_HEADER, AUTHORIZATION_HEADER, CONTENT_TYPE_HEADER, JSON_CONTENT_TYPE,
    RETRY_COUNT_HEADER, RUNTIME_HEADER, SDK_HEADER, SDK_NAME, USER_AGENT_HEADER,
};
use crate::error::Error;

#[derive(Clone, Debug)]
pub(crate) struct PreparedRequest {
    pub method: String,
    pub url: String,
    pub headers: HeaderMap,
    pub body: Option<bytes::Bytes>,
    pub timeout: Duration,
}

pub(crate) fn prepare(
    config: &Config,
    method: &str,
    path: &str,
    body: Option<bytes::Bytes>,
    timeout: Option<Duration>,
    extra_headers: &HeaderMap,
) -> Result<PreparedRequest, Error> {
    let mut headers = config.default_headers.clone();
    merge_headers(&mut headers, extra_headers);
    headers.remove(RETRY_COUNT_HEADER);
    let identity = format!("{SDK_NAME}/{}", env!("CARGO_PKG_VERSION"));
    set_header(
        &mut headers,
        AUTHORIZATION_HEADER,
        &format!("Bearer {}", config.api_key),
    )?;
    set_header(&mut headers, ACCEPT_HEADER, JSON_CONTENT_TYPE)?;
    set_header(&mut headers, USER_AGENT_HEADER, &identity)?;
    set_header(&mut headers, SDK_HEADER, &identity)?;
    set_header(&mut headers, RUNTIME_HEADER, &runtime_header())?;
    if body.is_some() {
        set_header(&mut headers, CONTENT_TYPE_HEADER, JSON_CONTENT_TYPE)?;
    }
    Ok(PreparedRequest {
        method: method.to_string(),
        url: format!("{}{path}", config.base_url),
        headers,
        body,
        timeout: crate::config::resolve_timeout(timeout.unwrap_or(config.timeout))?,
    })
}

fn merge_headers(target: &mut HeaderMap, extra: &HeaderMap) {
    for (name, value) in extra {
        target.insert(name.clone(), value.clone());
    }
}

fn set_header(headers: &mut HeaderMap, name: &str, value: &str) -> Result<(), Error> {
    let name = HeaderName::from_bytes(name.as_bytes())
        .map_err(|_| Error::sdk(format!("invalid header name {name}")))?;
    let value = HeaderValue::from_str(value).map_err(|_| Error::sdk("invalid header value"))?;
    headers.insert(name, value);
    Ok(())
}

pub(crate) fn set_retry_count(headers: &mut HeaderMap, attempts: u32) {
    if attempts == 0 {
        headers.remove(RETRY_COUNT_HEADER);
        return;
    }
    if let Ok(value) = HeaderValue::from_str(&attempts.to_string()) {
        if let Ok(name) = HeaderName::from_bytes(RETRY_COUNT_HEADER.as_bytes()) {
            headers.insert(name, value);
        }
    }
}

fn runtime_header() -> String {
    format!(
        "rust/{} ({}; {})",
        env!("TYPESAFE_RUSTC_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH
    )
}
