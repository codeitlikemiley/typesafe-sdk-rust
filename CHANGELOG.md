# Changelog

## 0.3.0 (unreleased)

Parity with Python `typesafe-sdk` 0.7.5: image assets, box questions, zstd request compression, the request and API key checks of 0.7.1 and 0.7.3, and a typed decode of the raw response for 0.7.0's `response_model`.

### Breaking

- `Question` and `Answer` are `#[non_exhaustive]` and have a new `Box` variant. A `match` on either needs a `_` arm.
- The client refuses, before any request, a noul with neither instructions nor a non-null meaning, such as `Question::noul_bare()` alone or `{"type":"noul"}`; a raw noul whose `criteria` is not an object; and a choice with no labels. `SystemOneRequest` deserialize refuses the same. The messages are Python's, such as `Noul question "q" has neither instructions nor criteria; at least one is required.`
- A raw `choice` or `score` whose `criteria` is `null`, `false`, `0`, `""`, `[]`, or `{}` is refused, as in Python. 0.2 refused only an empty score array.
- `build` trims the API key and refuses a key that is empty after trimming, or that has whitespace, control characters, or non-ASCII characters inside it: `API key must contain only printable ASCII characters without whitespace.` 0.2 sent such a key and failed later with `invalid header value`. This applies to `ClientBuilder::build`, `Client::from_env`, `Client::new`, and the blocking builder.
- With the default features, request bodies of 1024 bytes or more go zstd-compressed with `Content-Encoding: zstd`, as Python does by default. A server that does not accept that needs `.compression(Compression::None)` on the builder, or the crate with `default-features = false`.
- `SystemOneOpts` has the new public fields `assets` and `images`. A struct literal without `..SystemOneOpts::default()` no longer compiles.

### Changed

- `Debug` for `Client`, `ClientBuilder`, `SystemOneOpts`, and `ModelsOpts` shows the API key and the values of secret headers (`authorization`, `x-api-key`, cookies, and any name containing `token` or `secret`) as `***`. 0.2 derived `Debug` and printed the key.
- An `Error::Connection` message has the request's credentials replaced with `***`: secret header values, the token after `Bearer`, and their escaped forms.
- `SystemOneRequest` docs and the `wire` module docs list the new checks and describe a multipart image request.

### Added

- `Image`: an image from `Image::from_bytes`, `Image::from_path`, or `Image::from_base64`. Its `Display` and `reference()` give `<image:{32 lowercase hex}>`. Clones are the same asset; two images from the same bytes are not. A path is read on the first request that sends it and kept. PNG and JPEG are labeled by signature, anything else is `application/octet-stream`. There is no size limit and no Pillow support.
- `SystemOneOpts::assets` (alias to image, all sent) and `SystemOneOpts::images` (sent when their reference appears). A request with images is `multipart/form-data`, byte for byte as Python writes it: the JSON as the `request` part with an `assets` object last, then one `asset-N` part per key. Aliases come first in order, then references in order of first appearance. An `assets` key in `extra_body` sends plain JSON without reading any image. Errors name the alias, with Python's messages.
- `Client::attach_assets` and `blocking::Client::attach_assets` return an `AttachGuard`. Attached images are sent by any call on that client, and on its clones, that references them. Holds are counted and released on drop.
- `Question::Box`, `Question::box_`, `Question::box_bare`. `Answer::Box`, `BoxAnswer`, `BoxCoordinates`, `Answer::as_box`, `SystemOneResponse::box_`, and `SystemOneResponse::boxes`. A box answer's `confidence` key is required and may be `null`. All are in `wire` and at the crate root.
- `Compression` (`Zstd`, `None`), `ClientBuilder::compression`, and `blocking::Builder::compression`. The `zstd` feature, on by default, adds the `zstd` 0.13 dependency, which builds the C library. The body is compressed once and reused on every retry. `Compression::Zstd` without the feature fails at `build`.
- `SystemOneResponse::decode_as::<T>()` decodes the raw body into your own type. A failure is `ApiErrorKind::ResponseValidation` with the failing field path, such as `answers.spam.noul`.
- `examples/images.rs`, run against the mock in `tests/examples_integration.rs`.
- New dependencies: `base64` 0.22, `serde_path_to_error` 0.1, optional `zstd` 0.13, and `tokio`'s `fs` feature.

### Fixed

- The 0.2.0 heading of this changelog said "unreleased". 0.2.0 is on crates.io.

### MSRV

Still Rust 1.85. The new dependencies build on 1.85.0. The 0.2.0 notes on `yoke-derive` and `wiremock` still apply.

## 0.2.0

The types the client sends and decodes now serve the System One shape in both directions. The HTTP stack moves to `reqwest` 0.12 and `http` 1, so a caller on that stack can share its client instead of building a second one.

### Breaking

