#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
cargo fmt --all --check
cargo check --features mock
cargo check --features "mock blocking"
cargo clippy --features mock -- -D warnings
cargo clippy --features "mock blocking" -- -D warnings
cargo test --features mock
cargo test --features "mock blocking"
cargo build --examples --features mock
cargo build --example blocking_triage --features "mock blocking"
echo "typesafe-sdk verify passed"
