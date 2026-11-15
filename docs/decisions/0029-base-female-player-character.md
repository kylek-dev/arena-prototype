# ADR 0029: BaseFemale as the preferred player character

Date: 2026-11-14. **Status: accepted, then disabled by default the same day** at the user's request; see the update below. Originally requested to add `assets/BaseFemale.glb` to the game with the player using it. Human rendered/browser review never took place. This does not authorize binary redistribution.

## Decision

The player uses the first installed rigged character in this order: **BaseFemale**, then BaseMale, then the existing procedural fallback. There is no in-game body selector, and the opponent and other practice units stay procedural. Source GLBs are not modified. Collision, health, movement, cooldowns, damage, jump height and input contracts are unchanged.

`scripts/prepare_character.py` keeps the [ADR 0021](0021-base-male-skeletal-character.md) adapter and validates the selected source's own `<Name>_Rig` root. It writes only that model, to `web/public/assets/characters/<id>.glb` (`base-female` or `base-male`), and removes the other stale output. The manifest becomes `{"available": bool, "model": id | null}`. A malformed preferred source fails the build rather than silently falling back to the other model. The browser loader passes the model ID through the existing narrow bridge, and an older manifest without `model` is read as BaseMale.

The client keeps one `Profile` per model with its runtime path, measured rest bounds and authored clip segments. Unknown IDs select the procedural fallback. BaseFemale's measured rest span (sole 0.00006 m, ponytail top 1.747 m) is normalized to the shared 1.9 m character height, as BaseMale's was, so back weapon mounts, nameplates and picking stay consistent; this makes her about 9% larger than authored. The placement, scale and timing for BaseMale are unchanged.

Animation timing is per model. BaseFemale keys start at t = 0 at 30 fps. Jump seeks frames 8–25 (leaving the floor to touchdown) against rule-owned flight, and its 0.45 m root translation is stripped at build time. A successful Pulse plays Cast from frame 14 (late gather) to frame 36 (settled) at 1.5×, about 0.49 s, so the release on frames 17–21 lands quickly. The cast hold is derived from each profile's segment instead of being fixed at BaseMale's 0.8 s. Locomotion selection, crossfades, reset/tab-gap handling and readiness/fallback behavior are shared and unchanged.

## Verification

Synthetic Python GLB tests cover rig-name validation per character, selection order, the manifest, stale output removal and the rule that a malformed preferred source fails the build. Rust tests cover profile lookup and unknown IDs, grounded/forward-facing transforms for both models, jump seeks staying inside each model's airborne segment across a full rule-owned flight, and Cast seek/hold ending exactly at the authored segment within each clip's length. Browser diagnostics derive the model URL from the manifest; only their syntax was checked.

Local verification on 2026-11-14: `./scripts/check.sh` passed (72 client, 13 simulation, 11 Python, 28 Node input and 166 Vitest tests, plus lint/type/format checks). WASM Clippy with warnings denied, the complete web build with its non-root static HTTP/MIME check (9 resources, including `assets/characters/base-female.glb`) and the scripted headless scenario passed. The generated runtime GLB has one character-only scene, the eight clips in canonical order, no authoring extras and only rotation/scale on Jump's root. No browser was launched.

Human review must check BaseFemale's scale, facing and feet; back weapon clearance against her hair, ponytail and shoulders for both weapon types; all movement directions and diagonals, walls and idle transitions; standing/running jumps and landing; Pulse success and rejection; reset/tab resume; loading/failure fallback; and console errors on the intended browser/GPU. Automated checks do not establish rendered appearance.

## Update — 2026-11-14: rigged players disabled

At the user's request, the player uses only the procedural model for now. `scripts/prepare_character.py` ignores installed BaseFemale/BaseMale sources unless `ARENA_RIGGED_CHARACTER=1` is set. By default it removes any generated character GLBs and writes `{"available": false, "model": null}`, so the client loads no model and keeps the procedural player. The selection order, profiles and timing above are retained unchanged for re-enabling. A Python test covers the default-off behavior with both sources present. The 2026-11-14 default build shipped 8 static resources with no character GLB.
