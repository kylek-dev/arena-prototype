# ADR 0038: Speed-matched lateral run and smoother locomotion/jump transitions

Date: 2026-11-14. **Status: accepted for local integration**, at the user's
request after reviewing the Quaternius female in the browser. Human browser
review of this change is pending.

## Problem (reported by the user)

1. Strafes were too subtle: the body moved much faster than the small, slow
   steps, so it looked like sliding. Measured: the ADR 0036 side-step's
   planted feet moved about 0.5 m/s at clip speed, while rules move every
   direction at 7.2 m/s and the clip played at most 2.0×.
2. Holding forward and a strafe key alternated between the jog and strafe
   clips. A 45° diagonal tied the `|y| >= |x|` choice, so float noise flipped
   it, and each switch restarted the new loop from frame 0.
3. `Jump_Start` handed over to `Jump_Loop` mid-takeoff. Only its first 0.35 s
   played, but the 1.33 s clip only ends on the loop's first pose.
4. Landing while moving played the planted 0.8 s `Jump_Land` recovery while
   the body kept running, so the character slid.

## Decision

- **Re-author both strafes as a lateral run** with
  `scripts/author_universal_strafes.py` (standard-library Python), replacing
  the Blender-authored walk in the shared universal library. The new loops use
  the jog's 28-frame cycle, its footfall frames (left planted 1–5, right 15–19)
  and a similar short contact. Each planted foot sweeps 0.45 m across the
  body, so at the client's unchanged rate rule (2.0× at 7.2 m/s) planted feet
  move with the ground. Arms take the jog's own pumping rotations, so the
  style matches. Retargeting, skeleton, carrier, licenses and canonical
  indices 3/4 are unchanged. See the [library README](../../assets/animations/README.md).
- **Diagonals strafe.** Any clear sideways component plays the strafe clip,
  so forward/back + strafe keys always strafe, whichever key was pressed
  first. Strafe starts above a sideways/forward ratio of 0.45 (about 24°) and
  ends below 0.35 (about 19°). In between the current clip is kept, so jitter
  cannot alternate clips. (The first version kept whichever clip was already
  playing at 45°. The user found that inconsistent and preferred the strafe.)
- **Phase carry.** Profiles declare `locomotion_cycle` when their loops are
  frame-aligned (only the Quaternius female). Switching between Forward, Left
  and Right seeks the new loop to the outgoing loop's time. Backward maps
  through the reversed cycle. Crossfades then blend matching steps.
- **Complete takeoff.** The whole `Jump_Start` (0–40/30 s) plays over the first
  70% of rule flight (about 2.5× speed). `Jump_Loop` then continues from its
  first pose in real time until touchdown. Rule takeoff is still immediate.
  Profiles without jump edges keep the flight-spanning loop seek.
- **Moving landings.** Whenever ground movement is detected, any pending
  landing recovery is cleared. Running through touchdown crossfades from the
  airborne pose into the gait, and starting to move during a standing recovery
  ends it for good. Standing landings still play the full 0.8 s recovery.

The forward jog's playback rate is unchanged, because the user approved its
look. Crude contact measurement suggests its planted feet move faster than the
ground at 2.0× (zero slide would be about 1.15×). Tuning that would change the
approved cadence, so it is not done here.

## Verification

Rust tests cover every diagonal strafing regardless of key order, jitter at
45° and at the boundary, near-straight drift staying forward, phase mapping for all aligned loops, a takeoff that ends
on the loop's first pose, moving touchdown never entering `Jump_Land`, and
standing recovery that movement ends and stopping does not resume. Python
tests cover the 28-frame length, footfall frames, planted-foot speed
(3.32 m/s), swing clearance, non-crossing feet, loop closure, fixed root and
byte-identical regeneration. `scripts/check_animation_compatibility.py`
passes on all ten supplied humanoid targets. On the built female, a 1.97× rate
gives zero slide at 7.2 m/s (the client uses 2.0×).

`./scripts/check.sh`, WASM Clippy with warnings denied, the real-Bevy-loader
test and the full web build passed locally. No browser was launched. Poses were
checked with numeric dumps and stick-figure renders, not in Blender or the game.
Human review should cover A/D at full speed, all diagonals, rapid
forward↔strafe switching, standing and running jumps, landing into a run,
starting to move during a standing recovery, and jump spam.
