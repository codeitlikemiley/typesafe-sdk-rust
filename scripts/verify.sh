#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
cargo fmt --all --check
cargo check --features mock
cargo check --features "mock blocking"
cargo check --features "mock blocking clef openai-decisions"
cargo clippy --all-targets --features mock -- -D warnings
cargo clippy --all-targets --features "mock blocking" -- -D warnings
cargo clippy --all-targets --features "mock blocking clef openai-decisions" -- -D warnings
cargo clippy --all-targets --no-default-features --features "mock blocking" -- -D warnings
cargo test --features mock
cargo test --features "mock blocking"
cargo test --no-default-features --features "mock blocking"
cargo test --features "mock blocking clef openai-decisions"
cargo build --examples --features mock
cargo build --example blocking_triage --features "mock blocking"
cargo build --examples --features "mock clef openai-decisions"
RUSTDOCFLAGS='-D missing-docs' cargo doc --no-deps --all-features
echo "typesafe-sdk verify passed"
