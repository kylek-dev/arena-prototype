#!/usr/bin/env bash
# Starts a development match server and front door sharing a fresh signing
# key, for trying the lobby locally (ADR 0045). Serve the client separately
# (`npm run serve`), then open http://127.0.0.1:8090/lobby/ and create a match.
# Ctrl+C stops both. Development only: plain ws:// and http://, loopback.
#
# Usage: scripts/dev-door.sh [page URL]   (default http://localhost:8000/)
set -euo pipefail
cd "$(dirname "$0")/.."
page="${1:-http://localhost:8000/}"
cargo build -q -p arena-server --bin arena-server --bin arena-door
ARENA_TICKET_KEY="$(python3 -c 'import secrets; print(secrets.token_hex(32))')"
export ARENA_TICKET_KEY
target/debug/arena-server --dev --bind 127.0.0.1:8080 &
server=$!
target/debug/arena-door --dev --bind 127.0.0.1:8090 \
  --match-server ws://127.0.0.1:8080/play --page "$page" &
door=$!
trap 'kill -INT "$server" "$door" 2>/dev/null; wait' INT TERM EXIT
wait -n "$server" "$door"
