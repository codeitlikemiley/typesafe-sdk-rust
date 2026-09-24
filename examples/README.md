# Cookbook examples

Runnable Rust ports of patterns and cookbooks from [docs.typesafe.ai](https://docs.typesafe.ai/llms.txt).

## Mock vs live

| Mode | How to run |
| --- | --- |
| **Mock** (default, no API key) | `cargo run --example NAME --features mock` |
| **Live** (your account) | `TYPESAFE_LIVE=1` plus `TYPESAFE_API_KEY` set, then `cargo run --example NAME` |

Mock mode starts an in-process [wiremock](https://crates.io/crates/wiremock) server with canned JSON from `examples/support/fixtures.rs`. Live mode calls `TYPESAFE_BASE_URL` when set, otherwise `https://api.typesafe.ai`. `custom_base_url` is the exception. Its live path requires `TYPESAFE_BASE_URL` and does not fall back to `https://api.typesafe.ai`.

CI runs the same fixtures in `tests/examples_integration.rs` via `cargo test --features mock`.

## Examples

| Example | Doc link |
| --- | --- |
| `system_one` | [Quick start](https://docs.typesafe.ai/introduction/quickstart.md) |
| `fan_out` | [Speculative fan-out](https://docs.typesafe.ai/patterns/fan-out.md) |
| `confidence_routing` | [Confidence](https://docs.typesafe.ai/confidence.md) |
| `guardrails` | [LLM guardrails](https://docs.typesafe.ai/cookbooks/llm_guardrails.md) |
| `citation_check` | [Citation check](https://docs.typesafe.ai/cookbooks/citation_check.md) |
| `structured_state` | [State](https://docs.typesafe.ai/concepts/state.md) |
| `composite_scoring` | [Composite scoring](https://docs.typesafe.ai/patterns/composite-scoring.md) |
| `intent_routing` | [Intent routing](https://docs.typesafe.ai/patterns/intent-routing.md) |
| `noul_uncertainty` | [Self-consistency nouls](https://docs.typesafe.ai/cookbooks/consistency_noul_cookbook.md) |
| `blocking_triage` | Blocking client on live API; mock path uses async client against wiremock (`--features "mock blocking"`) |
| `custom_base_url` | Non-default base URL for `POST /v1/systemone` and `GET /v1/models`. Mock mounts `/v1/…` on the wiremock origin. Live requires `TYPESAFE_BASE_URL` and does not fall back to `https://api.typesafe.ai`. |

## Build all mocks locally

```bash
cargo test --features mock
for ex in system_one fan_out confidence_routing guardrails citation_check \
  structured_state composite_scoring intent_routing noul_uncertainty custom_base_url; do
  cargo run --example "$ex" --features mock
done
cargo run --example blocking_triage --features "mock blocking"
```
