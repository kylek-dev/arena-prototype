# ADR 0050: Live spectating and anti-ghosting delay

Date: 2026-11-22. **Status: proposed (server, protocol, bot measurements, and browser spectator client implemented; pending human browser review).**

Builds on [ADR 0044](0044-authoritative-match-server.md), [ADR 0048](0048-server-authoritative-casting-and-resources.md), and private custom lobbies (`planning/issues/custom-matches.md`) to satisfy [`planning/issues/spectating.md`](../../planning/issues/spectating.md).

## Context

Players in private custom matches (and future live match lists) need to watch matches without occupying a combatant seat. Spectating introduces three integrity and operational risks that must be prevented server-side:

1. **Privilege confusion:** A spectator ticket must never be usable as a player seat credential, nor may a spectator send inputs that move, target, or cast on any fighter.
2. **Ghosting:** In a 3v3 arena with pillar line-of-sight, 1.5 s timed casts (`RiftBolt` / `Mend`), and a 10 s interrupt (`Quell`) / 2 s lockout window, a zero-delay spectator on voice chat can call out pillar repositions, fake-casts, and cooldown states in real time.
3. **Fanout cost & privacy:** Spectators need full public combat state for every fighter (health, resource, active cast, lockout, cooldowns, appearance, round clock, and post-match scoreboard), without leaking any player's private `ArenaEvent::Rejected` feedback or multiplying per-tick serialization work.

## Decision

### 1. Signed `Role` claim isolation (`arena-server::ticket`)

- Replace `Claims.seat: u8` with `Claims.role: Role` (`Role::Player { seat: u8 } | Role::Spectator`) inside the HMAC-SHA256-signed ticket payload.
- Player and spectator tickets are structurally non-interchangeable: `Claims::seat()` returns `None` for spectators, `Claims::is_playable()` returns `false` for spectators, and `Claims::is_valid()` rejects spectator tickets whenever `settings.allow_spectators == false`.
- Add `allow_spectators: bool` (default `true`) to versioned `MatchSettings` (`version = 1`), configurable by the custom lobby host (`set_lobby_settings`) and enforced both in the front door (`Matchmaker`) and inside `arena-server::App::match_for` when validating tickets.

### 2. Protocol v7 watch-only handshake (`arena-protocol`)

- Bump `PROTOCOL_VERSION` from `6` to `7` (`RULES_VERSION` stays `5`).
- Add `spectator: bool` to `ServerMessage::Welcome` (for spectators, `actor` MUST be `0`; decoded and validated strictly by `valid_welcome`).
- Add `WireEvent::for_spectator(&ArenaEvent) -> Option<Self>`, which strips private `ArenaEvent::Rejected` events unconditionally while forwarding all public combat events (`Jumped`, `CastStarted`, `CastCompleted`, `CastInterrupted`, `Hit`, `Healed`, `Finished`).
- In `arena-server`, spectator connections strictly ignore all `ClientMessage::Input(_)` frames (keeping `RateWindow` ingress accounting and `Ping`/`Pong` active) and never mark any seat joined or lock any fighter appearance.

### 3. Anti-ghosting delay: **5 seconds (`150` ticks at 30 Hz)**

- Set `DEFAULT_SPECTATOR_DELAY = 5 * TICKS_PER_SECOND` (`150` ticks = **5.0 seconds**), configurable via `--spectator-delay-seconds` (`0..=60`).
- **Rationale:** 5 seconds exceeds the full duration of a 1.5 s timed cast, a 2.0 s interrupt lockout, and short pillar-kite transitions — neutralizing real-time voice-call ghosting of fake-casts, interrupts, and surprise peeks — while keeping custom-lobby watch parties responsive enough that spectators see the end of a round shortly after players finish.
- **Zero-leak semantics:**
  - While `Phase::Waiting` (`snapshot.tick == 0`), waiting attendance snapshots deliver immediately so spectators see players loading in.
  - Once live combat starts (`snapshot.tick >= 1`), every tick snapshot is enqueued for release at `clock + spectator_delay`.
  - Mid-match spectator joins or reconnects NEVER receive the live `self.arena` snapshot; they receive only the latest already-eligible delayed snapshot (if one exists) or wait until the first delayed frame matures. `Welcome.tick` reports that delayed tick (or `0`), never the live tick.
  - When the match enters `Phase::Finished`, seated players still close after `linger` (5 s), while the match session keeps draining remaining queued delayed snapshots (including the final `result` and `scoreboard`) to connected spectators for up to `spectator_delay + linger` before closing them with `MatchOver`.

