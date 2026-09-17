# Emit a compiling System One call

## Goal

Emit a compiling System One call.

## Add dependencies

Add these crates to `Cargo.toml`.

```toml
[dependencies]
typesafe-sdk = "0.1"
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
serde_json = "1"
```

## Set the API key

Export `TYPESAFE_API_KEY` before you run the program.

`Client::from_env` fails at runtime if the variable is unset or blank.

## Build the client

Prefer `Client::from_env`. Call `system_one`. Read answers with `noul`, `choice`, and `score`.

```rust
let client = Client::from_env()?;
```

If you pass the key in code, call `Client::builder().api_key(...).build()` only.

Never call `Client::builder().build()`. The builder is typestated. `build` exists only after `api_key`.

## Ask named questions

Copy this program. Then change the state and the question names.

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
```

Build the same program with `cargo build --example system_one`.

## Read answers by name

Call the helper that matches the question type.

- `response.noul("billing")` for a noul question
- `response.choice("tone")` for a choice question
- `response.score("urgency")` for a score question

The answer name must match the question name.

There is no `nouls` or `choices` map. Use `response.noul("name")`.

Scan mixed types through `response.answers`.

## Call from blocking code

If the program cannot be async, enable `blocking`. Use `typesafe_sdk::blocking::Client`. Do not `.await`.

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

## Fix common failures

Work these in order.

1. There is no `nouls` or `choices` dict. Use `response.noul("name")`, `response.choice("name")`, or `response.score("name")`.
2. Async code needs Tokio. Add `tokio` with `macros` and `rt-multi-thread`. Use `#[tokio::main]`.
3. Set `TYPESAFE_API_KEY` or pass `api_key` on the builder.
4. Answer names must match question names.
5. Call `api_key` before `build`. `Client::builder().build()` does not compile.
6. Enable `features = ["blocking"]` before you import `typesafe_sdk::blocking::Client`.

## Read more

- `README.md` for install, env defaults, retries, and Python parity
- https://docs.rs/typesafe-sdk
- `examples/system_one.rs` for the compile-checked program
- `src/*.rs` for the public API
