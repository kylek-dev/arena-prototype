# ADR 0028: Rounded procedural characters and open-step strafing

Date: 2026-11-08. **Status: accepted for implementation**, at the user's request. Human rendered review is pending.

## Decision

Refine the existing procedural character rather than add another binary model or dependency. Replace the broad capsule torso and cylindrical limb segments with scaled smooth spheres (ellipsoids). Overlap rounded shoulders, elbows, knees, wrists and ankles with their adjacent segments; add a thumb to each rounded hand. Slightly widen the hip spacing and use rougher skin, hair and trouser materials to reduce the plastic appearance. Preserve the existing Low/High shared mesh cache, facial features, appearance presets, total height and grounded boots.

Add a chest pivot parenting the tunic, neck, head and arms, and a separate head pivot. Animate a subtle directional chest lean and counter-twist, head counter-motion, relaxed outward shoulders, breathing and variable elbow flexion. Back weapons retain the existing shared carrier motion rather than inheriting the chest pivot; check clearance manually. Authored BaseMale clips and source assets are unchanged.

Replace symmetric lateral hip swing with alternating outward-only abduction and return to the resting stance. A leg can open its own lane but never swing inward across the stance centerline. Forward/backward swing remains separate and combines with the lateral step for diagonals. Strafe knee lift is smaller than forward knee lift. Actual rendered displacement, existing exponential blending, reset/tab-gap handling and rule-owned airborne height remain the drivers. This is a lightweight visual gait, not foot-locking IK or guaranteed zero ground sliding.

All changes are presentation-only: simulation, collision/picking bounds, input ABI, combat and optional model selection are unchanged. No per-frame mesh/entity allocation or runtime AI is added.

## Verification

Tests cover overlapping rounded limb geometry, appearance bounds, actual spawned chest/head hierarchy, cache reuse and stable entity/mesh counts. A full-cycle sweep of both strafe directions and all four diagonals checks conservative lower-leg/boot bounds against each leg's own lane, including composed hip and knee rotation. Additional tests cover chest breathing, head counter-rotation and elbow flexion, alongside existing movement/reset/jump regressions.

Run `./scripts/check.sh`, WASM Clippy with warnings denied and `./scripts/build-web.sh`. Human review must check front/side/rear silhouettes and seams, hands/thumbs, Low/High, all movement directions and reversals, blocked movement, idle transitions, standing/moving jumps, landing/reset/tab resume, back-weapon clearance, picking and optional-model fallback. Automated checks do not establish rendered appearance or GPU performance.

Local verification on 2026-11-08: `./scripts/check.sh` passed (70 client, 13 simulation, 9 Python, 28 Node input and 166 Vitest tests, plus lint/type/format checks). WASM Clippy with warnings denied, the complete web build/static-resource check (using procedural characters) and the scripted headless scenario passed. No browser was launched; human rendered review remains pending.

## Update — 2026-11-14: slimmer build

At the user's request ("looks a bit heavy"), the procedural body is narrower while height, head, facial features, leg spacing (hips at ±0.155 m) and back weapon mounts are unchanged. The torso goes from 0.65 × 0.39 m to 0.56 × 0.36 m (width × depth), the hips from 0.48 to 0.44 m wide, and the neck, shoulders and all arm/leg segments are 12–20% thinner. The shoulder pivot moves in from ±0.30 to ±0.27 m and the elbow chain from ±0.38 to ±0.35 m, which narrows the arm span from 0.93 to 0.84 m. Hanging hands keep about 3 cm of clearance from the thighs. A new test pins these limits; the joint-overlap and strafe-lane tests pass with the thinner lower-leg bounds. The torso's back surface moves forward by 1.5 cm, so check the gap to both back weapons. Human rendered review is pending.
