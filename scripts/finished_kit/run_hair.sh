#!/usr/bin/env bash
# Rebuild every hairstyle, beard and the stubble from the raw Meshy busts in $PITBORN_ART/human-male-hair,
# with the settings each was built with. Then run build_kit.py. See README.md, "Hair and beards".
set -euo pipefail
cd "$(dirname "$0")"
ART="${PITBORN_ART:-$HOME/pitborn-art}"; A="$ART/human-male-hair"; W="$A/work"; K="$ART/human-male-kit"
PY="${KIT_PYTHON:-$HOME/dev/warrior/.venv/bin/python}"; BL="${BLENDER:-$HOME/dev/blender-5.3/blender}"
mkdir -p "$W"
# name  folder  stem  fit flags  cut flags  triangles  chart centre  edge mm
while read -r n d stem fit cut tris centre edge; do
  raw="$A/$d/2026-11-24_model_a"
  [ "$fit" = "-" ] && fit=""; low=""
  "$PY" hair_fit.py "$raw.glb" "$W/${n}_fit.npz" $fit
  case "$cut" in
    geometry) "$PY" hair_cut.py "$W/${n}_fit.npz" "$W/${n}_dense.npz" ;;
    paint)    "$PY" hair_cut.py "$W/${n}_fit.npz" "$W/${n}_dense.npz" --zmin 1.58 --paint "${raw}_base_color.png" ;;
    paint-low) "$PY" hair_cut.py "$W/${n}_fit.npz" "$W/${n}_dense.npz" --zmin 1.45 --paint "${raw}_base_color.png" ;;
    # the beards as the user preferred them: without the later ear carve, nose cut, extra smoothing or chart splits
    beard)    "$PY" hair_cut.py "$W/${n}_fit.npz" "$W/${n}_dense.npz" --zmin 1.45 --no-ears --paint "${raw}_base_color.png" --paint-smooth 3 --beard --no-nose
              low="--no-split" ;;
  esac
  "$BL" -b --factory-startup -P hair_low.py -- "$W/${n}_dense.npz" "$W/${n}_low.npz" "$W/${n}_normal.png" --tris "$tris" --centre "$centre" $low
  "$PY" hair_tex.py "$W/${n}_low.npz" "$W/${n}_dense.npz" "${raw}_base_color.png" "$W/${n}_normal.png" "$K/tex/$stem" --edge "$edge"
  cp "$W/${n}_low.npz" "$K/work/$stem.npz"
done <<'EOF'
swept        swept-back    hair_swept    -       geometry   7600 0,0,1.70     6
crop         close-crop    hair_crop     -       paint      7600 0,0,1.70     2
tied         tied-back     hair_tied     -       paint-low  7600 0,0,1.70     2
beard_full   beard-full    beard_full    --beard beard      7000 0,-0.03,1.66 2
beard_goatee beard-goatee  beard_goatee  --beard beard      5000 0,-0.03,1.66 2
EOF
"$PY" stubble.py "$W/beard_full_dense.npz"
"$PY" build_kit.py
