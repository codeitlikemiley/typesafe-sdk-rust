//! Ask named System One questions and print typed answers.
//!
//! Set `TYPESAFE_API_KEY`, then run:
//!
//! ```bash
//! cargo run --example system_one
//! ```

use typesafe_sdk::{Client, Question};

#[tokio::main]
async fn main() -> Result<(), typesafe_sdk::Error> {
    let client = Client::from_env()?;
    let response = client
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
