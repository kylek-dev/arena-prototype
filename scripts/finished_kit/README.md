# Finished kit build (human male)

Offline tooling that rebuilds a generated body into a finished character kit,
as decided in [ADR 0053](../../docs/decisions/0053-finished-character-kits.md).
It produces `meshy-human-male.glb` and `kit.json`. Nothing here runs in the
game build or in the browser: the game ships the built file from
`assets/characters/kits/meshy-human-male/`.

The scripts are written for this one body. Heights, the hand-read eye outline
and similar constants are specific to it; a second body needs its own values.

## What it needs

- **The art workspace**, outside Git (`~/pitborn-art`, or set `PITBORN_ART`):
  - `character-sheets/human/male/Meshy_AI_male_human.glb` and
    `Meshy_AI_male_human_detailed.glb`: the raw Meshy exports.
  - `build/meshy-human-male/meshy-human-male.glb`: the first Meshy kit. Its
    bind pose and its ten clips are reused.
  - `human-male-kit/src/`: the raw textures (`extract_textures.py`), plus
    `rig.game_engine.json` and `targets/` copied from the MPFB add-on's CC0
    asset pack.
  - `human-male-kit/work/mh_base.npz`: the scaffold mesh, exported once from
    a Blender session with MPFB (`export_scaffold.py`).
  - `human-male-kit/work/`, `tex/`, `out/`, `qa/`: intermediate files, final
    textures, the built kit and review renders. All are regenerated.
- **Python 3.12** with numpy 2.2, scipy 1.18, libigl 2.5, opencv-python-headless
  5.1 and pillow 13 (`KIT_PYTHON`, default `~/dev/warrior/.venv/bin/python`).
- **Blender 5.3**, headless (`BLENDER`, default `~/dev/blender-5.3/blender`),
  for the normal bake and the review renders.

## Build

```sh
./run_all.sh          # everything from the raw files, 4096 px masters
```

`run_all.sh` runs `run_fit.sh 4096` and then the texture, face, brow and kit
stages. To change one thing, re-run its stage and every later stage.

| Stage | Script | Writes |
| --- | --- | --- |
| Prep, once | `extract_textures.py`, `prep_target.py`, `dehem.py`, `eye_outline.py` | `src/*.png`, `work/target.npz`, `work/target_clean.npz`, `work/eye_outline.json` |
| Pose the scaffold | `fit_pose.py` | `work/pose.npz` |
| Surface fit | `fit_nicp.py` | `work/fit.npz` |
| Eyes | `eyes.py` | `work/fit_eyes.npz`, `work/eye_parts.npz` |
| Closed mouth | `mouth.py` | `work/fit_eyes.npz` (in place) |
| Regions, straight garment edges | `regions.py` | `work/regions.npz`, `work/fit_eyes.npz` (in place) |
| Iris and eye white | `iris.py` | `work/tex_eye_*.png` |
| Paint transfer | `bake.py RES` | `work/bake_*.png`, `work/bake_*.npz` |
| Normal map | `bake_normal.py` (Blender), `normal_post.py` | `work/bake_normal.png` |
| Blank skin, graded to the palette | `skin_paint.py` | `work/tex_skin.png` |
| Final textures | `textures.py` | `tex/*.png` |
| Face presets | `faces.py` | `work/face_morphs.npz` |
| Eyebrows | `brows.py` | `work/brows.npz`, `tex/brows_*.png` |
| Kit | `build_kit.py [OUT_DIR]` | `out/meshy-human-male.glb`, `out/kit.json` |

Shared modules: `kitlib.py` (paths, spaces, mesh helpers), `mh.py` (scaffold),
`rigdata.py` and `skeleton.py` (the kit skeleton for this body), `layout.py`
(faces, UVs, regions), `glbwriter.py` and `gltfkit.py` (GLB input and output),
`eyelib.py`.

## Install into the game

```sh
cp "$PITBORN_ART/human-male-kit/out/"{meshy-human-male.glb,kit.json} \
   ../../assets/characters/kits/meshy-human-male/
python3 ../prepare_character.py      # ships every kit into web/public
../build-web.sh                      # refreshes dist for `npm run serve`
```

## Review tools

All take the built GLB, which is what the game loads.

- `check_glb.py KIT.glb OUT_PREFIX [--no-shadow] [--skin r,g,b]`: structure
  checks and renders of the body, face, mouth, waist, back and hems. `--skin`
  applies a skin tint the way the game does.
- `clip_sheet.py KIT.glb OUT_DIR [frames] [view]`: frames of every clip.
- `clip_closeups.py`, `hand_track.py`: close-ups that follow a point or a hand
  through a clip.
- `torso_profile.py KIT.glb`: the torso's outline in the bind pose and Idle.
- `sheet.py OUT.png COLS HEIGHT images…`: contact sheet.
- `overlay.py`, `heat.py`, `render_fit.py`, `region_view.py`,
  `assemble_preview.py`, `ref_renders.py`, `ref_views.py`: fit, region and
  reference renders used while building.

## Status

- `build_kit.py` run from this directory reproduces the installed kit byte for
  byte (checked on 2026-11-24).
- The whole of `run_all.sh` has not been re-run from this directory since the
  scripts moved here from the art workspace.
- `export_scaffold.py` is the code that was run once in a live Blender; it has
  not been re-run.
