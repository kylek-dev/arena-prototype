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
mkdir -p web/public/assets/licenses
cp assets/animations/LICENSE.txt web/public/assets/licenses/universal-strafes-MIT.txt
python3 scripts/prepare_weapon.py
# Prepared CC0 textures are tracked; regenerate them with scripts/fetch_textures.py.
rm -rf web/public/assets/textures
mkdir -p web/public/assets/textures
cp assets/textures/*.png web/public/assets/textures/
# Map files for the in-game editor; the WASM also embeds colosseum.json.
rm -rf web/public/assets/maps
mkdir -p web/public/assets/maps
cp assets/maps/*.json web/public/assets/maps/
# Static builds have no listing endpoint; the editor's Open list reads this.
python3 - <<'PY'
import json
from pathlib import Path
maps = []
for path in sorted(Path('assets/maps').glob('*.json')):
    maps.append({'id': path.stem, 'name': json.loads(path.read_text()).get('name', path.stem)})
Path('web/public/assets/maps/index.json').write_text(json.dumps({'maps': maps}, indent=2) + '\n')
PY
# CC0 Quaternius props for map placement, with shared shrunken textures.
python3 scripts/prepare_props.py
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
