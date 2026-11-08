#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
# The client is type-checked above; rendered behavior is reviewed manually.
cargo test -p arena-sim -p arena-headless --locked
# Presentation contracts and character-relative movement regressions.
cargo test -p arena-client --lib --locked
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s tests/unit -p 'test_*.py'
node --check web/input.js
npm run typecheck
npm run lint
npm run format:check
npm test
