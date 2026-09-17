//! Speculative fan-out triage. See https://docs.typesafe.ai/patterns/fan-out
//!
//! Mock (default): `cargo run --example fan_out --features mock`
//! Live API: `TYPESAFE_LIVE=1 cargo run --example fan_out`

#[path = "support/fixtures.rs"]
mod fixtures;
#[path = "support/harness.rs"]
mod harness;

use typesafe_sdk::Question;

#[tokio::main]
async fn main() -> Result<(), typesafe_sdk::Error> {
    let rt = harness::ExampleRuntime::start(fixtures::fan_out_triage()).await?;
    println!("mode={}", harness::ExampleRuntime::mode_label());

    let ticket = "The export button spins forever after the last deploy. I need this fixed today.";
    let response = rt
        .client
        .system_one(
            ticket,
            [
                (
                    "category",
                    Question::choice(
                        "Which category fits this ticket?",
                        [
                            ("bug_report", Some("Software defect".into())),
                            ("billing", Some("Payment issue".into())),
                            ("feature_request", Some("New capability".into())),
                        ],
                    ),
                ),
                (
                    "bug_severity",
                    Question::score(
                        "If this is a bug, how severe?",
                        ["cosmetic", "annoying", "blocking"],
                    ),
                ),
                (
                    "has_reproducible_steps",
                    Question::noul("Does the ticket describe reproducible steps?"),
                ),
                (
                    "refund_requested",
                    Question::noul("Is the customer asking for a refund?"),
                ),
                (
                    "frustration",
                    Question::score(
                        "How frustrated does the customer sound?",
                        ["calm", "frustrated", "angry"],
                    ),
                ),
            ],
        )
        .await?;

    let category = response.choice("category")?;
    let bug_severity = response.score("bug_severity")?;
    let bug_repro = response.noul("has_reproducible_steps")?;
    let refund = response.noul("refund_requested")?;
    let frustration = response.score("frustration")?;

    let mut action = "log_only";
    if category.choice == "bug_report" {
        if bug_severity.score > 1.5 && bug_repro.noul > 0.6 {
            action = "escalate_engineering";
        } else {
            action = "bug_backlog";
        }
    } else if category.choice == "billing" {
        action = if refund.noul > 0.7 {
            "billing_refund_flag"
        } else {
            "billing_queue"
        };
    } else if category.choice == "feature_request" {
        action = "feature_log";
    }

    if frustration.score > 1.5 {
        println!("priority_flag=true");
    }

    println!("category={} action={action}", category.choice);
    Ok(())
}
