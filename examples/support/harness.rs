use typesafe_sdk::{Client, Error, RetryPolicy};

#[cfg(feature = "mock")]
use serde_json::Value;
#[cfg(feature = "mock")]
use wiremock::matchers::{header, method, path};
#[cfg(feature = "mock")]
use wiremock::{Mock, MockServer, ResponseTemplate};

/// True when `TYPESAFE_LIVE` is `1`, `true`, or `yes` (case insensitive).
pub fn is_live() -> bool {
    std::env::var("TYPESAFE_LIVE")
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes"
            )
        })
        .unwrap_or(false)
}

/// Async client plus an optional mock server handle that must stay alive for the call.
pub struct ExampleRuntime {
    pub client: Client,
    #[cfg(feature = "mock")]
    _mock: Option<MockServer>,
}

impl ExampleRuntime {
    /// Live API from the environment, or wiremock with a canned System One body.
    #[cfg(feature = "mock")]
    pub async fn start(mock_body: Value) -> Result<Self, Error> {
        if is_live() {
            return Ok(Self {
                client: Client::from_env()?,
                _mock: None,
            });
        }
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/systemone"))
            .and(header("authorization", "Bearer mock-key"))
            .respond_with(ResponseTemplate::new(200).set_body_json(mock_body))
            .mount(&server)
            .await;
        let client = Client::builder()
            .api_key("mock-key")
            .base_url(server.uri())
            .retry(RetryPolicy::disabled())
            .build()?;
        Ok(Self {
            client,
            _mock: Some(server),
        })
    }

    #[cfg(not(feature = "mock"))]
    pub async fn start(_mock_body: serde_json::Value) -> Result<Self, Error> {
        if is_live() {
            Ok(Self {
                client: Client::from_env()?,
            })
        } else {
            Err(Error::Sdk(
                "Mock mode needs feature mock. Run `cargo run --example NAME --features mock`, \
                 or set TYPESAFE_LIVE=1 with TYPESAFE_API_KEY for the real API."
                    .into(),
            ))
        }
    }

    pub fn mode_label() -> &'static str {
        if is_live() { "live" } else { "mock" }
    }
}
