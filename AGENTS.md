# Call System One from Rust

## Dependencies

Add these crates to `Cargo.toml`.

```toml
[dependencies]
typesafe-sdk = "0.1"
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
serde_json = "1"
```

Write `serde_json = "1"` yourself. Do not run `cargo add serde_json` if that selects a newer patch. This crate pins `serde_json = "=1.0.134"`.

## API key

Export `TYPESAFE_API_KEY` before you run the program.

`Client::from_env` returns `Error::Sdk` when the variable is unset or blank.

## Client

Use `Client::from_env` when the key is in the environment.

```rust
let client = Client::from_env()?;
```

To pass the key in code, call `Client::builder().api_key(...).build()`.

`Client::builder().build()` does not compile. The builder is typestated. `build` exists only after `api_key`.

## Another System One server

The default base URL is `https://api.typesafe.ai`. To call your own server with the same `POST /v1/systemone` and `GET /v1/models` contract, set the base URL to the origin or a path prefix. Do not append those paths yourself. `tests/contract.rs` (`custom_base_url_serves_system_one_and_models`) sets `{mock}/gateway/` and the client posts to `/gateway/v1/systemone`.

```rust
let client = Client::builder()
    .api_key("local-key")
    .base_url("https://my-host.example/gateway")
    .model("your-model")
    .build()?;
```

`Client::from_env()` reads `TYPESAFE_BASE_URL` when `base_url` is omitted. A trailing slash is stripped. `models()` uses that same base URL. Copy `examples/custom_base_url.rs`. Mock mounts `/v1/systemone` on the wiremock origin (`cargo run --example custom_base_url --features mock`). Live requires `TYPESAFE_BASE_URL` and does not fall back to `https://api.typesafe.ai`. The live branch sets `.model("jev-latest")`.

## Ask questions

Copy this program. Change the state and the question names for your task.

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

Check the same program with `cargo build --example system_one --features mock`.

## Cookbook examples

See `examples/README.md` for the full list (fan-out, confidence routing, guardrails, and others).

Mock run (no API key):

```bash
cargo run --example fan_out --features mock
```

Live run against your account:

```bash
export TYPESAFE_API_KEY=...
export TYPESAFE_LIVE=1
cargo run --example fan_out
```

`TYPESAFE_LIVE=1` selects the real API. Without it, examples need `--features mock` and use wiremock fixtures in `examples/support/fixtures.rs`.

## Read answers

Use the helper that matches the question type.

- `response.noul("billing")?.noul` is an `f64` yes probability from 0.0 to 1.0. It is not a `bool`.
- `response.choice("tone")?.choice` is the selected label as a `String`.
- `response.score("urgency")?.score` is an `f64` rubric value. It is not a legend index.

The answer name must match the question name.

There is no `nouls` or `choices` map. Call `response.noul("name")`.

Scan mixed types through `response.answers`.

Choice descriptions are `Option<JsonContent>`. Write `Some("Calm".into())`, not `Some("Calm")`.

## Blocking client

If the program cannot be async, enable `blocking` and use `typesafe_sdk::blocking::Client`. Do not `.await`.

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

## Common failures

1. There is no `nouls` or `choices` dict. Use `response.noul("name")`, `response.choice("name")`, or `response.score("name")`.
2. Async code needs Tokio with `macros` and `rt-multi-thread`, plus `#[tokio::main]`.
3. Set `TYPESAFE_API_KEY` or pass `api_key` on the builder.
4. Answer names must match question names.
5. Call `api_key` before `build`. `Client::builder().build()` does not compile.
6. Enable `features = ["blocking"]` before you import `typesafe_sdk::blocking::Client`.
7. Do not call `blocking::Client` inside an existing Tokio runtime. It panics.
8. Treat `noul` as a probability (`f64`), not a boolean.
9. Pin consumer `serde_json` as `"1"`. A newer exact version conflicts with the crate pin.
10. `base_url` is an origin or a path prefix, not the full `/v1/systemone` path. The client appends `/v1/systemone` and `/v1/models`. A prefix such as `…/gateway` is valid. See `custom_base_url_serves_system_one_and_models` in `tests/contract.rs`.
11. Do not put `?` or `#` in the base URL. The client joins the path with string concatenation, so a query or fragment is not a prefix.
12. Do not end the base URL with `/v1` or `/v1/`. That joins to `/v1/v1/systemone`.

## More reading

- `README.md` for install, env defaults, retries, and Python parity
- https://docs.rs/typesafe-sdk
- `examples/system_one.rs` for the compile-checked program
- `src/*.rs` for the public API
