# ADR 0058: Cast buffer

Date: 2026-11-26. **Status: proposed (implemented; `PROTOCOL_VERSION = 11`,
`RULES_VERSION = 8`)**, at the user's request.

Builds on [ADR 0048](0048-server-authoritative-casting-and-resources.md)
(server-authoritative casting, GCD and its 2026-11-26 refund amendment) and
[ADR 0049](0049-bounded-targeting-lag-comp-and-adaptive-interp.md)
(bounded lag compensation).

## Context

Casting is server-authoritative and unpredicted. A player who presses their
next spell the moment their GCD or cast ends on screen is already late: the
client shows state about half a round trip old, and the press takes another
half round trip to reach the server. At 100 ms RTT every chained spell loses
about 100 ms, and the HUD rejects presses made earlier. Rotations get gaps,
and damage and healing per second fall with latency.

The user asked for a configurable spell queue that holds a spell pressed
shortly before the current cast or GCD finishes, under a name of its own.

## Decision

The feature is called the **cast buffer**.

- **Server-side, not client-side.** A queue kept in the client still has to
  wait for the server's snapshot and then send, paying the full round trip.
  The server holds the press and fires it on the exact tick the wait ends,
  so a buffered spell starts with no gap at any latency inside the window.
- **Rule.** In phase 4b, a `Cast` intent from a living fighter whose active
  cast, GCD (for on-GCD spells) or the spell's own cooldown still blocks it,
  and whose wait ends within the sender's buffer, is stored as
  `Combatant::buffered_cast` instead of being validated.
  `Combatant::ticks_until_free` measures the wait from 4b: an active cast
  with `r` remaining ticks completes `r` ticks later and frees that tick's
  intents; a GCD or cooldown of `g` reaches zero `g` ticks later. The own
  cooldown matters because it can outlast the GCD: Pulse's is 30 ticks
  against a 29-tick GCD, so a Pulse held only for the GCD failed as
  "Recharging" one tick early (found in the first playtest).
- **Firing.** On a tick with no new `Cast` from that fighter, a held spell
  whose wait is over goes through `try_cast` exactly like a fresh press with
  no lag compensation (it is aimed at the present). Every check (target,
  range, line of sight, movement, lockout, costs) happens then; a failure is
  an ordinary `Rejected` event. A spell held while
  Rift Bolt or Mend is casting starts on that cast's completion tick.
- **Latest press wins.** Any new `Cast` replaces the held spell: it is then
  held itself, cast at once, or rejected. One combat action per tick still
  holds, so a held spell never fires on a tick with a new press.
- **Dropped** by `CancelCast` (which silently clears a held spell when
  nothing is being cast), by any early end of the active cast (cancel,
  movement, failed re-check, interrupt; through `end_cast_early`), and by
  death.
- **Window.** Each `InputFrame` carries `cast_buffer` in ticks; the rules cap
  it at `MAX_CAST_BUFFER_TICKS = 12` (400 ms at 30 Hz), the genre's usual
  ceiling. Carrying it in the input keeps replays deterministic and needs no
  session state; `0` keeps the old behaviour. Bots send `0`.
- **Snapshots.** `FighterState.buffered_cast` mirrors the held spell, and
  `Arena::hash_state` covers it, so client hash checks and prediction
  restores stay exact. The bandwidth-measurement delta codec gains
  `MASK_BUFFERED_CAST`.
- **Setting.** Settings → Action bars → **Cast buffer**: 0–400 ms in 50 ms
  steps, default 400 ms, saved per profile (`castBufferMs`). The shell sends
  it in milliseconds as input ABI field 22; the client rounds it to ticks.
- **HUD.** A press the HUD cannot cast now still goes to the server when the
  buffer is on and the wait it sees (cast, GCD or the spell's cooldown) is
  within the window plus 500 ms of slack for its older view. The server
  judges the window against its own, newer timers; a press it finds too
  early is rejected as before, one round trip later. The slot of a held spell is outlined; the cast-error area keeps
  watching a held spell and shows a fire-time rejection when it fails.

## Consequences

- At any latency up to about the window, chained spells start on the tick
  the previous GCD or cast ends, as they would on a zero-latency
  connection.
- A held stationary spell fails with "Stand still to cast" if the player is
  still moving when it fires; players can stop moving a moment early.
- Pressing a spell during the GCD while out of range is held, then rejected
  when it fires instead of at once.
- `PROTOCOL_VERSION` 10 → 11 (`InputFrame.cast_buffer`,
  `FighterState.buffered_cast`); `RULES_VERSION` 7 → 8.
- The hub ([ADR 0057](0057-hub-on-the-world-server.md)) runs the same rules,
  so the buffer works there too.

## Alternatives considered

- **Client-side queue:** simpler, no protocol change, but it removes none of
  the latency, which is the point.
- **Server-wide fixed window:** not configurable, which the user asked for.
- **Validating at press time and holding only GCD or cast rejections:** the
  rules check movement and lockout before the GCD, so a stationary spell
  pressed while still moving would be rejected instead of held.
