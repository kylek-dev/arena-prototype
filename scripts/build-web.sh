#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
if [[ "$(wasm-bindgen --version)" != 'wasm-bindgen 0.2.122' ]]; then
  echo 'Install the pinned CLI: cargo install wasm-bindgen-cli --version 0.2.122 --locked' >&2
  exit 1
fi
profile="${ARENA_WEB_PROFILE:-web}"
start_seconds=$SECONDS
cargo build --locked -p arena-client --target wasm32-unknown-unknown --profile "$profile"
mkdir -p web/public/pkg web/public/assets/characters
python3 scripts/prepare_character.py
python3 scripts/prepare_weapon.py
bindgen_flags=(--remove-producers-section)
if [[ "${ARENA_DEBUG_SYMBOLS:-0}" != 1 ]]; then
  bindgen_flags+=(--remove-name-section)
fi
wasm-bindgen --target web "${bindgen_flags[@]}" --out-dir web/public/pkg --out-name arena_client "target/wasm32-unknown-unknown/$profile/arena_client.wasm"
python3 - <<'PY'
import gzip
from pathlib import Path
p = Path('web/public/pkg/arena_client_bg.wasm')
print(f'WASM: {p.stat().st_size:,} bytes; gzip: {len(gzip.compress(p.read_bytes())):,} bytes')
PY
echo "Build and bindings: $((SECONDS-start_seconds)) seconds"
npm run build
python3 scripts/check-static-assets.py
echo 'Serve production: npm run serve; UI development: npm run dev'
