//! Blocking client on the live API. Mock mode uses the async client against wiremock
//! so we do not nest Tokio runtimes.
//!
//! Mock: `cargo run --example blocking_triage --features "mock blocking"`
//! Live: `TYPESAFE_LIVE=1 cargo run --example blocking_triage --features blocking`

#[path = "support/fixtures.rs"]
mod fixtures;

use typesafe_sdk::Question;

#[cfg(feature = "blocking")]
use typesafe_sdk::blocking::Client as BlockingClient;

#[cfg(feature = "mock")]
use typesafe_sdk::{Client, RetryPolicy};
#[cfg(feature = "mock")]
use wiremock::matchers::{header, method, path};
#[cfg(feature = "mock")]
use wiremock::{Mock, MockServer, ResponseTemplate};

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

const QUESTIONS: [(&str, fn() -> Question); 1] = [("category", || {
    Question::choice(
        "Category?",
        [
            ("bug_report", None),
            ("billing", None),
            ("feature_request", None),
        ],
    )
})];

fn print_category(response: &typesafe_sdk::SystemOneResponse) -> Result<(), typesafe_sdk::Error> {
    println!("category={}", response.choice("category")?.choice);
    Ok(())
}

#[cfg(feature = "blocking")]
fn run_live() -> Result<(), typesafe_sdk::Error> {
    println!("mode=live client=blocking");
    let client = BlockingClient::from_env()?;
    let response = client.system_one(
        "Billing double-charged my card.",
        QUESTIONS.map(|(name, build)| (name, build())),
    )?;
    print_category(&response)
}

#[cfg(feature = "mock")]
async fn run_mock() -> Result<(), typesafe_sdk::Error> {
    println!("mode=mock client=async");
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .and(header("authorization", "Bearer mock-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixtures::fan_out_triage()))
        .mount(&server)
        .await;
    let client = Client::builder()
        .api_key("mock-key")
        .base_url(server.uri())
        .retry(RetryPolicy::disabled())
        .build()?;
    let response = client
        .system_one(
            "Billing double-charged my card.",
            QUESTIONS.map(|(name, build)| (name, build())),
        )
        .await?;
    print_category(&response)
}

fn main() -> Result<(), typesafe_sdk::Error> {
    if is_live() {
        #[cfg(feature = "blocking")]
        {
            return run_live();
        }
        #[cfg(not(feature = "blocking"))]
        {
            return Err(typesafe_sdk::Error::Sdk(
                "Live blocking example needs --features blocking.".into(),
            ));
        }
    }

    #[cfg(not(feature = "mock"))]
    {
        return Err(typesafe_sdk::Error::Sdk(
            "Use --features \"mock blocking\" or TYPESAFE_LIVE=1 with --features blocking.".into(),
        ));
    }

    #[cfg(feature = "mock")]
    {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| typesafe_sdk::Error::Connection {
                message: error.to_string(),
            })?;
        runtime.block_on(run_mock())
    }
}
