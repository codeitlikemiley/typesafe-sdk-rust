#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
cargo fmt --check
cargo test --features mock
cargo test --features "mock blocking"
echo "typesafe-sdk tests passed"
