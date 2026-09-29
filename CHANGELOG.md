# Changelog

## 0.2.0 (unreleased)

The types the client sends and decodes now serve the System One shape in both directions. The HTTP stack moves to `reqwest` 0.12 and `http` 1, so a caller on that stack can share its client instead of building a second one.

### Breaking

- `http` 0.2 → 1 and `reqwest` 0.11 → 0.12. `ClientBuilder::http_client` takes a `reqwest` 0.12 `Client`. `ClientBuilder::headers`, `blocking::Builder::headers`, `SystemOneOpts::extra_headers`, `ModelsOpts::extra_headers`, and `ApiError::headers` use the `http` 1 `HeaderMap`, the same type as `reqwest::header::HeaderMap`. Code that names it needs `http = "1"` or can import it from `reqwest::header`.
- `NoulCriteria` serializes with the wire keys `true` and `false`, not the field names `true_meaning` and `false_meaning`. `Question` output is unchanged.

### Changed

- `reqwest` 0.12 moved HTTP/2, the macOS/Windows system proxy, and charset decoding behind features. This crate enables `http2`, `system-proxy` and `charset` alongside `json` and `rustls-tls`, so the built-in client keeps all three, as it did on 0.1.x. It still uses rustls, not the platform TLS that `reqwest`'s own defaults would pull in.
- The client builds its request body as a `SystemOneRequest`. The bytes on the wire are unchanged. `request_body_bytes_are_stable` in `tests/contract.rs` pins them to 0.1.2's output.
- The `mock` feature and the tests use `wiremock` 0.6.

### Added

- `wire` module: `SystemOneRequest`, `ModelsResponse`, and the paths `SYSTEM_ONE_PATH` and `MODELS_PATH`. It also re-exports `Question`, `NoulCriteria`, `JsonContent`, `SystemOneResponse`, `Answer`, `NoulAnswer`, `ChoiceAnswer`, `ScoreAnswer`, `Usage`, and `ModelMetadata`. `SystemOneRequest` and `ModelsResponse` are also at the crate root.
- `Serialize` and `Deserialize` for those types, matching the wire JSON. A question deserializes to `Noul`, `Choice`, or `Score` when that variant holds it exactly, and to `Raw` otherwise. Deserializing rejects any question the client would refuse to send. `SystemOneResponse` deserializes with the client's decoder, which skips an answer of unknown type.
- Constructors: `SystemOneResponse::new`, `NoulAnswer::new`, `ChoiceAnswer::new`, `ScoreAnswer::new`, `Usage::new`, `ModelMetadata::new`, and `ModelsResponse::new`.

### Fixed

- `README.md` and `AGENTS.md` said `serde_json` was pinned to `=1.0.134`. `Cargo.toml` requires `serde_json = "1.0.134"`, which accepts any 1.x from 1.0.134 up. The docs now say so.

### MSRV

Still Rust 1.85. On 1.85.0, with a lockfile resolved for 1.85, the library builds and every test passes. Two transitive crates need a newer compiler but declare no `rust-version`, so the resolver cannot steer around them. `yoke-derive` 0.8.3 (via `url`) needs 1.87, and 0.1.2 was already affected. `wiremock` 0.6.5 (tests and the `mock` feature) needs 1.88. On 1.85, run `cargo update -p yoke-derive --precise 0.8.2` and `cargo update -p wiremock --precise 0.6.4`.

## 0.1.2 and earlier

See the git history.
