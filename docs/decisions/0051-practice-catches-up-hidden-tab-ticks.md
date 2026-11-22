# ADR 0051: Practice mode catches up on ticks missed while the tab is hidden

Date: 2026-11-22. **Status: proposed (implemented in `arena-client`; human browser check pending).**

## Context

Playtesting found that resource stops regenerating while the practice tab is
hidden. Online play was not tested at the time.

- **Online play keeps regenerating.** The match server advances every match on
  its own Tokio interval (`crates/arena-server/src/lib.rs`), and regeneration is
  derived from `Arena::tick` alone (ADR 0048), not from client input or
  visibility. A session test (`idle_seats_keep_regenerating_on_the_servers_clock`)
  now pins this: a seat that sends no input for 90 ticks (3 s) still
  regenerates on every third tick.
- **Practice pauses.** The practice round runs `arena-sim` inside the browser on
  Bevy's `FixedUpdate`. Bevy feeds `Time<Fixed>` from `Time<Virtual>`, which
  clamps each frame's delta to `max_delta` (250 ms by default, `bevy_time`
  0.20.1). Browsers are expected to stop delivering animation frames to hidden
  tabs, or throttle them heavily (not observed in this repository; part of the
  pending human check), so the practice sim either stops or runs at a fraction
  of real time. When the tab returns, the first frame advances at most 250 ms.

## Decision

Practice catches up on missed ticks, capped at **60 seconds** of ticks per gap.

- Each frame without a network session, once the startup overlay is ready,
  `practice_catch_up` (`crates/arena-client/src/lib.rs`, `PreUpdate`, chained
  just before `network_receive` so a frame that ends a match never replays a
  gap into the fresh practice round)
  compares real elapsed time (`Time<Real>` × the virtual clock's requested
  `relative_speed`) with the virtual time Bevy feeds to `FixedUpdate`. The
  difference is time the sim lost to the clamp. (Bevy's `effective_speed`
  already includes the clamp, so it cannot be used for this.) A paused virtual
  clock is never caught up. The loss is accumulated in `CatchUp`, keeping the
  sub-tick remainder, and paid back as whole ticks, at most
  `PRACTICE_CATCH_UP_MAX_TICKS = 60 × TICKS_PER_SECOND` (1,800) at once; any
  debt beyond the cap is dropped.
- The trigger is any frame longer than 250 ms, not only a hidden tab: a
  foreground stall, a very low frame rate or a debugger pause also catches up.
- Catch-up ticks use an idle input for the local player (no movement, no jump,
  no new cast, facing unchanged). Input listeners release held keys when the
  tab hides, so for a hidden tab this matches what the player was doing. In a
  foreground stall the player may still be holding a key; the replayed time is
  idle, so they cover the same ground as before this change while regen,
  cooldowns and the round clock keep wall-clock time. A cast already
  in progress still completes, and its events update combat text and feedback
  exactly like a normal tick.
- Network matches are unaffected: the server owns the clock there.

Why catch up rather than stay paused: practice is where players learn how fast
resource and cooldowns recover, and online play keeps the clock running while a
tab is hidden. Pausing practice would teach the wrong timing. The practice
dummies are idle, so catching up cannot hurt the player. The cap bounds the work
done in the one frame after the tab returns: the 4×2v2 server smoke run
averages about 13 µs per networked tick (`planning/ITERATION-LOG.md`), so even
1,800 local ticks of a practice round up to 3v3 should stay in the tens of
milliseconds (not measured in a browser). The cap is still
far longer than anything takes to recover in the current kit (a full resource
pool takes 10 s, the longest cooldown 6 s).

## Consequences

- Returning to a hidden practice tab shows regenerated resource and finished
  cooldowns, matching online behaviour.
- A practice round still ends at its 20-minute limit; catch-up ticks count
  toward it.
- Rendered and browser behaviour needs a human check (hide the tab for ~10 s in
  practice with a part-spent pool, then return).
