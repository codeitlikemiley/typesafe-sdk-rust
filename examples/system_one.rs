//! Named System One questions with typed answers.
//!
//! Mock: `cargo run --example system_one --features mock`
//! Live: `TYPESAFE_LIVE=1 cargo run --example system_one`

#[path = "support/fixtures.rs"]
mod fixtures;
#[path = "support/harness.rs"]
mod harness;

use typesafe_sdk::Question;

#[tokio::main]
async fn main() -> Result<(), typesafe_sdk::Error> {
    let rt = harness::ExampleRuntime::start(fixtures::mixed_primitives()).await?;
    println!("mode={}", harness::ExampleRuntime::mode_label());

    let response = rt
        .client
        .system_one(
            serde_json::json!({"document": "I was charged twice. Please fix this ASAP."}),
            [
                ("billing", Question::noul("Is this ticket about billing?")),
                (
                    "tone",
                    Question::choice(
                        "What is the customer's tone?",
                        [("calm", None), ("frustrated", None), ("angry", None)],
                    ),
                ),
                (
                    "urgency",
                    Question::score(
                        "How urgent is this ticket?",
                        ["can wait", "this week", "today"],
                    ),
                ),
            ],
        )
        .await?;

    println!("billing={}", response.noul("billing")?.noul);
    println!("tone={}", response.choice("tone")?.choice);
    println!("urgency={}", response.score("urgency")?.score);
    Ok(())
}
