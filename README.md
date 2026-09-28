# TypeSafe Rust SDK

Rust client for the [TypeSafe AI](https://typesafe.ai) API. It matches the Python `typesafe-sdk` 0.6.0 contract. Callers send named questions and get typed answers.

Product overview: [TypeSafe docs](https://docs.typesafe.ai/).

## For AI agents

Read [`AGENTS.md`](AGENTS.md) before you write integration code. Copy from [`examples/system_one.rs`](examples/system_one.rs).

## Install

Requires Rust 1.85 or later. Edition 2024.

```toml
[dependencies]
typesafe-sdk = "0.2"
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
serde_json = "1"
```

This crate requires `serde_json = "1.0.134"`, a minimum rather than a pin: any 1.x from 1.0.134 up works, so `serde_json = "1"` or `cargo add serde_json` is fine. It enables `serde_json`'s `preserve_order` feature, which Cargo unifies into your build.

For non-async scripts, enable `blocking`:

```toml
typesafe-sdk = { version = "0.2", features = ["blocking"] }
```

Upgrading from 0.1? See [`CHANGELOG.md`](CHANGELOG.md). 0.2 moves to `reqwest` 0.12 and `http` 1.

## Call System One

Set `TYPESAFE_API_KEY` in your environment. Ask named questions about a piece of state. `TypeSafeClient` is an alias for `Client` if you are coming from the Python package.

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

`state` can be a string, a JSON object, or a JSON array. Questions are noul (yes or no probability), choice (one label), or score (an ordered rubric). Answers use the same names.

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

The builder is typestated. `build()` exists only after `api_key(...)`, so a missing key is a compile error. `Client::from_env()` still fails at runtime when `TYPESAFE_API_KEY` is unset or blank.

## Your own System One server

The default host is `https://api.typesafe.ai`. Point the same client at any server that implements `POST /v1/systemone` and `GET /v1/models`. Set the base URL to the origin or a path prefix, without those suffixes. The client appends them. A trailing slash is removed. `system_one` and `models` use the same base URL. `tests/contract.rs` locks the prefix pattern. It sets `{mock}/gateway/` and expects `/gateway/v1/systemone` and `/gateway/v1/models`.

Do not put `?` or `#` in the base URL. The client joins the path by string concatenation. Do not end the base URL with `/v1` or `/v1/`. That joins to `/v1/v1/systemone`.

```rust
let client = Client::builder()
    .api_key("local-key")
    .base_url("https://my-host.example/gateway")
    .model("local-model")
    .build()?;
```

`TYPESAFE_BASE_URL` does the same thing when you use `Client::from_env()` and do not call `base_url`. An explicit blank or non-http(s) `base_url` fails at `build`. Auth is `Authorization: Bearer <api_key>`. The client also sends `x-typesafe-sdk`, `x-typesafe-runtime`, and `user-agent`. Compatible servers can ignore those headers. `x-typesafe-request-id` is read when the response includes it.

The default model name is `jev-latest`. Pass `model` when your server uses a different name. TLS is rustls. For a custom CA, proxy, or redirect policy, pass your own `reqwest` 0.12 client to `http_client`. A clone of a client you already use shares its connection pool. The default `reqwest` client follows up to 10 redirects. Same-origin `307` keeps the POST body and `Authorization` header. It speaks HTTP/1.1 and takes proxies from `HTTP_PROXY`, `HTTPS_PROXY`, and `NO_PROXY`, not from OS settings. Enable `reqwest`'s `http2` or `system-proxy` feature in your own manifest to change that.

See [`examples/custom_base_url.rs`](examples/custom_base_url.rs). The mock mounts `/v1/systemone` on the wiremock origin. Live mode requires `TYPESAFE_BASE_URL` and does not fall back to `https://api.typesafe.ai`. The live branch calls `.model("jev-latest")`.

Per-call overrides go on `SystemOneOpts` (`model`, `timeout`, `retry`, `extra_headers`, `extra_body`) or `ModelsOpts` (`timeout`, `retry`, `extra_headers`). `extra_body` is a shallow last-write-wins merge over `state`, `model`, and `questions`.

## Serve the System One shape

To be the server instead of the caller, use the types in `typesafe_sdk::wire`. They are the ones the client sends and decodes, and their `Serialize` and `Deserialize` match the wire JSON. Deserialize a request body into `SystemOneRequest`. Build the reply with `SystemOneResponse::new(model, usage, answers)` and the answer constructors, and the models listing with `ModelsResponse::new`. Mount your handlers at `wire::SYSTEM_ONE_PATH` and `wire::MODELS_PATH`. Questions of unknown type stay `Question::Raw` and unknown top-level keys stay in `SystemOneRequest::extra`, so a gateway can forward a request unchanged. See the `wire` module docs and the "Serve the System One shape" section of [`AGENTS.md`](AGENTS.md).

## Errors and retries

`Error` is a sum type. HTTP failures are `Error::Api` with a `kind` such as `BadRequest`, `Authentication`, or `RateLimited`. A 200 body that does not match the schema is `ApiErrorKind::ResponseValidation` and names the field path.

Default retries: 2 after the first attempt. Statuses 408, 429, and 5xx. Exponential backoff from 0.5s to 5s with 0.25 jitter. A 30s budget. Honor `Retry-After` and `retry-after-ms`.

## Cookbook examples

Pattern and cookbook samples live under `examples/`. Each example runs against wiremock by default or the live API when `TYPESAFE_LIVE=1`.

```bash
cargo run --example fan_out --features mock
TYPESAFE_LIVE=1 cargo run --example fan_out   # needs TYPESAFE_API_KEY
```

See [examples/README.md](examples/README.md) for the catalog and doc links.

## Run tests

```bash
cargo test --features mock
cargo test --features "mock blocking"
cargo bench --bench client_overhead
```

`cargo bench` talks to in-process wiremock only. It does not call `https://api.typesafe.ai`.

Live API calls are not part of `cargo test`. Example integration tests in `tests/examples_integration.rs` use the same wiremock fixtures. For manual live checks, set `TYPESAFE_LIVE=1` when running an example.

CI runs `cargo fmt --check`, `cargo check`, `cargo clippy`, tests, and example builds. Run `./scripts/verify.sh` locally for the same matrix.

## Python parity

Behavior targets [typesafe-sdk-python](https://github.com/typesafe-ai/typesafe-sdk-python) 0.6.0 and OpenAPI 0.2.0 (`POST /v1/systemone`, `GET /v1/models`). The Rust crate has its own version line (0.2.0 here). It is not a version bump of the Python release.

Look up one answer with `noul("name")`, `choice("name")`, or `score("name")`. Python's `result.nouls["name"]` maps to that call. Scan mixed types through the public `answers` map. There is no `nouls` dict.

`RetryStatuses::Default` is the Python set `{408, 429, *range(500, 600)}` as a predicate. Pass `RetryStatuses::Custom(set)` to replace it. An empty custom set retries no HTTP status.

These Python pieces stay out of the crate on purpose.

- Tenacity predicates. `Error` variants and `RetryStatuses` decide what retries.
- OpenAPI codegen. Two endpoints. Hand-written types plus `tests/contract.rs`.
- Live API tests in `cargo test`. Point `base_url` at a mock.
- `httpx.Response`. Use `raw_body()` and `request_id()`. `ApiError` keeps headers.
