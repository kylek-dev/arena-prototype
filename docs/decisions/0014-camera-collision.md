# ADR 0014: Collision-aware orbit camera

Date: 2026-11-07. **Status: accepted for the practice prototype; human browser review pending.**

The presentation camera collides at the eye, not along the entire player-to-eye
sight line. Static map cube spawns supply bounds
from the same transforms as the visible floor, foundation, arena rims, pillar
bodies, bases, caps and accents. Decorative floor seams are not blockers.
This is client presentation only; simulation collision and authority are unchanged.

Use a dependency-free slab intersection against axis-aligned boxes expanded by
0.3 metres, with a 0.01 metre contact gap. The padding keeps the eye and near-plane
region away from surfaces, including grazing pillar edges. A pillar may sit between
the player and a sufficiently distant camera without forcing zoom-in. If the
requested eye overlaps a box, shorten the boom to the box's near face; repeat for
overlapping contact boxes. The floor/foundation additionally constrain the boom
to prevent low-angle requests from placing the eye entirely below the map.

Sweep the previous actual eye toward the resolved orbit position each frame,
clipping remaining displacement along contact planes (up to three contacts).
This permits sliding while preventing fast orbit/zoom movement from tunneling
through a pillar. Approaching either side stops on that side; go around or over
the pillar to reach the other side rather than passing through it. Reset discards
the previous eye so it cannot sweep from the old practice-round position.

Retraction is immediate; recovery uses exponential easing at 5/s, always bounded
by contact constraints and the eye-motion sweep. There is no unchecked position interpolation through obstacles.
Orbit yaw/pitch remain responsive while constrained, letting the eye move along
surfaces and around corners. Requested wheel zoom (6–22 metres) remains separate
from the resolved eye, which may retract below 6 metres. Clearing an obstacle
restores the requested distance, not a hard-coded default. Pitch now allows
−1.2 to +1.2 radians, with collision rather than the old positive-only pitch limit
keeping the camera above the floor. Reset restores the default requested distance.

Bounds are deliberately conservative at corners. This is not general mesh physics:
rotated, moving or non-box map geometry would need a corresponding collider update.
Actors, effects and imported character meshes do not block the camera.

Tests cover distant occluders, floor contact, pillar grazing, overlapping-box ordering, parallel rays,
focus-inside handling, frame-rate-independent recovery, newly encountered blockers,
wheel zoom retraction, two-sided motion collision, sliding around pillars and
the presentation system's actual transform/zoom recovery.
Rendered feel, very close character views and intended viewport sizes still require
human browser review; automated checks do not establish appearance or GPU behavior.

The initial full-boom implementation passed local checks/builds. The user reported
it mostly worked but clarified that distant pillars should be allowed to obscure
the player. This contact-only revision replaces that line-of-sight constraint;
human rendered review of the revision remains pending.

Contact-only revision verification (2026-11-07): `./scripts/check.sh`, WASM Clippy,
`./scripts/build-web.sh` (procedural fallback; optional model not installed), and
`cargo run -p arena-headless` passed. No browser/E2E pass is claimed for this revision.

## Human review update — 2026-11-08

The user verified and approved all implemented UI/rendered changes, including the current contact-only camera revision. Historical pending human-review statements above are superseded; earlier feedback is retained as history. See [review record](../UI-REVIEW.md).
