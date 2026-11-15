# ADR 0034: Ranger presentation and authored jump edges

Date: 2026-11-14. **Status: accepted for local integration**, at the user's
request for no foot circle, jump start/land from the supplied animation packs,
character features and a basic armor set. Human browser review is pending.

Follow-up: [ADR 0035](0035-character-startup-budget.md) bounds textures and
compacts the model after the user's entry-hitch report, lengthens takeoff/
landing to 0.30/0.80 s and temporarily removes equipped weapons. The shorter
timings and source-resolution payload measurements below describe history.

## Decision

Extend the existing CC0 Quaternius female, not the simulation:

- Remove the persistent dark foot discs and the world-space gold target ring.
  HUD frame/nameplate selection feedback remains.
- Keep source eyes and brows; add the supplied rigged `Hair_Buns`.
- Use the included `Female_Ranger` torso/belts, arms/bracers, shoulder armor,
  legs and boots. Omit the hood to show hair/face. Remove covered body triangles
  from the base mesh's draw indices, preserving head/neck components. This
  retains unused source vertex bytes, not an optimized/decimated character.
- Merge clothing/hair meshes onto the base's one skeleton/skin only after
  checking joint order, hierarchy, local rest transforms and inverse bind
  matrices. Rebase buffer/accessor/material/texture indices and embed images.
  Deduplicate shared PNG bytes. Source downloads are never overwritten.
- Append the **actual included UAL1 `Jump_Start` and `Jump_Land` clips** after
  the original eight canonical animations. Legacy local characters still load
  eight graph nodes. The dressed Quaternius profile loads ten.

Takeoff uses the source `Jump_Start` 0–0.35 s segment during the first 0.12 s of
actual rule flight, then transitions to `Jump_Loop`. There is no pre-jump
gameplay windup or delayed movement. After visual touchdown, play the complete
`Jump_Land` 0–38/30 s impact/recovery over 0.28 s. Short crossfades and deterministic
pose seeking align these cosmetic phases to flight and render samples.

Preserve the source in-place crouch/recovery lowering of the pelvis; clamp
positive pelvis elevation in all three jump clips so it cannot add a second
world-height arc. The actor root alone supplies rule-owned altitude. The
landing pose never prevents movement, another legal jump, or a successful
Pulse; new jumps/casts override recovery. Reset, teleports and background gaps
clear stale phases. A rig first loaded midair enters the airborne loop rather
than replaying takeoff.

No character customization UI, armor stats, new ability, root-motion gameplay
or combat-rule change is introduced. Existing backward/strafe substitutions
remain as documented in ADR 0033.

## Provenance and verification

All source additions are from the already supplied **CC0** base/outfit/UAL1 packs,
by Quaternius. No QAL race asset is used or shipped. See the
[asset inventory](../../assets/quaternius/README.md).

Synthetic Python tests cover head/body triangle exclusion, skin/material/
accessor rebasing, mismatched equipment rest-pose rejection and canonical clip
order, along with prior retargeting tests. Rust tests cover start→loop→land→idle,
phase seek bounds, rule-owned height, no loading/reset replay and landing
interruption by another jump or successful cast. The optional real-Bevy-loader
test checks the clothing/hair/face nodes and included jump clip names.

Run `./scripts/check.sh`, WASM Clippy, the complete web build, the real-loader
test and headless scenario. Human review must check armor/skin seams, hair/face,
no foot/selection circles, all movement, takeoff/landing, jump spam and
movement/casts during recovery, reset/tab resume, both back weapons and browser
console errors. Build/loading checks are not a rendered or performance pass.

Local verification on 2026-11-14 passed: `./scripts/check.sh` (93 client tests,
17 simulation tests, 6 texture tests, 21 Python tests, 28 Node tests and 166
Vitest tests, plus formatting/lint/type checks), WASM Clippy with warnings
denied, the full web build with 16 static HTTP/MIME resources, the explicitly
invoked real-Bevy-loader test and the headless scenario. The normal client
suite ignores the local-asset loader test; it was run separately and passed.

The produced model has 13 drawn skinned nodes, one 65-joint skin, ten named
clips, no hood node, embedded PNGs, and only head/neck triangles in the base
body draw indices (3,164 triangles, lowest Y 1.467415 m). The prepared and
production GLBs match byte for byte. The model is 57,690,364 bytes with
source-resolution textures; bandwidth/texture optimization and device
performance are not verified. No browser was launched. Human rendered review
remains pending.
