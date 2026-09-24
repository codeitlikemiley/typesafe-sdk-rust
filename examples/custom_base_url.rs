//! Call a System One server that is not `https://api.typesafe.ai`.
//!
//! The client appends `/v1/systemone` and `/v1/models` to the base URL.
//! A trailing slash on the base URL is removed.
//!
//! Mock: `cargo run --example custom_base_url --features mock`
//! Live: `TYPESAFE_LIVE=1 TYPESAFE_BASE_URL=https://your-host TYPESAFE_API_KEY=... cargo run --example custom_base_url`

use typesafe_sdk::{Client, Question};

fn is_live() -> bool {
    std::env::var("TYPESAFE_LIVE")
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes"
            )
        })
        .unwrap_or(false)
}

#[cfg(feature = "mock")]
use typesafe_sdk::RetryPolicy;
#[cfg(feature = "mock")]
use wiremock::matchers::{header, method, path};
#[cfg(feature = "mock")]
use wiremock::{Mock, MockServer, ResponseTemplate};

async fn ask(client: &Client, base_url: &str) -> Result<(), typesafe_sdk::Error> {
    println!("base_url={base_url}");
    let response = client
        .system_one(
            "I was charged twice.",
            [("billing", Question::noul("Is this ticket about billing?"))],
        )
        .await?;
    let models = client.models().await?;
    println!("billing={}", response.noul("billing")?.noul);
    println!("models={}", models.models.len());
    Ok(())
}

async fn run_live() -> Result<(), typesafe_sdk::Error> {
    let base_url = std::env::var(typesafe_sdk::BASE_URL_ENV).map_err(|_| {
        typesafe_sdk::Error::Sdk(
            "Set TYPESAFE_BASE_URL to your System One server. This example does not fall back to https://api.typesafe.ai."
                .into(),
        )
    })?;
    let api_key = std::env::var(typesafe_sdk::API_KEY_ENV).map_err(|_| {
        typesafe_sdk::Error::Sdk(
            "No API key was provided. Pass api_key or set the TYPESAFE_API_KEY environment variable."
                .into(),
        )
    })?;
    let client = Client::builder()
        .api_key(api_key)
        .base_url(base_url.trim())
        .build()?;
    ask(&client, base_url.trim()).await
}

#[cfg(feature = "mock")]
async fn run_mock() -> Result<(), typesafe_sdk::Error> {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .and(header("authorization", "Bearer mock-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "model": "local-model",
            "usage": {"input_tokens": 4, "output_tokens": 1},
            "answers": {"billing": {"type": "noul", "noul": 0.91}}
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .and(header("authorization", "Bearer mock-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "models": [{
                "name": "local-model",
                "description": "fixture",
                "release_date": "2026-01-01"
            }]
        })))
        .mount(&server)
        .await;
    let base_url = format!("{}/", server.uri());
    let client = Client::builder()
        .api_key("mock-key")
        .base_url(&base_url)
        .retry(RetryPolicy::disabled())
        .build()?;
    ask(&client, server.uri().trim_end_matches('/')).await
}

#[tokio::main]
async fn main() -> Result<(), typesafe_sdk::Error> {
    if is_live() {
        return run_live().await;
    }
    #[cfg(feature = "mock")]
    {
        return run_mock().await;
    }
    #[cfg(not(feature = "mock"))]
    {
        Err(typesafe_sdk::Error::Sdk(
            "Mock mode needs feature mock. Run `cargo run --example custom_base_url --features mock`, \
             or set TYPESAFE_LIVE=1 with TYPESAFE_BASE_URL and TYPESAFE_API_KEY."
                .into(),
        ))
    }
}
