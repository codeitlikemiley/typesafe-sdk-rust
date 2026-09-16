#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
cargo test
cargo test --features blocking
echo "typesafe-sdk tests passed"
