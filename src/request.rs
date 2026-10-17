use std::time::Duration;

use http::{HeaderMap, HeaderName, HeaderValue};

use crate::config::{Compression, Config};
use crate::constants::{
    ACCEPT_HEADER, AUTHORIZATION_HEADER, CONTENT_TYPE_HEADER, JSON_CONTENT_TYPE,
    RETRY_COUNT_HEADER, RUNTIME_HEADER, SDK_HEADER, SDK_NAME, USER_AGENT_HEADER,
};
use crate::error::Error;

/// A request body and its content type.
pub(crate) struct Payload {
    pub content_type: String,
    pub bytes: Vec<u8>,
}

impl Payload {
    pub fn json(bytes: Vec<u8>) -> Self {
        Self {
            content_type: JSON_CONTENT_TYPE.to_string(),
            bytes,
        }
    }
}

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
    body: Option<Payload>,
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
    let body = match body {
        Some(payload) => {
            set_header(&mut headers, CONTENT_TYPE_HEADER, &payload.content_type)?;
            Some(compress(config.compression, payload.bytes, &mut headers)?)
        }
        None => None,
    };
    Ok(PreparedRequest {
        method: method.to_string(),
        url: format!("{}{path}", config.base_url),
        headers,
        body,
        timeout: crate::config::resolve_timeout(timeout.unwrap_or(config.timeout))?,
    })
}

/// Compresses a body of at least 1024 bytes once, for every attempt, and sets
/// `Content-Encoding`. Smaller bodies go as they are.
#[cfg_attr(not(feature = "zstd"), allow(unused_variables))]
fn compress(
    compression: Compression,
    bytes: Vec<u8>,
    headers: &mut HeaderMap,
) -> Result<bytes::Bytes, Error> {
    match compression {
        #[cfg(feature = "zstd")]
        Compression::Zstd if bytes.len() >= crate::constants::MIN_COMPRESSED_BODY_LENGTH => {
            let compressed = zstd::bulk::compress(&bytes, 0)
                .map_err(|_| Error::sdk("The request body could not be compressed with zstd"))?;
            set_header(headers, crate::constants::CONTENT_ENCODING_HEADER, "zstd")?;
            Ok(bytes::Bytes::from(compressed))
        }
        _ => Ok(bytes::Bytes::from(bytes)),
    }
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
