# ADR 0013: Momentum-based jumping

Date: 2026-11-07. **Status: accepted for the practice prototype; human browser review pending.**

## Rules

Space is the default remappable, edge-triggered Jump action. Holding it does not
repeat jumps or queue another jump on landing. Existing profiles gain the default
only if Space is not already assigned; explicit unbindings are preserved.

The pure 30 Hz integer simulation owns height, vertical velocity, gravity and
horizontal takeoff momentum. Initial upward speed is 220 mm/tick and gravity is
20 mm/tick²: a 1.32 m apex and 23 ticks (about 0.77 s) to land. Ground movement
keeps its existing speed and responsiveness. Jumping carries the actual bounded
ground displacement, not an extra speed boost. There is no air steering, braking,
double-jumping or accumulated bunny-hop acceleration. A standing jump stays put.

Existing airborne momentum advances once before ordered commands. Ground momentum
is cleared each tick, then Move records actual displacement. The client submits
Move, Jump, Attack in that order, so moving and jumping together work immediately.
Airborne Move commands are validated/budgeted but do not alter momentum. Jump
rejects invalid actors, airborne actors and completed matches. Vertical integration
runs after commands; landing clamps height/vertical velocity to zero. Terminal
rounds freeze physics. Reset constructs fresh state. Replay uses the same rules.

Walls and pillars retain their full planar blocking footprint at every height;
attack range and line of sight also remain planar. This is not a general 3D
physics or obstacle-vaulting system. Jumping does not evade Pulse or bypass cover.
Bevy interpolates rule-owned height along with horizontal position, for both the
optional model and procedural character. The existing follow camera follows it.
The local input ABI appends Jump at index 14; indices 0–13 are unchanged and old
shells default to no jump. No network authority is implemented by this change.

## Verification

Rust regressions cover the arc, exact landing, momentum, repeated jumps, speed,
collision, invalid actors, terminal rounds, replay and presentation height.
JavaScript tests cover ABI draining, repeat suppression, remapping, focus/menu
isolation and additive profile migration. Rendered feel, models and camera require
manual browser review; automated tests cannot establish that pass.

Local verification (2026-11-07): `./scripts/check.sh`, WASM Clippy,
`./scripts/build-web.sh` (procedural fallback; optional model not installed), and
`cargo run -p arena-headless` passed. No browser/E2E or rendered pass is claimed.

## Human review update — 2026-11-08

The user verified and approved all implemented UI/rendered changes, including jump presentation. Historical pending human-review statements above are superseded. See [review record](../UI-REVIEW.md).
