#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
cargo test
cargo test --features blocking
RUSTDOCFLAGS='-D missing-docs' cargo doc --no-deps --all-features
echo "typesafe-sdk tests passed"
