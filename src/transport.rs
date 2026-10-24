//! How a [`Client`](crate::Client) sends its HTTP requests.
//!
//! The client builds every request (headers, compression, multipart images),
//! retries it, and decodes the answer. A [`Transport`] only moves bytes and
//! waits between retries. With the `reqwest` feature, on by default, the
//! client uses `ReqwestTransport`. Without it, as on `wasm32-wasip2` inside
//! a host such as Spin or wasmCloud, pass your own with
//! [`ClientBuilder::transport`](crate::ClientBuilder::transport).

use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use bytes::Bytes;
use http::HeaderMap;

use crate::error::Error;

/// A boxed `Send` future. `Send` on wasm too: server frameworks such as
/// Leptos and axum require it, and Spin's outgoing HTTP futures are `Send`.
/// Wrap a host future that is not (`send_wrapper`) on single-threaded wasm.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// One attempt, ready to send.
#[derive(Clone, Debug)]
pub struct TransportRequest {
    pub method: http::Method,
    pub url: String,
    /// Includes `Authorization`. Do not log it.
    pub headers: HeaderMap,
    pub body: Option<Bytes>,
    /// The per-attempt timeout. Return [`Error::Timeout`] when it elapses.
    pub timeout: Duration,
}

/// The response to one attempt, whatever its status. The client turns a
/// non-2xx status into [`Error::Api`] and decides whether to retry.
#[derive(Clone, Debug)]
pub struct TransportResponse {
    pub status: u16,
    pub headers: HeaderMap,
    pub body: Bytes,
}

/// Sends requests and waits between retries.
///
/// Return [`Error::Timeout`] when the attempt's timeout elapses and
/// [`Error::Connection`] for any other failure to send or read. The client
/// masks credentials in a `Connection` message, so the message may quote the
/// underlying error as is.
pub trait Transport: Send + Sync + 'static {
    fn send(&self, request: TransportRequest) -> BoxFuture<'_, Result<TransportResponse, Error>>;

    /// Waits before a retry. A transport without a timer can return at once,
    /// but then set a [`RetryPolicy`](crate::RetryPolicy) with no retries:
    /// retrying a 429 without waiting only fails again.
    fn sleep(&self, duration: Duration) -> BoxFuture<'_, ()>;
}

/// The default transport: a `reqwest` 0.12 client and Tokio timers.
#[cfg(feature = "reqwest")]
#[derive(Clone, Debug)]
pub struct ReqwestTransport {
    client: reqwest::Client,
}

#[cfg(feature = "reqwest")]
impl ReqwestTransport {
    pub fn new(client: reqwest::Client) -> Self {
        Self { client }
    }
}

#[cfg(feature = "reqwest")]
impl Transport for ReqwestTransport {
    fn send(&self, request: TransportRequest) -> BoxFuture<'_, Result<TransportResponse, Error>> {
        Box::pin(async move {
            let timeout = request.timeout;
            let to_error = move |error: reqwest::Error| {
                if error.is_timeout() {
                    Error::Timeout { timeout }
                } else {
                    Error::Connection {
                        message: error.without_url().to_string(),
                    }
                }
            };
            let mut builder = self
                .client
                .request(request.method, &request.url)
                .timeout(timeout)
                .headers(request.headers);
            if let Some(body) = request.body {
                builder = builder.body(body);
            }
            let response = builder.send().await.map_err(to_error)?;
            let status = response.status().as_u16();
            let headers = response.headers().clone();
            let body = response.bytes().await.map_err(to_error)?;
            Ok(TransportResponse {
                status,
                headers,
                body,
            })
        })
    }

    fn sleep(&self, duration: Duration) -> BoxFuture<'_, ()> {
        Box::pin(tokio::time::sleep(duration))
    }
}
