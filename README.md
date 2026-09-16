# TypeSafe AI Rust SDK

Rust SDK for [TypeSafe AI](https://typesafe.ai). This crate mirrors the public API of the [Python SDK](https://github.com/typesafe-ai/typesafe-sdk-python).

## Quickstart

Add the dependency:

```toml
[dependencies]
typesafe-sdk = "0.6"
```

Set `TYPESAFE_API_KEY`, then call System One:

```rust
use std::collections::HashMap;
use serde_json::json;
use typesafe_sdk::{Choice, Question, Questions, TypeSafeClient};

let client = TypeSafeClient::from_env()?;
let mut questions = Questions::new();
questions.insert(
    "category".into(),
    Question::Choice(Choice::new(
        Some(json!("What is this ticket about?")),
        HashMap::from([
            ("billing".into(), None),
            ("technical".into(), None),
            ("other".into(), None),
        ]),
    )),
);

let response = client.system_one(
    json!({"document": "I was charged twice. Please fix this ASAP."}),
    &questions,
    None,
    None,
    None,
    None,
    None,
)?;

println!("{}", response.choices().get("category").unwrap().choice);
```

Learn what TypeSafe is and how to use it in the [TypeSafe docs](https://docs.typesafe.ai/).

## Features

- Blocking client (`TypeSafeClient`) and async client (`AsyncTypeSafeClient`)
- System One (`system_one`) and model listing (`models().list()`)
- Question primitives: `Noul`, `Choice`, `Score`, plus raw JSON questions
- Config from environment (`TYPESAFE_API_KEY`, `TYPESAFE_BASE_URL`, `TYPESAFE_DEFAULT_MODEL`)
- Retries with backoff, jitter, and `Retry-After` header support
- Typed HTTP errors matching the Python SDK

## Development

```bash
cargo test
```

Integration tests use [wiremock](https://github.com/LukeMathWalker/wiremock-rs) and do not call the live API.

## License

MIT
