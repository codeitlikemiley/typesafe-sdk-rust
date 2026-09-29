# Changelog

## 0.2.0 (unreleased)

The types the client sends and decodes now serve the System One shape in both directions. The HTTP stack moves to `reqwest` 0.12 and `http` 1, so a caller on that stack can share its client instead of building a second one.

### Breaking

- `http` 0.2 → 1 and `reqwest` 0.11 → 0.12. `ClientBuilder::http_client` takes a `reqwest` 0.12 `Client`. `ClientBuilder::headers`, `blocking::Builder::headers`, `SystemOneOpts::extra_headers`, `ModelsOpts::extra_headers`, and `ApiError::headers` use the `http` 1 `HeaderMap`, the same type as `reqwest::header::HeaderMap`. Code that names it needs `http = "1"` or can import it from `reqwest::header`.
- `NoulCriteria` serializes with the wire keys `true` and `false`, not the field names `true_meaning` and `false_meaning`. `Question` output is unchanged.
- `ScoreAnswer::legend` and `ScoreAnswer::probabilities` are `IndexMap<u32, _>`, not `BTreeMap<u32, _>`. They keep the order the server sent, or the order given to `ScoreAnswer::new`, and serialize in that order instead of sorted by index. Lookups such as `legend[&0]` and `probabilities.get(&2)` compile and behave as before. Iteration follows wire order. `legend[0]` now also compiles, as `IndexMap`'s positional index. Keys are still `u32`, so a non-canonical key such as `"01"` reads as index 1 and serializes as `"1"`.
- `Deserialize` for `Usage`, `ModelMetadata`, `NoulAnswer`, and `ChoiceAnswer` was derived in 0.1.2. It now runs the decoder `Client` uses, so serde and the client read a body the same way. A repeated key takes its last value instead of failing. A token count above `i64::MAX` wraps to a negative number instead of failing, as it always has in the client. An error names the field, such as `Invalid usage data at 'input_tokens'.` The input format must be self-describing, as JSON is.

### Changed

- `reqwest` 0.12 moved HTTP/2, the macOS/Windows system proxy, and charset decoding behind features. This crate enables `http2`, `system-proxy` and `charset` alongside `json` and `rustls-tls`, so the built-in client keeps all three, as it did on 0.1.x. It still uses rustls, not the platform TLS that `reqwest`'s own defaults would pull in.
- The client builds its request body as a `SystemOneRequest`. The bytes on the wire are unchanged. `request_body_bytes_are_stable` in `tests/contract.rs` pins them to 0.1.2's output.
- The `mock` feature and the tests use `wiremock` 0.6.

### Added

- `wire` module: `SystemOneRequest` and the paths `SYSTEM_ONE_PATH` and `MODELS_PATH`. It also re-exports `Question`, `NoulCriteria`, `JsonContent`, `SystemOneResponse`, `ListModelsResponse`, `Answer`, `NoulAnswer`, `ChoiceAnswer`, `ScoreAnswer`, `Usage`, and `ModelMetadata`. `SystemOneRequest` is also at the crate root.
- `Serialize` and `Deserialize` for those types, matching the wire JSON. A question deserializes to `Noul`, `Choice`, or `Score` only when that variant serializes back to the same bytes, key order included, and to `Raw` otherwise. Deserializing rejects any question the client would refuse to send.
- One decoder per response type. `SystemOneResponse`, `ListModelsResponse`, `Answer`, the three answer structs, `Usage`, and `ModelMetadata` deserialize with the code `Client` decodes a response with, so both accept the same input, give the same values, and name the same field on error. Inside a `SystemOneResponse`, an answer of unknown type is skipped as the client skips it.
- Round trips, as the `wire` module docs list them. Serializing writes compact JSON. Beyond that, a `Question` always serializes back to the bytes it was read from. A `SystemOneRequest` does when its keys start with `state`, then `model` if present, then `questions`. Otherwise its known keys move to the front, and a `null` `model` is left out. Response types keep only what the client decodes. Unknown keys, `null` token counts, and answers of unknown type are dropped. Fields come back in declaration order, and `1` in an `f64` field comes back as `1.0`.
- Constructors: `SystemOneResponse::new`, `ListModelsResponse::new`, `NoulAnswer::new`, `ChoiceAnswer::new`, `ScoreAnswer::new`, `Usage::new`, and `ModelMetadata::new`.

### Fixed

- `README.md` and `AGENTS.md` said `serde_json` was pinned to `=1.0.134`. `Cargo.toml` requires `serde_json = "1.0.134"`, which accepts any 1.x from 1.0.134 up. The docs now say so.

### MSRV

Still Rust 1.85. On 1.85.0, with a lockfile resolved for 1.85, the library builds and every test passes. Two transitive crates need a newer compiler but declare no `rust-version`, so the resolver cannot steer around them. `yoke-derive` 0.8.3 (via `url`) needs 1.87, and 0.1.2 was already affected. `wiremock` 0.6.5 (tests and the `mock` feature) needs 1.88. On 1.85, run `cargo update -p yoke-derive --precise 0.8.2` and `cargo update -p wiremock --precise 0.6.4`.

## 0.1.2 and earlier

See the git history.
