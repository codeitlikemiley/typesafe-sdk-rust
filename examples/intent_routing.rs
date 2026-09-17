//! Intent routing pattern. See https://docs.typesafe.ai/patterns/intent-routing
//!
//! Mock: `cargo run --example intent_routing --features mock`
//! Live: `TYPESAFE_LIVE=1 cargo run --example intent_routing`

#[path = "support/fixtures.rs"]
mod fixtures;
#[path = "support/harness.rs"]
mod harness;

use typesafe_sdk::Question;

#[tokio::main]
async fn main() -> Result<(), typesafe_sdk::Error> {
    let rt = harness::ExampleRuntime::start(fixtures::intent_routing()).await?;
    println!("mode={}", harness::ExampleRuntime::mode_label());

    let request = "Summarize this quarter's revenue drivers in two paragraphs.";
    let response = rt
        .client
        .system_one(
            request,
            [(
                "intent",
                Question::choice(
                    "Best handler for this request",
                    [
                        ("deterministic", Some("Fixed code path".into())),
                        ("specialist_llm", Some("Large model task".into())),
                        ("human", Some("Needs a person".into())),
                    ],
                ),
            )],
        )
        .await?;

    let intent = response.choice("intent")?;
    let handler = match intent.choice.as_str() {
        "deterministic" => "rules_engine",
        "specialist_llm" => "frontier_model",
        _ => "human_queue",
    };

    println!(
        "intent={} confidence={} handler={handler}",
        intent.choice, intent.confidence
    );
    Ok(())
}