### 4. Caps, limits, and zero-copy fanout

- Default per-match spectator cap: `DEFAULT_MAX_SPECTATORS = 4` (configurable via `--max-spectators` `0..=64`), matching the 4 custom-lobby spectator slots. Distinct spectator connections past the cap receive `ErrorCode::MatchFull`; reconnecting with the same spectator `player` id replaces the older connection (`ErrorCode::Replaced`).
- Existing per-address open-connection limits (`--connections-per-address`) and per-connection ingress rate limits (`RateWindow`) apply identically to spectators.
- Each eligible delayed snapshot is serialized **once** per tick into `Arc<[u8]>` (`EncodedOut::SharedFrame`) and sent zero-copy via `axum::body::Bytes::from_owner`, avoiding per-spectator serialization overhead.
- Expose `arena_spectators_open` on `/metrics`.

### 5. Watch-only spectator client (`arena-client` & `web`)

- `arena-client::network::Session` records `Welcome.spectator`, keeps server actor order `0..count`, sets `joined() == false` (so `input_action` never emits `ClientMessage::Input`), and publishes `"spectating": true` and `"spectatorFollow": number | null` on engine snapshots along with all fighter nameplates (including actor 0).
- `SpectatorCamera` provides both **Free camera** (WASD fly, Space rise, drag look, wheel speed/dolly, clamped inside the venue stands) and **Follow-a-player** orbit camera (click any Team A / Team B unit frame, overhead nameplate, or **Prev** / **Next** controls).
- The browser HUD mounts `SpectatorHud` (Team A / Team B columns with full `UnitFrame` health, resources, cast bars, lockouts, and disconnected indicators + bottom camera control bar) instead of combat `Frames`, `ActionBars`, `Spellbook`, and targeting controls. `commands.ts` blocks all combat, targeting, focus, spellbook, layout, and quick-bind actions while spectating, and `RoundClock` / `Scoreboard` render neutral `Team A` / `Team B` labels without a `"You"` highlight or public requeue button (retaining **Return to lobby** in custom lobbies).

## Observed Cost (2026-11-22)

Measured with isolated release builds (`25 × 3v3` = 150 active combat bots + `0`/`25`/`50`/`100` spectators, see [`docs/research/spectator-cost-2026-11-22.md`](../research/spectator-cost-2026-11-22.md)):

- **CPU:** `+0.04` cores for `+100` spectators (`~0.4` millicores/spectator; tick mean `5.1 µs` → `7.8 µs`, `0` overruns). Zero-copy sharing lowers total CPU per snapshot sent from `30.8 µs` to `24.8 µs`.
- **Memory:** `+13.5 MiB` peak RSS for `+100` spectators (`~138 KiB`/spectator connection; delay buffer `~24 KiB`/match shared across watchers).
- **Bandwidth:** `~37.5 kbit/s` steady-state 30 Hz 3v3 outbound payload per spectator (`0` private events leaked across `29,924` snapshots, `0` hash mismatches across `72,630` checks).

## Consequences

- **Positive:** Spectators can watch live custom matches with full cast/resource readability and both free/follow cameras at negligible server cost, with strong cryptographic and tick-delay boundaries against impersonation and ghosting.
- **Tradeoff:** Keeping a finished match task alive up to `spectator_delay + linger` (10 s default) holds ~24 KiB of delayed frames per finishing match with active spectators; matches with zero spectators still exit after the 5 s player linger.
- **Future work:** Public live-match directory spectating can reuse the exact same `Role::Spectator` tickets and server session path without changing the match protocol.
