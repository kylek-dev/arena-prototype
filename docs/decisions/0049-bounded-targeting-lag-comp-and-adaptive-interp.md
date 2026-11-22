# ADR 0049: Bounded server-side targeting lag compensation and adaptive remote interpolation delay

Date: 2026-11-21. **Status: proposed (Lane A simulation, protocol, server, client networking and bot measurement implemented and verified).**

Builds on [ADR 0044](0044-authoritative-match-server.md), [ADR 0048](0048-server-authoritative-casting-and-resources.md), and the [2026-11-21 netcode measurements](../research/netcode-measurements-2026-11-21.md) for [`planning/issues/netcode-hardening.md`](../../planning/issues/netcode-hardening.md).

## Context

In the [2026-11-21 targeting-fairness measurements](../research/netcode-measurements-2026-11-21.md#2-targeting-fairness--lag-compensation-disagreement-rates) (20×2v2 = 80 bots, 25 s each on Colosseum at 30 Hz), the browser client predicted its own movement immediately (`Predictor`) while rendering remote fighters at a fixed `INTERPOLATION_DELAY = 3.0` ticks (`100 ms`) behind the newest received snapshot, and the server validated spell range and line of sight strictly on its present tick:

- Even at **0 ms RTT** with the fixed 100 ms remote buffer, **7.94%** (`49 / 617`) of casts reaching geometry validation failed with `OutOfRange` disagreements (versus **0.00%** at 0 ms interp), because 100 ms of remote staleness adds `720–1,440 mm` of relative separation at 7.2 m/s (`240 mm/tick`).
- Disagreements rose to **9.26%** at 50 ms RTT + 10 ms jitter, **9.07%** at 100 ms RTT + 20 ms jitter, **15.46%** at 200 ms RTT + 40 ms jitter, and **18.40%** under 100 ms RTT + 20 ms jitter + 2% simulated TCP loss + 40 ms relay clumping.

Two root causes compounded each other:

1. **Unnecessary remote staleness on low-jitter links:** A fixed 3-tick (`100 ms`) interpolation delay leaves ~55–60 ms of unused buffer on clean broadband/LAN links, pushing total target age (`RTT + interp + queue`) past 200 ms even at 100 ms RTT.
2. **Zero server-side target-position rewind:** Every cast-start range/LoS check ran strictly against present-tick target positions, rejecting legitimate casts pressed while the target was clearly in range and line of sight on the player's screen.

Naive flat range padding ("range leeway") was rejected because it causes permanent range creep against stationary targets and cannot help with pillar line-of-sight disagreements. Client-authored positions were rejected because server authority forbids trusting client coordinates.

## Decision

### 1. Adaptive remote-fighter interpolation delay (`arena_protocol::prediction::InterpDelay`)

Add a pure, clock-injected, deterministic `InterpDelay` estimator in `arena-protocol::prediction`, shared by `arena-client::network::Session` and `arena-bot` (`--adaptive-interp`):

- **Bounds:** Starts at `DEFAULT_INTERP_DELAY_TICKS = 3.0` (`100 ms`), relaxes down to `MIN_INTERP_DELAY_TICKS = 1.25` (`~41.7 ms`, one full tick plus a quarter-tick guard for smooth interpolation), and caps at `MAX_INTERP_DELAY_TICKS = 5.0` (`~166.7 ms`).
- **Clock-skew-invariant jitter window:** Records `(tick, now)` pairs for fresh (non-backlog, `<= FRESH_PER_FRAME`) running snapshots in a 60-sample (`~2 s`) sliding window, computing arrival spread `max(now - tick / rate) - min(now - tick / rate)` using the live `TickRate` estimate. Startup and background-tab bursts (`> FRESH_PER_FRAME` snapshots in one frame) are ignored as jitter evidence.
- **Asymmetric responsiveness:** Expands quickly (`alpha = 0.5`) when jitter, clumping, or stalls widen the arrival spread (`MIN + 1.15 * spread_ticks`), bumps by `+0.5` ticks at most once per distinct stalled `newest_tick` on view underruns (framerate-independent), and relaxes smoothly at `-0.025` ticks per fresh frame (`~0.75` ticks/s) when arrivals stabilize.

### 2. Bounded server-side target-position history rewind (`arena-sim` + `arena-server`)

- **Inline position history in `Arena` (`arena-sim`):** Maintain a zero-allocation, fixed-size ring buffer (`PositionHistory`, `6 × 6` `Position` entries = 288 bytes inline) storing each fighter's authoritative post-movement planar position for the last `MAX_LAG_COMP_TICKS = 6` ticks (`200 ms` at 30 Hz). Initialized with spawn positions in `Arena::practice`, pushed deterministically each running tick, and cleared on `Arena::restore`, `Arena::restore_in_place`, and `Arena::relayout`. `Arena::state_hash` and snapshot payloads do not change.
- **Per-slot `ActorInput.rewind_ticks: u8`:** Defaults to `0` (strict present-tick rules for offline practice, `arena-headless`, and legacy inputs) and is unconditionally clamped to `<= MAX_LAG_COMP_TICKS` inside `Arena::advance`.
- **Cast-start geometry validation (Phase 4b):**
  - Check present-tick `pre_combat` geometry first (so targets moving *into* range or stepping *out* from behind pillars always pass).
  - If `rewind_ticks > 0` and present-tick fails range or LoS while the target is alive, check the server-recorded target position at `clamped` ticks ago and (when `clamped > 1`) `clamped - 1` ticks ago — matching the `[floor(view_tick), ceil(view_tick)]` endpoints of client interpolation.
  - **Simultaneous range + LoS invariant:** A historical tick satisfies geometry only if **both** range and line of sight hold at that same historical tick from the caster's present authoritative position (`pre_combat[actor].position`). Range and LoS are never mixed across different ticks.
  - **Strict present-tick boundaries:** Target liveness (`health > 0`), team identity, resources, GCD, cooldowns, interrupt eligibility (`still_casting_after_4a`, preserving ADR 0048 fake-casting), and **timed cast completions (Phase 4a)** remain strictly present-tick so stationary targets never suffer range creep, dead targets cannot be hit, and pillar-kiting a 1.5 s `RiftBolt` or `Mend` stays crisp for defenders.

### 3. Monotonic, server-clamped `InputFrame.view_tick` (`PROTOCOL_VERSION = 4`, `RULES_VERSION = 5`)

- Add `view_tick: u64` to `InputFrame` (`0` = unspecified / present tick only; 1–3 bytes varint on the wire), bumping `PROTOCOL_VERSION` from `3` to `4` (in Rust and `web/src/domain/network.ts`) and `RULES_VERSION` from `4` to `5`.
- Clients (`arena-client::network::Session` and `arena-bot`) report the monotonic integer server tick of the remote fighters currently rendered in their view.
- `arena-server::session::Session` never trusts `view_tick` directly:
  - `view_tick == 0` yields `rewind_ticks = 0`.
  - Non-zero `view_tick` is clamped to `<= post_tick` (`arena.tick() + 1`, so future ticks yield `0` rewind and cannot poison state) and enforced monotonically non-decreasing per seat across the entire match (`effective = clamped.max(seat.max_view_tick)`, retained across reconnects to block backwards tick-shopping).
  - `rewind_ticks = post_tick.saturating_sub(effective).min(MAX_LAG_COMP_TICKS)`.
  - During backlog folding, `rewind_ticks` stays paired with the winning `WireAction::Cast` frame.

## Observed verification (2026-11-21)

Measured on release builds (`taskset -c 0-7` `arena-server --dev`, `taskset -c 8-15` `arena-bot`, 20×2v2 = 80 bots, 25 s per condition, Colosseum) against the exact 2026-11-21 baseline matrix:

| Condition | Baseline OOR Disagree (no lag comp, fixed 100 ms) | Lag Comp Only (fixed 100 ms interp) | Lag Comp + Adaptive Interp (`p50/p99` ms) | `hash_mismatches` / Overruns |
| --- | --- | --- | --- | --- |
| 0 ms RTT | 7.94% (49/617) | 0.35% (2/575) | **0.18%** (1/563, `44/100` ms) | 0 / 0 |
| 50 ms RTT + 10 ms jit | 9.26% (54/583) | 0.19% (1/537) | **0.57%** (3/529, `54/100` ms) | 0 / 0 |
| 100 ms RTT + 20 ms jit (0 ms interp) | 1.89% (10/528) | **0.00%** (0/505) | — | 0 / 0 |
| 100 ms RTT + 20 ms jit | 9.07% (50/551) | 2.85% (15/526) | **0.19%** (1/517, `65/100` ms) | 0 / 0 |
| 200 ms RTT + 40 ms jit | 15.46% (79/511) | 8.63% (41/475) | **6.42%** (30/467, `87/100` ms) | 0 / 0 |
| 100 ms RTT + 20 ms jit + 2% loss + 40 ms clump | 18.40% (99/538) | 13.65% (67/491) | **8.12%** (39/480, `100/167` ms) | 0 / 0 |

Key observations:

- Combining adaptive interpolation with the 200 ms server rewind cap cut `OutOfRange` disagreements by **93.8–97.9%** across 0–100 ms RTT (to **0.18–0.57%**) and by **55.9–58.5%** under 200 ms RTT and 2% TCP loss + 40 ms clumping (where the hard 200 ms cap intentionally protects defenders).
- Mean server tick time stayed at **2.2–3.3 µs** with **0 tick overruns** and **0 hash mismatches** across all 12 scenarios.
- Eliminating first-attempt `OutOfRange` rejections prevents repeated cast-spam cascades while a target flees, which explains why `casts_saved_by_lag_comp` (8–11 per run) eliminates 40–50 downstream disagreements.

## Consequences

- `PROTOCOL_VERSION` is now `4` and `RULES_VERSION` is now `5`; older clients receive `VersionMismatch` cleanly at `Hello`.
- Low-jitter players render remote fighters ~55 ms fresher (~42–55 ms vs 100 ms), while jittery/lossy connections expand smoothly up to 167 ms to absorb clumping and TCP stalls.
- Defenders retain crisp pillar-kiting on timed cast completions and a strict 200 ms ceiling (≤ 1.44 m along real server-recorded trajectories) on cast starts, with zero range creep against stationary targets and zero client coordinate trust.
- Real human playtests should verify the 200 ms (`6`-tick) ceiling and adaptive decay feel around Colosseum pillars before marking this ADR `Accepted`.
