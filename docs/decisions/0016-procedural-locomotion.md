# ADR 0016: Presentation-only procedural locomotion

Date: 2026-11-07. **Status: accepted for the prototype**, at the user's request. Human rendered review is pending.

## Decision

Animate the procedural humanoids with transform hierarchies at shoulders, elbows, hips and knees. Preserve the authored bind-pose coordinates and appearance scaling. A separate motion carrier adds small positive-height walking bob and idle breathing motion, shared by the character and back weapons. Ground circles, target indicators, authoritative fighter transforms and collision do not bob.

`animation.rs` samples the interpolated fighter transform after presentation. Actual planar displacement, expressed in current fighter-local coordinates, drives a distance-based gait: opposed arm/leg swing, reversed forward swing when backing up, and lateral swing when strafing. Blocked movement therefore settles to idle instead of walking against a wall. An exponential blend eases movement to rest. Simulation height blends to a tucked-knee airborne pose and back on landing; the gait does not advance in flight. Initial frames, reset/teleport discontinuities and background-tab gaps discard velocity rather than animating a false stride. No simulation, input ABI or combat timing changes.

Internal resource settings expose stride length, limb swing, blending, bob and breathing amounts for future tuning. There is no animation settings UI or persisted animation format. No per-frame mesh creation, entity spawning, animation clips, external assets or new dependencies are needed.

The optional imported model has no skeleton, so its limbs remain static; it receives only the shared body bob/breathing motion. The procedural fallback remains articulated while loading or on failure. Debug `character.animated` identifies whether the player's visible character uses procedural limb animation; a ready imported model reports false. This is a lightweight prototype, not foot planting/IK, authored attack animation or a production rig.

## Verification

Renderer-free tests cover paired counter-swing, backward/strafe axes, movement-to-idle blending, reset/teleport handling, airborne knee tuck, landing and background gaps. An ECS test exercises actual hierarchical spawning, cached meshes, quality application and the animation system together, including stable mesh/entity counts over repeated updates.

Manual review must check idle, forward/backward/diagonal/strafe motion, direction changes, stopping at walls/pillars, free-look versus turning, standing/running jumps, landing, reset, tab resume, weapon fit, both actor presets/proportions and Low/High detail. Also verify targeting/hover and optional imported-model/fallback behavior. Human rendered/browser review remains pending; automated tests do not establish appearance or performance.

Local verification on 2026-11-07: `./scripts/check.sh` passed (12 simulation tests, 30 client tests, 26 Node input tests and 122 Vitest tests); WASM Clippy with warnings denied, `./scripts/build-web.sh` and the scripted headless scenario passed. The build used procedural characters (optional model not installed). No browser was launched; human rendered review remains pending.
