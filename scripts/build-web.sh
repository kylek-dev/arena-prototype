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
mkdir -p web/pkg web/assets/characters
# Third-party stock art stays local. Clean clones still build with a placeholder.
python3 - <<'ASSETS'
import json
import shutil
from pathlib import Path
source = Path('assets/characters/pixellabs-undead-3417.glb')
target = Path('web/assets/characters') / source.name
if source.is_file():
    shutil.copy2(source, target)
else:
    target.unlink(missing_ok=True)
Path('web/assets/character.json').write_text(json.dumps({'available': source.is_file()}) + '\n')
print('Character asset:', 'local model copied' if source.is_file() else 'placeholder (optional model not installed)')
ASSETS
bindgen_flags=(--remove-producers-section)
if [[ "${ARENA_DEBUG_SYMBOLS:-0}" != 1 ]]; then
  bindgen_flags+=(--remove-name-section)
fi
wasm-bindgen --target web "${bindgen_flags[@]}" --out-dir web/pkg --out-name arena_client "target/wasm32-unknown-unknown/$profile/arena_client.wasm"
python3 - <<'PY'
import gzip
from pathlib import Path
p = Path('web/pkg/arena_client_bg.wasm')
print(f'WASM: {p.stat().st_size:,} bytes; gzip: {len(gzip.compress(p.read_bytes())):,} bytes')
PY
echo "Build and bindings: $((SECONDS-start_seconds)) seconds"
echo 'Serve: python3 -m http.server 8000 --directory web --bind 127.0.0.1'
