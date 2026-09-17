//! Structured JSON state. See https://docs.typesafe.ai/concepts/state
//!
//! Mock: `cargo run --example structured_state --features mock`
//! Live: `TYPESAFE_LIVE=1 cargo run --example structured_state`

#[path = "support/fixtures.rs"]
mod fixtures;
#[path = "support/harness.rs"]
mod harness;

use typesafe_sdk::Question;

#[tokio::main]
async fn main() -> Result<(), typesafe_sdk::Error> {
    let rt = harness::ExampleRuntime::start(fixtures::structured_ticket()).await?;
    println!("mode={}", harness::ExampleRuntime::mode_label());

    let state = serde_json::json!({
        "ticket": {
            "subject": "Duplicate charge",
            "messages": [
                {"from": "customer", "text": "I was charged twice for order A-104."}
            ]
        },
        "order": {
            "id": "A-104",
            "charges": [
                {"amount_usd": 49, "status": "captured"},
                {"amount_usd": 49, "status": "captured"}
            ]
        },
        "refund_policy": "Duplicate charges are eligible for a refund."
    });

    let response = rt
        .client
        .system_one(
            state,
            [
                (
                    "duplicate_charge",
                    Question::noul("Does the order show duplicate captured charges?"),
                ),
                (
                    "refund_eligible",
                    Question::noul("Does the refund policy cover this case?"),
                ),
            ],
        )
        .await?;

    println!(
        "duplicate_charge={} refund_eligible={}",
        response.noul("duplicate_charge")?.noul,
        response.noul("refund_eligible")?.noul
    );
    Ok(())
}
