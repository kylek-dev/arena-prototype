#!/usr/bin/env bash
# Full rebuild: fit (4096 masters) -> skin paint -> regions -> textures -> faces, brows -> kit GLB + manifest.
set -euo pipefail; cd "$(dirname "$0")"
PY=${KIT_PYTHON:-$HOME/dev/warrior/.venv/bin/python}
./run_fit.sh 4096
$PY textures.py | tail -2
$PY faces.py | tail -3
$PY brows.py | tail -1
$PY textures.py > /dev/null
$PY build_kit.py | tail -3
