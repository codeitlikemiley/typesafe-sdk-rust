# Agent guide for typesafe-sdk

How to call TypeSafe System One from Rust without guessing the API.

## Goal

Emit a compiling async call that asks named questions and reads typed answers.

## Add dependencies

```toml
[dependencies]
typesafe-sdk = "0.1"
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
serde_json = "1"
```

Set `TYPESAFE_API_KEY` in the environment before `Client::from_env()`.

## Call System One

```rust
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
                        [
                            ("calm", None),
                            ("frustrated", None),
                            ("angry", None),
                        ],
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

    let billing = response.noul("billing")?.noul;
    let tone = response.choice("tone")?.choice.as_str();
    let urgency = response.score("urgency")?.score;
    println!("{billing} {tone} {urgency}");
    Ok(())
}
```

Copy the same pattern from `examples/system_one.rs`.

## Build the client

Prefer `Client::from_env()` when the key is in the environment.

If you set the key in code, use the typestate builder:

```rust
let client = Client::builder().api_key("sk-...").build()?;
```

`Client::builder().build()` does not compile. Call `api_key` first.

`Client::new("sk-...")` is equivalent to `builder().api_key(...).build()`.

## Read answers

Question names are the lookup keys. Match the answer helper to the question type.

| Question | Lookup | Field |
| --- | --- | --- |
| `Question::noul` | `response.noul("name")?` | `.noul` (`f64`, yes probability) |
| `Question::choice` | `response.choice("name")?` | `.choice` (`String`) |
| `Question::score` | `response.score("name")?` | `.score` (`f64`) |

Scan mixed types through `response.answers`. There is no `nouls`, `choices`, or `scores` map.

## Blocking client

When the caller cannot be async:

```toml
typesafe-sdk = { version = "0.1", features = ["blocking"] }
```

```rust
use typesafe_sdk::blocking::Client;
use typesafe_sdk::Question;

fn main() -> Result<(), typesafe_sdk::Error> {
    let client = Client::from_env()?;
    let response = client.system_one("some text", [("q", Question::noul("Yes?"))])?;
    println!("{}", response.noul("q")?.noul);
    Ok(())
}
```

No `.await`. Same question and answer types as the async client.

## Failure checklist

Before you invent an API, check these.

1. Missing `TYPESAFE_API_KEY` makes `from_env` return `Error::Sdk` at runtime.
2. `builder().build()` without `api_key` is a compile error.
3. Async `Client` needs a Tokio runtime (`#[tokio::main]` or equivalent).
4. Blocking types live in `typesafe_sdk::blocking` and need `features = ["blocking"]`.
5. Do not port Python `result.nouls["name"]`. Use `response.noul("name")?`.
6. Answer helper and question type must match, or the lookup returns `Error::Sdk`.
7. Per-call overrides use `SystemOneOpts` / `ModelsOpts`, not ad hoc extra kwargs.

## Where else to look

- Human overview: `README.md`
- Compile-checked example: `examples/system_one.rs`
- API docs: https://docs.rs/typesafe-sdk
- Source of truth for types: `src/client.rs`, `src/question.rs`, `src/answer.rs`, `src/error.rs`
