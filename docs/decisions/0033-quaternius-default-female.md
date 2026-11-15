# ADR 0033: Quaternius female base and offline humanoid animation

Date: 2026-11-14. **Status: accepted for local integration**, at the user's
request to curate the downloaded packs and use an animated female human by
default. Human rendered review is pending.

Follow-up: [ADR 0034](0034-dressed-character-and-jump-edges.md) adds Ranger armor,
bun hair and the included takeoff/landing clips. Its ten-state dressed model
supersedes the initial eight-state bare base described below.

## Decision

Prefer the CC0 Universal Base Characters female full-body human and CC0
Universal Animation Library 1 Standard. Build a self-contained GLB offline
using the Python standard library. Retain the female's rest skeleton, skin and
bone lengths; check bone names and hierarchy, retarget local rotation deltas,
scale pelvis pose offsets, and remove root motion/airborne pelvis elevation.
Only eight selected animation states and the base's textures ship.

V2 is compatible supplemental content, not a replacement for the locomotion
library. Both downloaded free Standard libraries lack dedicated backward and
strafe jogs. Reverse the forward cycle for backward movement and reuse it for
lateral movement without rotating the character. This is an explicitly limited
placeholder set, not production directional animation. Add authored lateral
clips later without changing simulation intent or authority.

`prepare_character.py` selects `quaternius-female` by default when both sources
are installed. `ARENA_RIGGED_CHARACTER=0` forces the procedural player.
`ARENA_RIGGED_CHARACTER=1` also allows the historical BaseFemale/BaseMale
fallbacks when Quaternius sources are absent; those local exports' publication
licensing remains unconfirmed. With neither setting nor Quaternius sources,
use the procedural player, not license-unconfirmed binaries.

Use the existing manifest, Bevy 0.20.1 animation graph/controller, load/error
fallback and scale convention. The model changes no combat rule, collider,
position, cooldown, health, input ABI or multiplayer authority. Other actors
remain procedural. Hair/outfits/race selection are not part of this change.

## Asset library

Retain glTF/GLB exports, all referenced BIN/PNG dependencies, unique alternate
textures and license notices. Archive redundant engine-specific formats before
pruning; repair the supplied broken base image URIs. Keep CC0 notices and
Quaternius authorship separate from the code's MIT license.

The races pack's actual license is QAL v1.0, not CC0. At the user's request,
retain it locally for permitted game use. Git-ignore raw race sources to avoid
standalone redistribution. QAL permits shipping a completed game incorporating
them but forbids free or paid standalone asset distribution; review the exact
future release packaging before including them.

See [asset inventory, provenance, hashes and clip mapping](../../assets/quaternius/README.md).

## Verification

Synthetic Python tests cover rest-relative rotation transfer, normalized
quaternions, hierarchy/clip validation, root/scale/length suppression, airborne
height, reversed keys, embedding, canonical order and default selection.
Rust tests include the new profile in grounding/facing and flight/cast
boundaries. A manually invoked ignored integration test loads the prepared
model with Bevy's real glTF loader, without a window/renderer/GPU:

```sh
python3 scripts/prepare_character.py
cargo test -p arena-client --lib prepared_quaternius_loads_in_bevy_without_a_renderer --locked -- --ignored
```

Run the repository checks, WASM Clippy, complete web build and headless scenario.
Human browser review must cover mesh deformation, proportions, source-base
appearance, facing, feet, movement substitutes, transitions, jump/landing,
Pulse success/rejection, back-weapon fit and fallback. No automated browser gate
or unobserved rendering/performance pass is claimed.

Local verification on 2026-11-14: workspace checks, WASM Clippy with warnings
denied, the complete production web build (16 static HTTP/MIME resources under
`/dist/`) and the headless scenario passed. The ignored real-Bevy loader test
was explicitly run and passed. All external dependencies of the 136 retained
glTF files resolve. Prepared and production female GLBs match byte for byte.
No browser was launched; human rendered review remains pending.
