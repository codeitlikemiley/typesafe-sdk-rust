//! Confidence-gated routing. See https://docs.typesafe.ai/confidence
//!
//! Mock: `cargo run --example confidence_routing --features mock`
//! Live: `TYPESAFE_LIVE=1 cargo run --example confidence_routing`

#[path = "support/fixtures.rs"]
mod fixtures;
#[path = "support/harness.rs"]
mod harness;

use typesafe_sdk::Question;

const FLOOR: f64 = 0.5;
const TRANSFER_CONFIRM: f64 = 0.9;

#[tokio::main]
async fn main() -> Result<(), typesafe_sdk::Error> {
    let rt = harness::ExampleRuntime::start(fixtures::confidence_routing()).await?;
    println!("mode={}", harness::ExampleRuntime::mode_label());

    let user_message = "Please approve the pending withdrawal on my account.";
    let response = rt
        .client
        .system_one(
            user_message,
            [(
                "action",
                Question::choice(
                    "What is the user trying to do?",
                    [
                        ("check_balance", Some("View balance".into())),
                        ("approve_transfer", Some("Approve withdrawal".into())),
                        ("support", Some("General help".into())),
                    ],
                ),
            )],
        )
        .await?;

    let action = response.choice("action")?;
    let confidence = action.confidence;

    let route = if confidence < FLOOR {
        "human"
    } else if action.choice == "check_balance" {
        "show_balance"
    } else if action.choice == "approve_transfer" {
        if confidence > TRANSFER_CONFIRM {
            "confirm_then_execute"
        } else {
            "ask_user_confirm"
        }
    } else {
        "support_queue"
    };

    println!(
        "choice={} confidence={confidence} route={route}",
        action.choice
    );
    Ok(())
}
