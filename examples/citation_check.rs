//! Citation support check. See https://docs.typesafe.ai/cookbooks/citation_check
//!
//! Mock: `cargo run --example citation_check --features mock`
//! Live: `TYPESAFE_LIVE=1 cargo run --example citation_check`

#[path = "support/fixtures.rs"]
mod fixtures;
#[path = "support/harness.rs"]
mod harness;

use typesafe_sdk::Question;

const REVIEW_CONFIDENCE: f64 = 0.55;

#[tokio::main]
async fn main() -> Result<(), typesafe_sdk::Error> {
    let rt = harness::ExampleRuntime::start(fixtures::citation_check()).await?;
    println!("mode={}", harness::ExampleRuntime::mode_label());

    let state = serde_json::json!({
        "source": "Section 4.2: Refunds are issued within 5 business days for duplicate charges.",
        "claim": "Customers receive refunds the same day for duplicate charges."
    });

    let response = rt
        .client
        .system_one(
            state,
            [(
                "supports_claim",
                Question::choice(
                    "Does the source support the claim?",
                    [
                        ("yes", Some("Fully supports".into())),
                        ("partially", Some("Partially supports".into())),
                        ("no", Some("Does not support".into())),
                    ],
                ),
            )],
        )
        .await?;

    let answer = response.choice("supports_claim")?;
    let needs_review = answer.confidence < REVIEW_CONFIDENCE || answer.choice != "yes";

    println!(
        "verdict={} confidence={} needs_review={needs_review}",
        answer.choice, answer.confidence
    );
    Ok(())
}
