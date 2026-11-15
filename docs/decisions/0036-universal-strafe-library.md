# ADR 0036: Shared Blender-authored lateral animation library

Date: 2026-11-14. **Status: accepted for local integration**, at the user's
request to create missing left/right strafes through Blender MCP, make them
universal/shareable across bases, races and outfits, and use neutral names.
Human browser review is pending.

Follow-up: [ADR 0038](0038-matched-locomotion-and-jump-transitions.md) replaces
these slow walking side-steps with jog-cadence lateral run loops authored by
`scripts/author_universal_strafes.py`. The 0.8 s timing, file size and Blender
script described below are history.

## Decision

Author on the **CC0 UAL1 universal mannequin**, not the current female mesh.
Export `assets/animations/universal-strafes.glb` with two 0.8 s loops, the common
65-joint skeleton and a tiny weighted carrier. Include no character/outfit/race
meshes, images or scene props. Preserve earlier Blender scenes; keep the
authoring and preview scenes separate, restoring the original active scene
after inspection.

Use offline analytic two-bone leg posing to author outward lead/trailing-close
steps with grounded support, lifted swing feet, bent knees and small body/arm
counter-motion. Scale gait distances to reference leg lengths; keep root
translation and heading fixed. Bake standard glTF keys so no new runtime
animation/IK dependency is required.

The existing adapter reads the separate library for canonical `Strafe_Left`
and `Strafe_Right`, preserving runtime indices 3/4 and all other clips. Fail
clearly if the authored library is missing rather than silently falling back
to forward jogging. Retarget by names, hierarchy, local rest rotations and
pelvis proportions, preserving target bone lengths. Blender's constant `STEP`
tracks are accepted only when every key is identical; animated step tracks
remain rejected. Runtime output remains linear.

The reference skeleton/idle baseline retain Quaternius CC0 provenance. New
curves/carrier and new original authoring code are MIT, with a separate notice
for the shareable library and a copied notice in production builds. No QAL race
data is exported or relicensed. See [provenance and reuse](../../assets/animations/README.md).

## Scope and limitations

“Universal” means the verified common Quaternius humanoid rig, not arbitrary
humanoid/animal skeletons. Clothing fit remains separate from animation
compatibility. No customization UI, race selector, multiplayer rule, ability,
root-motion authority or physics change is introduced. Backward still reverses
the forward cycle; weapons remain temporarily unequipped.

## Verification

The standalone asset is 119,272 bytes, one scene/skin/three-vertex carrier and
two named clips, with no images/materials or unrelated objects. Tests check
closed pose loops, fixed root, alternating support/swing and uncrossed feet.
Synthetic adapter tests check selecting separate libraries in canonical order,
missing-library failure, distinct directions, constant-key interpolation and
existing retargeting boundaries.

`python3 scripts/check_animation_compatibility.py` passed both clips on ten
installed sources: two human bases, four elf/orc models and four complete
Ranger/Peasant outfits. All match the shared hierarchy, preserve root/facing
and close their loops without crossed feet. Contact-height deviations after
proportion-only retargeting ranged up to about 2.3 cm; no model-perfect foot
planting or rendered approval of every source is claimed.

Blender inspections covered the source mannequin and static retargeted poses
on the dressed female. `./scripts/check.sh`, WASM Clippy with warnings denied,
the full web build (17 static HTTP/MIME resources including the license notice)
and the explicitly run real-Bevy-loader test passed. The real-loader test also
requires both strafe clips' 0.8 s duration. Prepared and production character
GLBs match; the resulting player GLB is 9,131,324 bytes.

No browser was launched. Human review must check A/D and diagonals, rapid
direction changes, idle/wall contact, steering, jump/landing/cast transitions,
armor/body intersections and intended device frame pacing. In-place clips do
not guarantee zero foot sliding at every game speed.
