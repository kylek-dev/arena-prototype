#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
# The client is type-checked above; executing a GUI is covered by browser tests.
cargo test -p arena-sim -p arena-headless --locked
node --check web/shell.js
