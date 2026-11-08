# ADR 0021: BaseMale skeletal player presentation

Date: 2026-11-07. **Status: accepted for the local prototype**, at the user's request to integrate `assets/BaseMale.glb`. Human rendered/browser review is pending. This does not authorize binary redistribution.

## Decision

Use the original Blender-authored BaseMale character for the player when its local source is installed. The opponent remains procedural. Keep the complete procedural player as the loading/error/absent-asset fallback. The source GLB is not modified. Neither collision, health, movement, cooldowns, damage, jump height nor input contracts change.

`scripts/prepare_character.py` generates a single character-only scene at `web/public/assets/characters/base-male.glb` and the existing availability manifest. Validate the rig, skinned meshes, reachable joints and eight named clips. Canonicalize animation order for explicit Bevy animation labels; remove only Jump's root-translation channel so the rule-owned jump is not added twice. Preserve node/skin/accessor indices and binary data. Drop authoring extras (including local file paths and MCP server state) and unrelated scenes from the generated document. Missing source removes stale runtime output and selects the procedural fallback; malformed installed sources fail the build rather than silently shipping the wrong model.

Enable Bevy **0.20.1** `gltf_animation`; follow that pinned release's `animated_mesh.rs` and `animated_mesh_control.rs` examples. Construct one shared animation graph. The scene-ready observer requires meshes and exactly one animation player before revealing the imported scene and hiding the fallback. Keep the scene hidden on malformed readiness; load/dependency failures retain the visible fallback and existing loading notice. Normalize measured rest bounds (sole 0.009 m, head 1.907 m) to the existing 1.9 m character height and rotate glTF +Z forward to game -Z.

`character_animation.rs` selects idle/forward/backward/left/right using actual interpolated displacement in fighter-local coordinates, not held input or camera orientation. Dominant-axis diagonal selection prefers longitudinal movement on a tie. Playback rate is bounded, with short crossfades; this is not foot-locking IK or a directional blend space. Walls settle to idle. First samples, resets, teleports and background gaps discard stale movement/casts. The procedural animation carrier stops adding bob when the authored rig is ready; skeletal clips already supply body motion. Back weapons retain their existing separate carrier/mounts, not new hand attachments or bone-bound armor.

Seek Jump's takeoff-to-landing limb poses using rule-owned vertical velocity and fixed-step interpolation; the fighter root alone supplies world height. Pulse remains instantaneous: a successful rule-owned cooldown edge triggers only the latter cosmetic gather/release portion of Cast, without delaying damage. Rejected intent never triggers it. Jump takes priority while airborne. Cast is full-body and can slide during movement; human review/tuning is required. Attack remains available in the GLB/graph for a future melee presentation, but no new attack ability or binding is invented. There is no cast-time rule, VFX, armor editor, animation settings UI or runtime AI dependency.

## Verification

Synthetic Python GLB tests exercise scene isolation, clip order, root-channel removal, binary/source preservation, invalid exports, skin/animation boundaries and missing-source cleanup. Renderer-free Rust tests exercise directional selection, stopped/blocked displacement, facing, accepted/rejected cast edges, reset/teleport/tab-resume handling, late loading, jump posing and readiness/fallback behavior. Run `./scripts/check.sh`, WASM Clippy and the complete web build. Static HTTP checks include the generated character under a non-root hosting prefix.

Human review must cover scale/facing/feet, all movement directions/diagonals, walls, transitions, standing/running jumps/landing, Pulse success/rejection, reset/tab resume, fallback/loading errors, both weapon types/palettes, clipping and console errors on the intended browser/GPU. Automated builds and asset checks are not a rendered pass.

Local asset inspection on 2026-11-07 confirmed one generated scene, eight ordered clips, the 45-joint skin, unchanged binary payload, removed Jump root-translation channel and stripped authoring extras. Source SHA-256: `ca5af6352a9e1cb2b18bc02d9a67bb7953a7c68d152fa84b10973f60f448ca15`. WASM Clippy with warnings denied, the complete web build and its non-root static HTTP/MIME checks passed. The scripted headless scenario also passed. No browser was launched; human rendered review remains pending.

`./scripts/check.sh` passed locally: 12 simulation tests, 56 client tests, nine synthetic Python asset tests, 26 Node input tests and 131 Vitest tests, plus workspace formatting/Clippy and frontend checks. The manually invoked character browser diagnostics were updated for the BaseMale URL and animated state (and no longer count weapon downloads as character downloads); only their JavaScript syntax was checked, not Playwright or rendered behavior.

## Human review update — 2026-11-08

The user verified and approved all implemented UI/rendered changes, including character and animation presentation. Historical pending human-review statements above are superseded. Binary redistribution is not authorized by this UI approval. See [review record](../UI-REVIEW.md).
