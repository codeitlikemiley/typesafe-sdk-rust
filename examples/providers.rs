//! The same questions and typed answers on TypeSafe System One, Cloudflare
//! Clef, and OpenAI Decisions (experimental).
//!
//! Mock: `cargo run --example providers --features "mock clef openai-decisions"`
//! Live: `TYPESAFE_LIVE=1 TYPESAFE_PROVIDER=clef CLOUDFLARE_ACCOUNT_ID=... CLOUDFLARE_API_TOKEN=... cargo run --example providers --features "clef openai-decisions"`
//! (or `TYPESAFE_PROVIDER=openai` with `OPENAI_API_KEY`, or unset with `TYPESAFE_API_KEY`).

#[path = "support/fixtures.rs"]
mod fixtures;
#[path = "support/harness.rs"]
#[allow(dead_code)]
mod harness;

use typesafe_sdk::{Client, Question};

async fn ask(label: &str, client: &Client) -> Result<(), typesafe_sdk::Error> {
    // String state, instructions, and descriptions: the subset every
    // provider accepts.
    let response = client
        .system_one(
            "I was charged twice. Please fix this ASAP.",
            [
                ("billing", Question::noul("Is this ticket about billing?")),
                (
                    "tone",
                    Question::choice(
                        "What is the customer's tone?",
                        [
                            ("calm", Some("Even and polite".into())),
                            ("frustrated", Some("Annoyed or impatient".into())),
                        ],
                    ),
                ),
            ],
        )
        .await?;
    println!(
        "{label}: model={} billing={} tone={}",
        response.model,
        response.noul("billing")?.noul,
        response.choice("tone")?.choice
    );
    Ok(())
}

#[cfg(feature = "mock")]
async fn run_mock() -> Result<(), typesafe_sdk::Error> {
    use typesafe_sdk::{Provider, RetryPolicy};
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let server = MockServer::start().await;
    for (route, body) in [
        ("/v1/systemone", fixtures::provider_system_one()),
        (
            "/accounts/mock-account/ai/run/@cf/cloudflare/clef",
            fixtures::provider_clef(),
        ),
        ("/v1/decisions", fixtures::provider_openai()),
    ] {
        Mock::given(method("POST"))
            .and(path(route))
            .and(header("authorization", "Bearer mock-key"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(&server)
            .await;
    }
    for (label, provider) in [
        ("typesafe", Provider::TypeSafe),
        (
            "clef",
            Provider::Clef {
                account_id: "mock-account".to_string(),
            },
        ),
        ("openai", Provider::OpenAiDecisions),
    ] {
        // `base_url` points every provider at the mock, as it would at a gateway.
        let client = Client::builder()
            .api_key("mock-key")
            .provider(provider)
            .base_url(server.uri())
            .retry(RetryPolicy::disabled())
            .build()?;
        ask(label, &client).await?;
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), typesafe_sdk::Error> {
    if harness::is_live() {
        let client = Client::from_env()?;
        let label = std::env::var(typesafe_sdk::PROVIDER_ENV).unwrap_or_else(|_| "typesafe".into());
        return ask(&label, &client).await;
    }
    #[cfg(feature = "mock")]
    {
        run_mock().await
    }
    #[cfg(not(feature = "mock"))]
    {
        Err(typesafe_sdk::Error::Sdk(
            "Mock mode needs feature mock. Run `cargo run --example providers --features \"mock clef openai-decisions\"`, \
             or set TYPESAFE_LIVE=1 with TYPESAFE_PROVIDER and that provider's key."
                .into(),
        ))
    }
}
