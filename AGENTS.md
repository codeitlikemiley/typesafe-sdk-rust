# Call System One from Rust

## Dependencies

Add these crates to `Cargo.toml`.

```toml
[dependencies]
typesafe-sdk = "0.2"
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
serde_json = "1"
```

This crate requires `serde_json = "1.0.134"`. That is a minimum, not a pin: any 1.x from 1.0.134 up satisfies it. `serde_json = "1"` and `cargo add serde_json` both resolve to such a version. The crate enables `serde_json`'s `preserve_order` feature. Cargo unifies features, so your `serde_json::Map` keeps insertion order too.

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
typesafe-sdk = { version = "0.2", features = ["blocking"] }
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

## Serve the System One shape

To implement `POST /v1/systemone` and `GET /v1/models` yourself, use `typesafe_sdk::wire`. It holds the same types `Client` sends and decodes. Their `Serialize` and `Deserialize` match the wire JSON.

- Mount handlers at `wire::SYSTEM_ONE_PATH` and `wire::MODELS_PATH` under your base URL.
- Deserialize the body into `SystemOneRequest` (`state`, `model`, `questions`, `extra`). Unknown top-level keys land in `extra`. Deserialize rejects what the client refuses to send: no `state`, no questions, a question without a nonempty string `type`, `choice` or `score` without `criteria`, or `score` with no scores.
- A question becomes `Question::Noul`, `Choice`, or `Score` only when that variant serializes back to the same bytes, key order included. Anything else, such as an unknown type, an extra key, or `instructions` before `type`, stays `Question::Raw`. Either way it serializes back with the same keys in the same order.
- Build the reply with `SystemOneResponse::new(model, usage, answers)`. Answers are `Answer::Noul(NoulAnswer::new(p))`, `Answer::Choice(ChoiceAnswer::new(label, confidence, probabilities))`, and `Answer::Score(ScoreAnswer::new(score, confidence, legend, probabilities))`.
- Serve models with `ListModelsResponse::new([ModelMetadata::new(name, description, release_date)])`. It is the type `client.models()` returns.
- Response types deserialize with the client's own decoder. `SystemOneResponse`, `ListModelsResponse`, `Answer`, the answer structs, `Usage`, and `ModelMetadata` accept what `Client` accepts and name the same field when they fail. They keep only what the client decodes, so unknown keys, `null` token counts, and answers of unknown type do not survive a round trip. The `wire` module docs list every difference.

```rust
use typesafe_sdk::wire::{Answer, NoulAnswer, SystemOneRequest, SystemOneResponse, Usage};

fn handle(body: &[u8]) -> Result<Vec<u8>, serde_json::Error> {
    let request: SystemOneRequest = serde_json::from_slice(body)?;
    let answers = request
        .questions
        .keys()
        .map(|name| (name.clone(), Answer::Noul(NoulAnswer::new(0.5))));
    let model = request.model.as_deref().unwrap_or("jev-latest");
    let response = SystemOneResponse::new(model, Usage::new(Some(12), Some(1)), answers);
    serde_json::to_vec(&response)
}
```

To forward a request upstream, pass `request.state`, `request.questions`, and `SystemOneOpts { model: request.model, extra_body: Some(request.extra), .. }` to `system_one_opts`. If the request is compact JSON that names a `model` and starts with the keys `state`, `model`, `questions`, the client then sends the bytes it received. Without a `model`, the client sends its default model. `gateway_forwards_a_request_byte_for_byte` in `tests/wire.rs` checks that. A response you build or deserialize has no `request_id()` and an empty `raw_body()`. The compile-checked example is the module doc in `src/wire.rs`.

## Common failures

1. There is no `nouls` or `choices` dict. Use `response.noul("name")`, `response.choice("name")`, or `response.score("name")`.
2. Async code needs Tokio with `macros` and `rt-multi-thread`, plus `#[tokio::main]`.
3. Set `TYPESAFE_API_KEY` or pass `api_key` on the builder.
4. Answer names must match question names.
5. Call `api_key` before `build`. `Client::builder().build()` does not compile.
6. Enable `features = ["blocking"]` before you import `typesafe_sdk::blocking::Client`.
7. Do not call `blocking::Client` inside an existing Tokio runtime. It panics.
8. Treat `noul` as a probability (`f64`), not a boolean.
9. `serde_json` must resolve to 1.0.134 or later. An exact pin below that, such as `=1.0.100`, cannot resolve.
10. `base_url` is an origin or a path prefix, not the full `/v1/systemone` path. The client appends `/v1/systemone` and `/v1/models`. A prefix such as `…/gateway` is valid. See `custom_base_url_serves_system_one_and_models` in `tests/contract.rs`.
11. Do not put `?` or `#` in the base URL. The client joins the path with string concatenation, so a query or fragment is not a prefix.
12. Do not end the base URL with `/v1` or `/v1/`. That joins to `/v1/v1/systemone`.

## More reading

- `README.md` for install, env defaults, retries, and Python parity
- https://docs.rs/typesafe-sdk
- `examples/system_one.rs` for the compile-checked program
- `src/wire.rs` for the server-side wire types
- `CHANGELOG.md` for what changed in each release
- `src/*.rs` for the public API
