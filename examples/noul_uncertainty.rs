//! Noul probability band for human review. See https://docs.typesafe.ai/cookbooks/consistency_noul_cookbook
//!
//! Mock: `cargo run --example noul_uncertainty --features mock`
//! Live: `TYPESAFE_LIVE=1 cargo run --example noul_uncertainty`

#[path = "support/fixtures.rs"]
mod fixtures;
#[path = "support/harness.rs"]
mod harness;

use typesafe_sdk::Question;

const UNCERTAIN_LOW: f64 = 0.30;
const UNCERTAIN_HIGH: f64 = 0.70;

#[tokio::main]
async fn main() -> Result<(), typesafe_sdk::Error> {
    let rt = harness::ExampleRuntime::start(fixtures::noul_uncertainty_band()).await?;
    println!("mode={}", harness::ExampleRuntime::mode_label());

    let claim = "The claimant's story conflicts with the police report on file.";
    let response = rt
        .client
        .system_one(
            claim,
            [(
                "fraud_likely",
                Question::noul("Is this claim likely fraudulent?"),
            )],
        )
        .await?;

    let p = response.noul("fraud_likely")?.noul;
    let action = if p < UNCERTAIN_LOW {
        "auto_approve"
    } else if p > UNCERTAIN_HIGH {
        "auto_deny"
    } else {
        "human_review"
    };

    println!("fraud_likely={p} action={action}");
    Ok(())
}
