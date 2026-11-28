#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
# Account tests need PostgreSQL (ADR 0055): ARENA_TEST_DATABASE_URL, or a
# throwaway container started here and removed on exit.
if [[ -z "${ARENA_TEST_DATABASE_URL:-}" ]]; then
  if ! docker info >/dev/null 2>&1; then
    echo "check.sh: account tests need PostgreSQL: set ARENA_TEST_DATABASE_URL or start Docker" >&2
    exit 1
  fi
  postgres="arena-check-postgres-$$"
  docker run -d --rm --name "$postgres" -e POSTGRES_PASSWORD=arena-check \
    -p 127.0.0.1::5432 postgres:18.7-alpine >/dev/null
  trap 'docker stop "$postgres" >/dev/null 2>&1 || true' EXIT
  # Over TCP, so the image's init-time server (socket only) does not count.
  for _ in $(seq 60); do
    docker exec "$postgres" pg_isready -U postgres -h 127.0.0.1 -q && break
    sleep 1
  done
  port="$(docker port "$postgres" 5432/tcp | head -1 | sed 's/.*://')"
  export ARENA_TEST_DATABASE_URL="postgres://postgres:arena-check@127.0.0.1:${port}/postgres"
fi
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
# The optional Pyroscope agent (ADR 0054) is built into the Docker image.
cargo clippy -p arena-world --all-targets --features profiling --locked -- -D warnings
# The client is type-checked above; rendered behavior is reviewed manually.
cargo test -p arena-sim -p arena-protocol -p arena-world -p arena-bot -p arena-headless -p arena-textures -p arena-map -p arena-palette --locked
# Presentation contracts and character-relative movement regressions.
cargo test -p arena-client --lib --locked
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s tests/unit -p 'test_*.py'
node --check web/input.js
node scripts/palette.mjs --check
npm run typecheck
npm run lint
npm run format:check
npm test