- `http` 0.2 → 1 and `reqwest` 0.11 → 0.12. `ClientBuilder::http_client` takes a `reqwest` 0.12 `Client`. `ClientBuilder::headers`, `blocking::Builder::headers`, `SystemOneOpts::extra_headers`, `ModelsOpts::extra_headers`, and `ApiError::headers` use the `http` 1 `HeaderMap`, the same type as `reqwest::header::HeaderMap`. Code that names it needs `http = "1"` or can import it from `reqwest::header`.
- `NoulCriteria` serializes with the wire keys `true` and `false`, not the field names `true_meaning` and `false_meaning`. `Question` output is unchanged.
- `ScoreAnswer::legend` and `ScoreAnswer::probabilities` are `IndexMap<u32, _>`, not `BTreeMap<u32, _>`. They keep the order the server sent, or the order given to `ScoreAnswer::new`, and serialize in that order instead of sorted by index. Lookups such as `legend[&0]` and `probabilities.get(&2)` compile and behave as before. Iteration follows wire order. `legend[0]` now also compiles, as `IndexMap`'s positional index. Keys are still `u32`, so a non-canonical key such as `"01"` reads as index 1 and serializes as `"1"`.
- `Deserialize` for `Usage`, `ModelMetadata`, `NoulAnswer`, and `ChoiceAnswer` was derived in 0.1.2. It now runs the decoder `Client` uses, so serde and the client read a body the same way. A repeated key takes its last value instead of failing. A token count above `i64::MAX` wraps to a negative number instead of failing, as it always has in the client. An error names the field, such as `Invalid usage data at 'input_tokens'.` The input format must be self-describing, as JSON is.

### Changed

- `reqwest` 0.12 moved HTTP/2, the macOS/Windows system proxy, and charset decoding behind features. This crate enables `http2`, `system-proxy` and `charset` alongside `json` and `rustls-tls`, so the built-in client keeps all three, as it did on 0.1.x. It still uses rustls, not the platform TLS that `reqwest`'s own defaults would pull in.
- The client builds its request body as a `SystemOneRequest`. The bytes on the wire are unchanged. `request_body_bytes_are_stable` in `tests/contract.rs` pins them to 0.1.2's output.
- The `mock` feature and the tests use `wiremock` 0.6. This is not a breaking change. `mock` only turns on the optional `wiremock` dependency for the examples, and no `wiremock` type appears in this crate's API, so a dependent that uses `wiremock` 0.5 itself is unaffected. On Rust 1.85, `mock` needs the `wiremock` pin under MSRV.

### Added

- `wire` module: `SystemOneRequest` and the paths `SYSTEM_ONE_PATH` and `MODELS_PATH`. It also re-exports `Question`, `NoulCriteria`, `JsonContent`, `SystemOneResponse`, `ListModelsResponse`, `Answer`, `NoulAnswer`, `ChoiceAnswer`, `ScoreAnswer`, `Usage`, and `ModelMetadata`. `SystemOneRequest` is also at the crate root.
- `Serialize` and `Deserialize` for those types, matching the wire JSON. A question deserializes to `Noul`, `Choice`, or `Score` only when that variant serializes back to the same bytes, key order included, and to `Raw` otherwise.
- Deserializing a `SystemOneRequest` rejects a missing `state` or one that is not a string, object, or array; a `model` that is neither a string nor `null`; an empty `questions` object; a question that is not an object with a nonempty string `type`; a `choice` or `score` question without `criteria`; and a `score` question whose `criteria` is an empty array. These are the checks the client runs before it sends, so a server refuses what the client would refuse. `extra_body` can still send a field the client did not check.
- One decoder per response type. `SystemOneResponse`, `ListModelsResponse`, `Answer`, the three answer structs, `Usage`, and `ModelMetadata` deserialize through the decoder `Client` uses, so serde and the client accept the same input, give the same values, and name the same field on error. Inside a `SystemOneResponse`, an answer of unknown type is skipped as the client skips it.
- Round trips, as the `wire` module docs list them. Serializing writes compact JSON. Beyond that, a `Question` always serializes back to the bytes it was read from. A `SystemOneRequest` does when its keys start with `state`, then `model` if present, then `questions`. Otherwise its known keys move to the front, and a `null` `model` is left out. Response types keep only what the client decodes. Unknown keys, `null` token counts, and answers of unknown type are dropped. Fields come back in declaration order, and `1` in an `f64` field comes back as `1.0`.
- Constructors: `SystemOneResponse::new`, `ListModelsResponse::new`, `NoulAnswer::new`, `ChoiceAnswer::new`, `ScoreAnswer::new`, `Usage::new`, and `ModelMetadata::new`.

### Fixed

- `README.md` and `AGENTS.md` said `serde_json` was pinned to `=1.0.134`. `Cargo.toml` requires `serde_json = "1.0.134"`, which accepts any 1.x from 1.0.134 up. The docs now say so.

### MSRV

Still Rust 1.85. On 1.85.0, with a lockfile resolved for 1.85, the library builds and every test passes. Two transitive crates need a newer compiler but declare no `rust-version`, so the resolver cannot steer around them. `yoke-derive` 0.8.3 (via `url`) needs 1.87, and 0.1.2 was already affected. `wiremock` 0.6.5 (tests and the `mock` feature) needs 1.88. On 1.85, run `cargo update -p yoke-derive --precise 0.8.2` and `cargo update -p wiremock --precise 0.6.4`.

## 0.1.2 and earlier

See the git history.
