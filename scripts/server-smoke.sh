#!/usr/bin/env bash
# Smoke test of the real binaries: start a development match server, play bot
# 2v2 matches through 100 ms of simulated latency with drop-outs and rejoins,
# then drain the server. Fails on any bot failure (error, unexpected close,
# state hash mismatch, failed rejoin) or an unclean server exit.
#
# Usage: scripts/server-smoke.sh [debug|release]   (builds must already exist)
set -euo pipefail
cd "$(dirname "$0")/.."
bin="target/${1:-debug}"
port="${ARENA_SMOKE_PORT:-18085}"
log="$(mktemp)"

"$bin/arena-server" --dev --bind "127.0.0.1:$port" --drain-seconds 2 2>"$log" &
server=$!
cleanup() {
  kill "$server" 2>/dev/null || true
  rm -f "$log"
}
trap cleanup EXIT

if ! curl -sf --retry 30 --retry-connrefused --retry-delay 1 \
  "http://127.0.0.1:$port/healthz" >/dev/null; then
  echo "server did not become healthy" >&2
  cat "$log" >&2
  exit 1
fi

"$bin/arena-bot" --server "127.0.0.1:$port" --matches 4 --team-size 2 \
  --seconds 10 --rtt 100 --jitter 20 --reconnect-every 4

kill -TERM "$server"
if ! wait "$server"; then
  echo "server exited uncleanly" >&2
  cat "$log" >&2
  exit 1
fi
echo "server drained cleanly"
