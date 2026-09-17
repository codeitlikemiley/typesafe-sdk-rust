# TypeSafe Rust SDK

Rust client for the [TypeSafe AI](https://typesafe.ai) API. It rebuilds the Python `typesafe-sdk` 0.6.0 contract for Rust callers: named questions in, typed answers out.

Learn what TypeSafe is in the [TypeSafe docs](https://docs.typesafe.ai/).

## For AI agents

Read [`AGENTS.md`](AGENTS.md) before writing integration code. Copy from [`examples/system_one.rs`](examples/system_one.rs).

## Install

Requires Rust 1.85 or later. Edition 2024.

```toml
[dependencies]
typesafe-sdk = "0.1"
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
serde_json = "1"
```

Write `serde_json = "1"` by hand. Do not let `cargo add` pick a newer patch that fights this crate's `serde_json = "=1.0.134"` pin.

For scripts that should not be async, enable `blocking`:

```toml
typesafe-sdk = { version = "0.1", features = ["blocking"] }
```

## Call System One

Set `TYPESAFE_API_KEY` in your environment, then ask named questions about a piece of state. `TypeSafeClient` is an alias for `Client` if you are coming from the Python package.

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
                    Question::score("How urgent is this ticket?", ["can wait", "this week", "today"]),
                ),
            ],
        )
        .await?;

    println!("{}", response.noul("billing")?.noul);
    println!("{}", response.choice("tone")?.choice);
    println!("{}", response.score("urgency")?.score);
    Ok(())
}
```

`state` can be a string, a JSON object, or a JSON array. Questions are noul (yes or no), choice (one label), or score (an ordered rubric). Answers use the same names.

List models with `client.models().await?`.

## Configure the client

Constructor values win over environment variables. Whitespace-only environment values are ignored.

| Setting | Environment | Default |
| --- | --- | --- |
| API key | `TYPESAFE_API_KEY` | required |
| Base URL | `TYPESAFE_BASE_URL` | `https://api.typesafe.ai` |
| Model | `TYPESAFE_DEFAULT_MODEL` | `jev-latest` |
| Timeout | - | 10 seconds per attempt |
| Log level | `TYPESAFE_LOG_LEVEL` | unset. Values are `debug`, `info`, `warn`, `warning`, `error`, `off`. Secret headers are redacted. |

```rust
use std::time::Duration;
use typesafe_sdk::{Client, RetryPolicy};

let client = Client::builder()
    .api_key("sk-...")
    .model("jev-latest")
    .timeout(Duration::from_secs(20))
    .retry(RetryPolicy { max_retries: 0, ..RetryPolicy::default() })
    .build()?;
```

The builder is typestated: `build()` only exists after `api_key(...)`, so a missing key is a compile error. `Client::from_env()` still fails at runtime when `TYPESAFE_API_KEY` is unset or blank.

Per-call overrides go on `SystemOneOpts` (`model`, `timeout`, `retry`, `extra_headers`, `extra_body`) or `ModelsOpts` (`timeout`, `retry`, `extra_headers`). `extra_body` is a shallow last-write-wins merge over `state`, `model`, and `questions`.

## Errors and retries

`Error` is a sum type. HTTP failures are `Error::Api` with a `kind` (`BadRequest`, `Authentication`, `RateLimited`, and the rest). A 200 body that does not match the schema is `ApiErrorKind::ResponseValidation` and names the field path.

Default retries: 2 after the first attempt, statuses 408, 429, and 5xx, exponential backoff from 0.5s to 5s with 0.25 jitter, a 30s budget, and honor `Retry-After` / `retry-after-ms`.

## Run tests

```bash
cargo test
```

Integration against the live API is not part of `cargo test`. Point `Client::builder().base_url(...)` at a mock, or set `TYPESAFE_API_KEY` and call the real host from your own binary.

## Python parity

Behavior targets [typesafe-sdk-python](https://github.com/typesafe-ai/typesafe-sdk-python) 0.6.0 and OpenAPI 0.2.0 (`POST /v1/systemone`, `GET /v1/models`). The Rust crate is 0.1.0 because it is a new package, not a version bump of the Python release.

Look up one answer with `noul("name")`, `choice("name")`, or `score("name")`. Python's `result.nouls["name"]` maps to that call. Scan mixed types through the public `answers` map. There is no `nouls` dict.

`RetryStatuses::Default` is the Python set `{408, 429, *range(500, 600)}` as a predicate. Pass `RetryStatuses::Custom(set)` to replace it. An empty custom set retries no HTTP status.

These Python pieces stay out of the crate on purpose.

- Tenacity predicates. `Error` variants and `RetryStatuses` decide what retries.
- OpenAPI codegen. Two endpoints. Hand-written types plus `tests/contract.rs`.
- Live API tests in `cargo test`. Point `base_url` at a mock.
- `httpx.Response`. Use `raw_body()` and `request_id()`. `ApiError` keeps headers.
