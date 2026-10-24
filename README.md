# TypeSafe Rust SDK

Rust client for the [TypeSafe AI](https://typesafe.ai) API. It matches the Python `typesafe-sdk` 0.7.5 contract. Callers send named questions, optionally with images, and get typed answers.

Product overview: [TypeSafe docs](https://docs.typesafe.ai/).

## For AI agents

Read [`AGENTS.md`](AGENTS.md) before you write integration code. Copy from [`examples/system_one.rs`](examples/system_one.rs).

## Install

Requires Rust 1.85 or later. Edition 2024.

```toml
[dependencies]
typesafe-sdk = "0.4"
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
serde_json = "1"
```

This crate requires `serde_json = "1.0.134"`, a minimum rather than a pin: any 1.x from 1.0.134 up works, so `serde_json = "1"` or `cargo add serde_json` is fine. It enables `serde_json`'s `preserve_order` feature, which Cargo unifies into your build.

For non-async scripts, enable `blocking`:

```toml
typesafe-sdk = { version = "0.4", features = ["blocking"] }
```

The default `zstd` feature compresses large request bodies. It builds the C zstd library, so it needs a C compiler. To drop it, use `default-features = false`; bodies then go uncompressed.

Upgrading? See [`CHANGELOG.md`](CHANGELOG.md). 0.4 makes the HTTP stack pluggable and builds for `wasm32-wasip2`; with `default-features = false`, add the `reqwest` feature to keep the built-in client. 0.3 adds Cloudflare Clef and OpenAI Decisions providers, images, box questions, and request compression, and refuses a few questions and API keys it used to send. 0.2 moved to `reqwest` 0.12 and `http` 1.

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

`state` can be a string, a JSON object, or a JSON array. Questions are noul (yes or no probability), choice (one label), score (an ordered rubric), or box (where something is in an image). Answers use the same names.

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

The builder is typestated. `build()` exists only after `api_key(...)`, so a missing key is a compile error. `Client::from_env()` still fails at runtime when `TYPESAFE_API_KEY` is unset or blank. `build()` trims the key and refuses an empty one or one with anything other than printable ASCII without spaces. The error never contains the key, and `Debug` output shows it, and secret header values, as `***`.

Request bodies of 1024 bytes or more are compressed with zstd and sent with `Content-Encoding: zstd`. Each body is compressed once and reused on retries. `GET /v1/models` has no body. Turn compression off with `.compression(Compression::None)`, for example for a server that does not accept compressed bodies.

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

The default model name is `jev-latest`. Pass `model` when your server uses a different name. The default `reqwest` client uses rustls for TLS. It negotiates HTTP/2 with `https` servers that offer it and uses HTTP/1.1 otherwise. It takes proxies from `HTTP_PROXY`, `HTTPS_PROXY`, `ALL_PROXY`, and `NO_PROXY`, and on macOS and Windows falls back to the OS proxy settings when those are unset. It follows up to 10 redirects. Same-origin `307` keeps the POST body and `Authorization` header. For a custom CA, proxy, HTTP version, or redirect policy, pass your own `reqwest` 0.12 client to `http_client`. A clone of a client you already use shares its connection pool.

See [`examples/custom_base_url.rs`](examples/custom_base_url.rs). The mock mounts `/v1/systemone` on the wiremock origin. Live mode requires `TYPESAFE_BASE_URL` and does not fall back to `https://api.typesafe.ai`. The live branch calls `.model("jev-latest")`.

Per-call overrides go on `SystemOneOpts` (`model`, `timeout`, `retry`, `extra_headers`, `extra_body`, `assets`, `images`) or `ModelsOpts` (`timeout`, `retry`, `extra_headers`). `extra_body` is a shallow last-write-wins merge over `state`, `model`, and `questions`. An `assets` key in `extra_body` replaces the client's own, and the call then sends plain JSON without reading any image.

## WebAssembly and custom transports

The client builds every request (headers, zstd, multipart images), retries, and decodes the answers. A `Transport` only moves bytes and waits between retries. The default is `ReqwestTransport`, behind the `reqwest` feature. On `wasm32-wasip2` hosts such as Spin, reqwest and Tokio do not run, so turn the default features off and pass your own transport with `ClientBuilder::transport`:

```toml
typesafe-sdk = { version = "0.4", default-features = false }
```

```rust
use std::time::Duration;
use typesafe_sdk::transport::BoxFuture;
use typesafe_sdk::{Client, Compression, Error, Transport, TransportRequest, TransportResponse};

struct HostHttp;

impl Transport for HostHttp {
    fn send(&self, request: TransportRequest) -> BoxFuture<'_, Result<TransportResponse, Error>> {
        Box::pin(async move {
            // Send `request.method`, `request.url`, `request.headers` and
            // `request.body` with the host's outgoing HTTP. Return every status;
            // the client turns non-2xx into `Error::Api` and decides on retries.
            // Map a failure to `Error::Timeout { timeout: request.timeout }` or
            // `Error::Connection { message }` (credentials in it are masked).
            todo!()
        })
    }

    fn sleep(&self, duration: Duration) -> BoxFuture<'_, ()> {
        // The host's timer. Without one, return at once and disable retries.
        todo!()
    }
}

let client = Client::builder()
    .api_key("sk-...")
    .compression(Compression::None)
    .transport(HostHttp)
    .build()?;
```

`zstd` needs a C toolchain for the target, so leave it off on wasm and set `Compression::None`. On wasm, `BoxFuture` and `Transport` drop their `Send`/`Sync` bounds, because host HTTP futures are single-threaded. `Image::from_path` reads with `std::fs` there. CI checks `wasm32-wasip2` with and without the provider features. A transport also stands in for an HTTP stack in tests: see [`tests/transport.rs`](tests/transport.rs).

## Providers

One client serves three decision backends. The questions, the answers, and `system_one`/`system_one_opts` are the same on all of them; the client translates the request and the response.

| Provider | Feature | `Provider` value | Key | Default base URL | Default model |
| --- | --- | --- | --- | --- | --- |
| TypeSafe System One | always on | `Provider::TypeSafe` (default) | `TYPESAFE_API_KEY` | `https://api.typesafe.ai` | `jev-latest` |
| Cloudflare Clef | `clef` | `Provider::Clef { account_id }` | `CLOUDFLARE_API_TOKEN` (Workers AI) | `https://api.cloudflare.com/client/v4` | `clef` (or `clef-flash`) |
| OpenAI Decisions (experimental) | `openai-decisions` | `Provider::OpenAiDecisions` | `OPENAI_API_KEY` | `https://api.openai.com` | `gpt-6-luna` |

```toml
typesafe-sdk = { version = "0.4", features = ["clef", "openai-decisions"] }
```

```rust
use typesafe_sdk::{Client, Provider, Question};

let client = Client::builder()
    .api_key(std::env::var("CLOUDFLARE_API_TOKEN").unwrap())
    .provider(Provider::Clef { account_id: std::env::var("CLOUDFLARE_ACCOUNT_ID").unwrap() })
    .model("clef-flash")
    .build()?;
let response = client
    .system_one("I was charged twice.", [("billing", Question::noul("Is this about billing?"))])
    .await?;
println!("{}", response.noul("billing")?.noul);
```

`Client::from_env()` reads `TYPESAFE_PROVIDER`: `typesafe` (the default when unset), `clef`, or `openai`, in any case. It then reads that provider's key variable from the table. `clef` also needs `CLOUDFLARE_ACCOUNT_ID`. An unknown value, or a provider whose feature is off, is an `Error::Sdk` that names the value or the feature. `TYPESAFE_BASE_URL` and `TYPESAFE_DEFAULT_MODEL` apply to TypeSafe only. `base_url` and `model` on the builder work for every provider, for example to go through a gateway. The blocking builder has the same `provider` method. Key checks and redaction apply to every provider's key.

What goes on the wire:

- Clef posts the System One body unchanged, `model` included, to `{base}/accounts/{account_id}/ai/run/@cf/cloudflare/{model}`. The model is in the path and in the body because Workers AI requires both. The response's `{success, errors, result}` envelope is unwrapped. `success: false`, or any entry in `errors`, is an `Error::Api` whose message is Cloudflare's, such as `No route for that URI (code 7000)`. An HTTP error with that envelope gets the same message. A decode failure names the path under `result.`, such as `result.answers.q.noul`.
- OpenAI Decisions posts `{model, input, questions: [...]}` to `{base}/v1/decisions`. A noul is a `predicate`. A choice's criteria become `choices: [{value, description}]`, without `description` when it is `None`. A score's criteria become `levels: [{label, description}]` with labels `"0"`, `"1"`, and so on. Answers come back as the usual name-keyed map in question order. A score's `legend` is rebuilt from the criteria sent. The response has no model name or token counts, so `response.model` is the requested model and `usage` is empty.
- A refusal, which only OpenAI documents, is `Answer::Refusal`. `response.refusals()` lists them, and `response.noul(name)` and the other lookups fail with `The model refused to answer "name"`. `RefusalAnswer::raw` holds the object as received.
- Request bodies are never zstd-compressed for Clef or OpenAI. Retries, timeouts, and the `x-typesafe-*` headers work as they do for TypeSafe.

| Capability | TypeSafe | Clef | OpenAI Decisions |
| --- | --- | --- | --- |
| noul, choice, score | yes | yes | yes (noul as `predicate`) |
| Box questions | yes | no | no |
| Images (`images`, `assets`, attached images, `extra_body` `assets`) | yes | no | no |
| Structured (object or array) state, instructions, criteria | yes | yes | no, strings only |
| Noul criteria (yes and no meanings) | yes | yes | no |
| A question without instructions | yes | yes | no |
| Raw questions | sent as given | sent as given | only a plain noul, choice, or score |
| Questions per request | no limit in the SDK | 1 to 64 | no limit in the SDK |
| Question names | any | 1 to 100 of `[A-Za-z0-9_.-]` | any |
| A choice with one label | yes | no (Workers AI returns 422) | yes |
| `models()` | yes | no | no |
| Refusal answers | no | no | yes |
| zstd request bodies | yes | never | never |

Anything marked no fails with `Error::Unsupported { provider, feature }` before a request is sent, for example `The Clef provider does not support box questions (question "dog").` The client never converts a structured value to text or folds noul criteria into the instructions for you.

The OpenAI provider is experimental. The public guide is its only specification. It has no API reference, and the provider has not been checked against the live API. Error bodies, limits, usage, and whether a choice needs a `description` are unknown. Its shapes may change before the API is generally available.

To run Clef yourself, the Hugging Face model card describes an SGLang server with `POST /v1/systemone` and no Cloudflare envelope. That is plain System One: use `Provider::TypeSafe` with `base_url` pointing at your server, and the model name it serves.

See [`examples/providers.rs`](examples/providers.rs) (`cargo run --example providers --features "mock clef openai-decisions"`).

## Images

Wrap image file bytes or a path in `Image`. Its `Display` is a reference, `<image:` and 32 hex digits and `>`, that you put in any string of `state`, instructions, or criteria. Pass the image in `SystemOneOpts::images` so the call knows it. Or give it a name in `SystemOneOpts::assets` and write that name, such as `<photo>`, instead.

```rust
use typesafe_sdk::{Client, Image, Question, SystemOneOpts};

let client = Client::builder()
    .api_key("sk-...")
    .header("TypeSafe-Danger-Zone-Images", "true")?
    .model("jev-preview") // box questions: see below
    .build()?;
let photo = Image::from_path("photos/product.png");
let response = client
    .system_one_opts(
        format!("Listing photo: {photo}"),
        [("product", Question::box_("The product for sale"))],
        SystemOneOpts { images: vec![photo], ..SystemOneOpts::default() },
    )
    .await?;
let edges = &response.box_("product")?.box_;
println!("{} {} {} {}", edges.top, edges.bottom, edges.left, edges.right);
```

Image support on the API is opt-in. Send `TypeSafe-Danger-Zone-Images: true`, as above. The client does not add it. Images need a vision-language model. As of 2026-10-17, `https://api.typesafe.ai` answers box questions only with `jev-preview`; with `jev-latest` it returns `400 Invalid request.`, from this crate and the Python SDK alike. Noul, choice, and score questions about an image work with either model.

A request with images goes as `multipart/form-data`: the JSON body is the `request` part, with an `assets` object naming each image's part, and the image bytes follow. A file is read on the first request that sends it and kept after that. The SDK only checks that the data is not empty and labels PNG and JPEG by their signature. It sets no size limit. `Image::from_base64` decodes base64 text.

`client.attach_assets([image])` returns a guard. While it lives, any call on that client that contains the image's reference sends the image. `examples/images.rs` runs against the mock.

## Serve the System One shape

To be the server instead of the caller, use the types in `typesafe_sdk::wire`. They are the ones the client sends and decodes, and their `Serialize` and `Deserialize` match the wire JSON. Deserialize a request body into `SystemOneRequest`. Build the reply with `SystemOneResponse::new(model, usage, answers)` and the answer constructors, and the models listing with `ListModelsResponse::new`, the type `Client::models` returns. Mount your handlers at `wire::SYSTEM_ONE_PATH` and `wire::MODELS_PATH`. Questions of unknown type stay `Question::Raw` and unknown top-level keys stay in `SystemOneRequest::extra`, so a gateway can forward a request without dropping anything. The `wire` module docs list exactly what a deserialize-then-serialize round trip keeps. See the `wire` module docs and the "Serve the System One shape" section of [`AGENTS.md`](AGENTS.md).

## Errors and retries

`Error` is a sum type and `#[non_exhaustive]`, so a `match` needs a `_` arm. HTTP failures are `Error::Api` with a `kind` such as `BadRequest`, `Authentication`, or `RateLimited`. A request the provider cannot express is `Error::Unsupported`, returned before any request and never retried. A 200 body that does not match the schema is `ApiErrorKind::ResponseValidation` and names the field path. `response.decode_as::<T>()` decodes the raw body into your own `Deserialize` type and fails the same way. A connection error message has the request's credentials masked as `***`.

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
cargo test --no-default-features --features "mock blocking"   # without zstd
cargo test --features "mock blocking clef openai-decisions"    # all providers
cargo bench --bench client_overhead
```

`cargo bench` talks to in-process wiremock only. It does not call `https://api.typesafe.ai`.

Live API calls are not part of `cargo test`. Example integration tests in `tests/examples_integration.rs` use the same wiremock fixtures. For manual live checks, set `TYPESAFE_LIVE=1` when running an example.

CI runs `cargo fmt --check`, `cargo check`, `cargo clippy`, tests, and example builds. Run `./scripts/verify.sh` locally for the same matrix.

## Python parity

Behavior targets [typesafe-sdk-python](https://github.com/typesafe-ai/typesafe-sdk-python) 0.7.5 and its OpenAPI schema (`POST /v1/systemone`, `GET /v1/models`). The Rust crate has its own version line (0.4.0 here). It is not a version bump of the Python release.

Look up one answer with `noul("name")`, `choice("name")`, `score("name")`, or `box_("name")`. Python's `result.nouls["name"]` maps to that call. Scan mixed types through the public `answers` map. There is no `nouls` dict. `boxes()` iterates the box answers, like Python's `result.boxes`.

`RetryStatuses::Default` is the Python set `{408, 429, *range(500, 600)}` as a predicate. Pass `RetryStatuses::Custom(set)` to replace it. An empty custom set retries no HTTP status.

The wire matches Python byte for byte where Python pins it: the multipart layout and part names, the order of `assets`, the reference format, the 1024-byte compression threshold, the `Content-Encoding` header, and the validation messages for questions, assets, and API keys.

These differ from Python on purpose.

- An image goes in text by its reference. Rust `serde_json::Value` cannot hold an `Image`, so pass the image in `SystemOneOpts::images`, `SystemOneOpts::assets`, or `attach_assets`. Python can also place the `Image` object itself in the content.
- Providers are Rust only. The Python SDK talks to TypeSafe alone; `Provider::Clef` and `Provider::OpenAiDecisions` have no Python counterpart.
- No `Image.from_pil`. There is no Pillow. Encode the image yourself and use `Image::from_bytes`.
- Clones of a `Client` share attached images. A Python client cannot be cloned.
- `attach_assets` returns a guard that releases the images on drop, instead of a `with` block.
- `response_model` is `SystemOneResponse::decode_as::<T>()` on the response, not an argument to `system_one`. There is no lifting of answers into fields of a subclass.
- `TypeSafe-Danger-Zone-Images` is not sent by default, as in Python. Set it with `ClientBuilder::header`.
- Compression follows the `zstd` Cargo feature, which is on by default, instead of an optional Python extra. Requesting `Compression::Zstd` without the feature fails at `build`.
- Python 0.7.4 sends NaN and infinities as the strings `"NaN"`, `"Infinity"`, and `"-Infinity"`. Not applicable here: `serde_json::Value` cannot hold them, and `json!(f64::NAN)` is `null`.

These Python pieces stay out of the crate on purpose.

- Tenacity predicates. `Error` variants and `RetryStatuses` decide what retries.
- OpenAPI codegen. Two endpoints. Hand-written types plus `tests/contract.rs`.
- Live API tests in `cargo test`. Point `base_url` at a mock.
- `httpx.Response`. Use `raw_body()` and `request_id()`. `ApiError` keeps headers.
- Connection pool settings. `reqwest` already keeps idle connections for 90 seconds, longer than Python's 30.
