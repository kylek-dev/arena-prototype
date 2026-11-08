# ADR 0015: Configurable-ready procedural humanoids

Date: 2026-11-07. **Status: accepted for the prototype**, at the user's request for more human-like models and room for later configuration. Human rendered review is pending.

## Decision

Replace the public procedural capsule characters with original, stylized humanoids assembled from cached capsule and sphere meshes. Both practice actors have a torso, paired arms/hands, legs/boots, ears, nose, mouth, eyebrows, eye whites, irises, pupils and hair. Two initial appearance presets distinguish skin, hair, eyes, hairstyle and build; clothing retains the existing actor accent colors.

Keep `Appearance` and the builder in `crates/arena-client/src/humanoid.rs`, separate from simulation identity and rules. Appearance describes colors, clothing material, height/build scale and a typed hairstyle. These are internal presentation settings, not a stable serialized format. A character editor, saved customization, network appearance validation, animation and production character art are deferred. Limbs are distinct geometry, not an animated skeleton.

Follow-up: [ADR 0016](0016-procedural-locomotion.md) adds a lightweight articulated hierarchy and procedural movement animation; customization UI/persistence and production skeletal assets remain deferred.

Reuse the existing Low/High mesh cache and quality switching for rounded parts. All visual geometry is parented to the fighter, so movement, facing, jumping and reset continue through the existing root transform. Hide the complete player humanoid subtree when an optional imported model becomes ready; failures retain it. The imported asset itself is unchanged. Existing cosmetic back weapons remain attached separately. Neither appearances nor model detail change combat, collision, selection bounds, camera rules or the input bridge.

## Verification

Unit tests check paired limbs/eyes, facial direction, positive part dimensions and grounded height bounds for both hairstyles. Existing graphics tests check Low/High geometry bounds. Run `./scripts/check.sh`, WASM Clippy and the complete web build. Manual acceptance must review front/side/back silhouettes, facial readability, hair, back-weapon fit, both quality settings, movement/jumping/reset, targeting/hover and optional-model success/fallback. No rendered or performance pass is inferred from automated checks.

Local verification on 2026-11-07: `./scripts/check.sh` passed (12 simulation tests, 27 client tests, 26 Node input tests and 122 Vitest tests), WASM Clippy with warnings denied passed, and `./scripts/build-web.sh` passed using the procedural characters (optional model not installed). The scripted headless scenario completed successfully. A renderer-free hierarchy test also checks that the player's complete fallback has one visibility root and every humanoid part uses the shared quality cache. No browser was launched; human rendered review remains pending.

## Human review update — 2026-11-08

The user verified and approved all implemented UI/rendered changes, including character presentation. Historical pending human-review statements above are superseded. See [review record](../UI-REVIEW.md).
