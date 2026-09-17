//! Composite scoring in code. See https://docs.typesafe.ai/patterns/composite-scoring
//!
//! Mock: `cargo run --example composite_scoring --features mock`
//! Live: `TYPESAFE_LIVE=1 cargo run --example composite_scoring`

#[path = "support/fixtures.rs"]
mod fixtures;
#[path = "support/harness.rs"]
mod harness;

use typesafe_sdk::Question;

#[tokio::main]
async fn main() -> Result<(), typesafe_sdk::Error> {
    let rt = harness::ExampleRuntime::start(fixtures::composite_scores()).await?;
    println!("mode={}", harness::ExampleRuntime::mode_label());

    let draft = "Thanks for reaching out. We will look into the duplicate charge soon.";
    let response = rt
        .client
        .system_one(
            draft,
            [
                (
                    "clarity",
                    Question::score("How clear is the reply?", ["poor", "ok", "excellent"]),
                ),
                (
                    "completeness",
                    Question::score(
                        "Does it address the customer's issue?",
                        ["missing", "partial", "complete"],
                    ),
                ),
                (
                    "tone",
                    Question::score("How warm is the tone?", ["harsh", "neutral", "warm"]),
                ),
            ],
        )
        .await?;

    let clarity = response.score("clarity")?.score;
    let completeness = response.score("completeness")?.score;
    let tone = response.score("tone")?.score;

    let weighted = 0.4 * clarity + 0.4 * completeness + 0.2 * tone;
    let publish = weighted >= 1.8;

    println!(
        "clarity={clarity} completeness={completeness} tone={tone} weighted={weighted:.2} publish={publish}"
    );
    Ok(())
}
