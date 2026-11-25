#!/usr/bin/env bash
# Rebuild the fitted body from the raw Meshy files: pose -> surface fit -> eyes -> paint and normal bakes.
set -euo pipefail; cd "$(dirname "$0")"
PY=${KIT_PYTHON:-$HOME/dev/warrior/.venv/bin/python}; B=${BLENDER:-$HOME/dev/blender-5.3/blender}; W=${PITBORN_ART:-$HOME/pitborn-art}/human-male-kit/work
$PY fit_pose.py | grep -E "global|hand length|fingers found|foot scale|head:|distance"
$PY fit_nicp.py | grep -E "^ear|final"
$PY eyes.py | tail -2
$PY mouth.py | tail -3
$PY regions.py | grep -E "waist loop|trunks hem:|triangles"
$PY iris.py > /dev/null
$PY bake.py ${1:-2048}
$B -b --factory-startup --python-exit-code 1 -P bake_normal.py -- ${1:-2048} $W/bake_normal_raw.png 2>&1 | grep -E "^bake|Error|Traceback" || true
$PY normal_post.py
$PY skin_paint.py | tail -4
