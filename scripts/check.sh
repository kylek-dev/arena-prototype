#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
# The client is type-checked above; rendered behavior is reviewed manually.
cargo test -p arena-sim -p arena-headless --locked
node --check web/shell.js
node --check web/input.js
node --test tests/unit/*.test.mjs
