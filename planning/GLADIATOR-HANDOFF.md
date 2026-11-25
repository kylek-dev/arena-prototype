# Handoff: the Gladiator slice after the finished human male

Written 2026-11-25 for the agent that continues this work. It assumes no
memory of earlier sessions. Read first:
[AGENTS.md](../AGENTS.md),
[ADR 0052](../docs/decisions/0052-art-scope-and-quality-bar.md) (scope and
quality bar), [ADR 0053](../docs/decisions/0053-finished-character-kits.md)
(finished kits: structure, hair and beards, weapons, "Only finished kits
ship"), [ART-PRODUCTION.md](../docs/ART-PRODUCTION.md) and
[scripts/finished_kit/README.md](../scripts/finished_kit/README.md).

## Where things stand

- Branch `feat/finished-human-male-kit`, pushed, with a pull request to
  `main`. Every item below was reviewed and accepted by the user in the
  browser.
- **The finished human male is the only character the game ships**
  (`character_kits.SHIPPED = ()`; the Quaternius kits stay buildable with
  `kits=KITS`). Ancestry `human`, body `male`. Default look: swept-back hair,
  natural brows.
- **Kit content:** face presets (Chiseled, Broad, Lean, Rugged), three brow
  shapes, hairstyles Swept back, Close crop and Tied back, beards Full and
  Handlebar plus Stubble, tints for skin, hair, facial hair and eyes (eye
  swatches are deliberately dark and dull). Kit GLB 7.4 MB: opaque textures
  are embedded as JPEG.
- **Weapon:** the Gladiator greatsword (1.35 m, 5,000 triangles,
  `assets/weapons/gladiator-greatsword/`) is held in `socket_weapon_r`
  (`crates/arena-client/src/held_weapon.rs`). A "Sheathe / draw weapon"
  keybind (default Z) moves the local player's sword to `socket_back`;
  fighters start sheathed and using an ability draws it. Presentation only:
  not saved, not in the combat input, not networked (others hold theirs
  drawn).
- **The game has no shields.** The Gladiator slice has one weapon, the
  two-handed sword.
- **Camera zoom eases** (`camera::ease_zoom`); the customizer drag no longer
  selects text.
- **Meshy credits:** 216 spent on hair, beards and the sword. Credits are
  available, but **ask the user before every paid call**, with the model and
  the cost.

## What is next

Recommended order, agreed in outline with the user:

1. **Two-handed animations.** Every clip in the kit is unarmed (Attack is a
   punch jab, from Quaternius UAL1 plus hand edits by the user), so the drawn
   sword sits in one hand. Minimum: an idle hold and an attack swing with
   both hands on the grip; then locomotion and strafes with the sword; ideally
   a draw and sheathe motion for the keybind. Sources to compare and price
   for the user before spending: Meshy's animation library (about 5 credits
   to rig, 3 per action; it animates Meshy's own skeleton, so clips must be
   retargeted to the kit's 65-joint skeleton), CC0 packs (check what exists
   for two-handed swords), or the user animating in Blender. The kit's clips
   are written by `build_kit.py` from the first Meshy kit's GLB
   (`$PITBORN_ART/build/meshy-human-male/meshy-human-male.glb`), with the
   pelvis height retargeted and finger keys re-based. Read that section of
   `build_kit.py` before adding clips.
2. **Gladiator gear set,** six pieces: helmet (head), shoulder guard, chest
   harness, arm guard with bracers, greaves with skirt, sandal boots. Gear is
   skinned to the body and hides the regions it covers (ADR 0053 slot table).
   This is the riskiest unproven step: generated, fitted, skinned gear.
3. **Female body,** built exactly like the male (ADR 0053), with her own
   hairstyles.
4. **Eight spell icons** (2D, ART-PRODUCTION.md icon list).

## What the user expects

These come from the user's own feedback. Each was learned the hard way.

- **Finished art** (ADR 0052). Show a small visual sample before building
  around it: concept image, then the 3D piece on the body, then the game.
- **Generator-made art, real pieces.** Script-built hair and clay caps were
  rejected. Every customizable feature is its own mesh and material; nothing
  is detected in a texture at run time. Reading a generated piece's own paint
  at build time to place its edge was approved (for hair and beards).
- **Money:** confirm the model and cost before every paid Meshy call. If a
  create call fails or times out, check `meshy_list_tasks` before calling it
  again. If the image editor refuses a prompt on content policy, do not
  reword around it. Never post assets to the Meshy Community page. Keep the
  user's name, email and project details out of prompts.
- **Report what was actually observed.** The user reviews in the browser; do
  not claim a browser check you did not see. Ask the user to check after each
  visible change.
- **Reverts:** when the user says a change looked worse, put back exactly what
  they had (see the beard flags in `run_hair.sh`).
- Commit only when asked; push and open pull requests only when asked.

## Pitfalls already paid for

- Meshy's glTF splits vertices at UV seams: weld before any connectivity
  work.
- A generated bust's face is not the kit face: cheeks, jaw and neck stand up
  to 7.5 mm proud, and its ears sit elsewhere. Fit on skin that is skin on
  both; carve hair 6 mm clear of the kit's ears.
- Smart UV Project shatters grooved surfaces into specks; a tube chart (a
  ponytail) collapses. `hair_low.py` charts by direction (or `--by-normal`)
  and splits charts until each is a disc.
- Hair rigid on `Head` lets nape skin, partly weighted to the neck, slide out
  from under it; hair following neck skin bends a ponytail into the back. The
  kit uses the head's skin weights only, near the skin.
- Generated paint is dark (hair mean luminance about 0.19): a plain gain to
  the 0.78 `hairAtlas` clips highlights; `hair_tex.py` uses a shoulder.
- A single-image generation guesses depth: the sword's blade came out 4 cm
  thick and the grip 6 cm across; `weapon.py` thins them.
- libigl's `ray_mesh_intersect` drops hits; use closest-point queries or the
  numpy ray test in `hair_cut.py`.
- 64-bit sums for texture statistics; fill outside UV islands from good
  texels only; recompute smooth normals (Meshy exports flat ones).
- `check_glb.py` renders with shadows by default; use `--no-shadow` before
  chasing a blemish. Its `--hair-only` mode renders every hair and beard
  piece, which takes several minutes.
- Bevy needs its `jpeg` feature for the kit (enabled).

## Environment

- Python with the scientific packages: `~/dev/warrior/.venv/bin/python`.
- Headless Blender 5.3: `~/dev/blender-5.3/blender`.
- Art workspace outside Git: `~/pitborn-art/` (`human-male-kit/` for the kit
  build, `human-male-hair/` for hair and beards, `gladiator-greatsword/` for
  the sword, one dated raw file per attempt).
- Rebuild: `scripts/finished_kit/run_all.sh` (body), `run_hair.sh` (hair and
  beards; it records each piece's settings but has not been run end to end),
  the weapon stages in the tooling README, then `build_kit.py`, copy
  `out/{meshy-human-male.glb,kit.json}` to `assets/characters/kits/meshy-human-male/`,
  `./scripts/build-web.sh`, `npm run serve` on port 8000.
- Checks: `./scripts/check.sh`, `npm test`, `npm run lint`,
  `npm run format:check`, and
  `cargo clippy -p arena-client --target wasm32-unknown-unknown --locked -- -D warnings`.

## Not decided yet

- Whether other players should see a fighter's sheath state (needs a
  protocol change).
- Texture budget beyond JPEG: KTX2/Basis would need a C++ transcoder in the
  WASM build.
