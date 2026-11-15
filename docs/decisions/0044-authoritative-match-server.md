# ADR 0044: Authoritative match server, fair tick resolution and scaling topology

Date: 2026-11-15. **Status: mixed.** The fair tick resolution and authoritative
facing in `arena-sim`, the `arena-protocol` message layer and the
`arena-server` match server, the `arena-bot` load tester and the browser's
network transport and authoritative view (Phase 5a) are **accepted and
implemented**. The Phase 5b match UI (disconnected/reconnecting display,
join and rejoin controls, network diagnostics) is **accepted as the plan**
and not yet implemented. No browser has run a network match yet. The scaling topology is **proposed**: it stays open
until load measurements from the bot exist.

Builds on [ADR 0001](0001-stack.md) (Axum/Tokio, WebSockets, one regional
match process first) and the network-duel stage in
[the roadmap](../../planning/ROADMAP.md).

## Context

The user asked to start the authoritative server: the server must decide
cooldowns, position, range, line of sight, resources and health for
thousands of concurrent users, and asked whether that means one large
vertically scaled server or horizontal scaling.

Before this change `Arena::step` resolved commands in the order the caller
listed them, so the first actor in the list won ties (for example, it could
kill an opponent before the opponent's same-tick attack was checked).
Commands also carried their own `actor` field, and facing lived only in the
browser presentation.

## Decision

### Authority is the server running the rules

The server is not a separate validation layer. It runs the shared
`arena-sim` rules at a fixed 30 Hz and is the only place outcomes are
produced. Ingress validation only checks message shape, size and rate. A
client can only submit intent: movement axes, facing, jump, attack target.
Whatever the rules check (cooldowns, range, line of sight, health, and later
resources and cast times) is therefore authoritative on the server with no
server-specific code. Resources, cast times and the GCD come from wiring the
existing, currently uncompiled `casting`, `combat_state` and `spells`
modules into the arena **after** the network duel works (user decision).

### Per-actor input slots and simultaneous resolution (implemented)

- `Arena::advance(&[ActorInput])` takes one slot per actor per tick. The
  slot index *is* the actor, so an input cannot act for another actor, and a
  slot holds at most one movement, one facing, one jump and one attack:
  the per-tick command budget. The server fills slot `i` only from the
  connection whose join ticket was issued for seat `i`.
- Ticks resolve in phases, never in caller order: airborne momentum →
  movement and facing → jumps → attacks → vertical motion. Movement and jumps
  are independent between actors. Every attack is validated against the same
  post-movement state, and all damage lands together.
- **Two lethal hits on the same tick both land** (user decision). An actor
  killed on a tick still resolves its own attack that tick. When both teams
  are eliminated together the round is a `RoundResult::Draw`; otherwise
  `Won { team }`. `Hit.damage` is each hit's own amount against the
  pre-attack health, so simultaneous hits never see each other.
- `Arena::step(&[ArenaCommand])` remains as a convenience for scripts and
  tests. It bins commands into slots; a second command of the same kind for
  one actor is rejected as `Duplicate` (first wins) and an unknown actor as
  `InvalidActor`.
- `Rejected` events now carry the actor so the server can route each
  rejection only to that player.
- `Combatant.facing` is a `u16` binary angle (65536 per turn, 0 faces −Z,
  counter-clockwise from above). It is authoritative state relayed to other
  players and is free to change while alive (also airborne, without
  steering); the dead keep their heading. Team 0 spawns at 0, team 1 at a
  half turn. No rule reads facing yet. This supersedes ADR 0002's
  "facing belongs to the presentation adapter".
- `Arena::state_hash()` is a 64-bit FNV-1a digest of the tick, result and
  every fighter, pinned by a golden test. It lets the server, replays and a
  future predicting client detect divergence. The layout is excluded: it is
  fixed per match and versioned separately.

### Protocol (implemented) and transport (planned)

Secure WebSocket binary frames, encoded with `serde` + `postcard` and a JSON
debug dump for diagnostics (user decision). The `arena-protocol` crate
mirrors rule types as wire types instead of adding `serde` to `arena-sim`,
which keeps the rules dependency-free. Exhaustive conversions fail to compile
when the rules gain a variant. Decoding rejects empty, oversized,
truncated, trailing-byte, unknown-variant and over-limit frames. New pinned
dependencies: `postcard =1.1.3` (with `cobs` and `thiserror`). `serde` and
`serde_json` were already pinned in the workspace. The browser's TypeScript runtime
owns the socket and passes frames to WASM through the existing narrow bridge
(user decision). See the draft [protocol](../PROTOCOL.md).

### Match server (implemented)

`arena-server` is an Axum/Tokio binary. Each match is one Tokio task owning
a pure, clock-free `Session` state machine (waiting, running, finished,
ended), driven by a fixed interval that delays rather than bursts when late.
Connections must send `Hello` within 5 s, verify an HMAC-SHA256 join ticket
(`ticket.rs`) and are then attached to the seat the ticket names. After that
they only forward inputs and answer pings. Every limit in the
[protocol](../PROTOCOL.md) is enforced at ingress, and `/metrics` exposes
Prometheus text counters. Development mode (`--dev`, loopback only)
generates a key and offers `POST /dev/match`. Production requires
`ARENA_TICKET_KEY`.

New pinned dependencies: `axum =0.8.9` (http1, ws, query, json),
`tokio =1.54.1`, `hmac =0.13.0`, `sha2 =0.11.1`, `getrandom =0.4.3`, and for
tests only `tokio-tungstenite =0.29.0` (the version axum uses) and
`futures-util =0.3.34`. Each release is older than two weeks. `arena-bot`
uses those two at runtime and tokio's `test-util` feature in tests; it adds
no other crates.

### Disconnects and recovery (user decision)

A disconnected player is **not** forfeited. Their fighter stands idle in
place and stays targetable, and the round continues. The server marks each
fighter `connected` in every snapshot (implemented), so other players can
see who is gone. The player can rejoin at any time while the match runs,
using the same join ticket. The seat, state and identity are kept.

Browser behavior (Phase 5a implemented unless marked 5b):

- After an unexpected close, the runtime reconnects automatically with
  capped exponential backoff (0.5 s doubling to 8 s) while the ticket is
  valid. The same ticket retakes the seat.
- The ticket, server address and expiry are stored when joining, in two
  places. Per tab (`sessionStorage`): a reload or a restored tab rejoins
  automatically. Per browser (`localStorage`): the last match is
  remembered and offered as a "Rejoin match" control (5b). This refines the
  original requirement of rejoining automatically from `localStorage`: two
  tabs would otherwise both claim one seat and keep replacing each other.
- `MatchOver`, `BadTicket`, `VersionMismatch` and `MatchFull`, or an expired
  ticket, end the link and delete both stored copies. Other errors (such as
  `ServerShutdown`) keep retrying.
- (5b) A disconnected fighter is shown greyed out, with "Disconnected" on its
  unit frame and nameplate, until `connected` returns; the local player sees
  "Reconnecting…" while the link retries.
- The ticket is a bearer credential for one seat in one match only and
  expires (two hours by default). It is never logged or sent in a query
  string. Development join links (`scripts/dev-match.sh`) carry it in the
  URL fragment, which browsers do not send to servers, and the page removes
  it from the address bar on load.

### Browser network mode (Phase 5a implemented)

The TypeScript runtime (`web/src/runtime/network.ts`) owns the WebSocket
and only moves bytes. Each frame, Rust reads `[generation, ticket,
...frames]` through `arenaShell.readNetwork`, where `generation` increases
with every newly opened socket. Rust then sends frames through
`sendNetwork` and reports status through `onNetwork`. Rust
(`arena-client/src/network.rs`) owns the protocol. It greets each new
socket with `Hello`, sends inputs and pings, and decodes welcomes,
snapshots and errors with `arena-protocol`.

- **Local numbering.** The client assumes it is actor 0, with friends on
  even and enemies on odd indices. A session renumbers the server's actors
  so this seat is local 0, friendly seats `2k` and enemy seats `2k + 1`.
  The rules, HUD, picking and nameplates work unchanged; attack targets and
  events are translated at the boundary.
- **Authoritative view.** `Arena::restore` rebuilds the rules state from
  each snapshot. The view trails the newest snapshot by 3 ticks (100 ms) on
  a clock that runs at real time and eases toward its target, snapping only
  after large gaps. Positions and facings are interpolated between
  snapshots; when snapshots stop, the view holds the newest state rather
  than extrapolating (an "underrun"). Hits, jumps and the result show when
  the view reaches their tick. The player's own rejections show at once.
  Every snapshot is checked against its state hash.
- **No prediction yet.** The player's own movement appears after a round
  trip plus the view delay, as the roadmap asks (measure first). The
  session reports smoothed ping, input-to-acknowledgement time, view delay,
  underruns and hash mismatches.
- During a match the client plays on the server's map (the colosseum),
  ignores local reset and practice-size changes and defers map-editor
  edits. A different map digest is reported. When the runtime ends the
  match, a fresh local practice round starts.

### Bots and load testing (implemented)

`arena-bot` plays real matches against a `--dev` server through ordinary
inputs only. Each direction passes through a simulated link with base RTT
plus jitter (in-process, TCP ordering kept), and the bots measure ping RTT,
input-to-acknowledgement latency, snapshot spacing, bandwidth and rejoins.
They also cross-check that every seat of a match receives the same state
hash per tick. It exits non-zero on any error, unexpected close, hash
mismatch or failed rejoin. Results are in
[the load baseline](../research/match-server-load.md). The bots found one
server bug: a connection could close without delivering `MatchOver` when
the match ended while an input was in flight. It is fixed.

### Scaling topology (proposed; first measurements)

Arena matches are small (2–6 players), self-contained and short-lived, which
makes them the easiest kind of game server to scale horizontally. Industry
practice for session-based games separates:

1. **A stateless front door** (HTTP: lobby, matchmaking, later accounts),
   horizontally scaled behind a load balancer.
2. **Stateful match servers.** A process hosts many matches, each one Tokio
   task with its own fixed tick and no shared mutable state. Capacity grows
   by adding processes and machines. Matches never migrate; a draining node
   refuses new matches and finishes current ones.
3. **Signed, short-lived join tickets** (match, player, seat, expiry) issued
   by the front door. The client connects straight to the assigned match
   server, which derives actor identity from the ticket alone.
4. **Regional deployments**, because latency, not CPU, is usually the
   binding constraint for players.

We reject one large vertically scaled server as the architecture: one
failure ends every match, distant players get poor latency, and capacity has
a hard ceiling. Vertical sizing still applies inside each node (a process
should use its cores).

First measurement ([load baseline](../research/match-server-load.md),
loopback on a shared WSL2 development machine): one process held about
2,000 concurrent 3v3 bots at 30 Hz on about 4.5 logical CPUs, with no late
ticks and no state divergence. Server latency added at most one tick to the
simulated network. CPU grows faster than linearly, and per-message I/O
rather than the rules appears to dominate. So we build shared-nothing from
the start but deploy one match process per region first, as ADR 0001
proposed. Fleet tooling (Agones, Kubernetes or a hosted allocator) waits
until measurements on the intended host and real networks justify it.

## Consequences

- A caller can no longer win a tie by listing its command first. Scripts that
  depended on in-list ordering (for example a second jump being rejected as
  `Airborne`) now see `Duplicate`.
- The local practice client builds actor 0's slot and sends its heading as
  facing every tick. Opponent presentation still uses the client's spawn
  facing toward the enemy team's centre. Networked opponents will use the
  authoritative facing instead.
- Clients must wait for the server to see their own movement until prediction
  exists. Interpolation is measured first, as the roadmap requires.

## Verification (2026-11-15)

New rules tests cover: simultaneous lethal hits and the resulting draw; that
every rotation and reversal of a full 3v3 command list produces identical
events and state; focus fire judged on shared pre-attack state; facing
authority (spawn headings, airborne turning, duplicates, frozen when
defeated); roster bounds for input slots and agreement between `step` and
`advance`; and a golden state hash that changes with every authoritative
field. A client test covers the yaw-to-binary-angle conversion and that the
practice tick sends the player's heading.

Server tests: 15 unit tests cover ticket forgery (every flipped bit),
expiry and keys, the waiting/running/finished/ended lifecycle, input
merging that matches the rules advanced directly, stale, foreign and
overflowing inputs, reconnect takeover, per-player rejections, grace-period
abandonment, shutdown, and disconnected seats reported as idle without a
forfeit and restored on rejoin. 7 WebSocket tests against a live server on
loopback cover a full two-player match where only the sender's fighter
moves and an out-of-range attack is refused to its sender only; every
hostile handshake (tampered, expired, foreign-key and wrong-seat tickets,
missing match, future protocol, text, garbage, input before `Hello`,
silence, oversized frame, second `Hello`); rate limiting; reconnect
takeover; a player whose socket closes mid-match showing as disconnected
and idle to the other player, with no forfeit, then rejoining with the same
ticket; health, readiness and metrics; the development guards; and
draining shutdown. They passed five consecutive runs locally (about 0.3 s
each). The binary was also started by hand: it served `/healthz`, created a
2v2 match and drained cleanly on SIGTERM, and it refused to start without a
key or with `--dev` on `0.0.0.0`.

Protocol tests cover round trips of every message within its frame limit,
pinned golden frames, rejection of every malformed-frame class, 20,000
deterministic random and mutated frames that never panic, reading the
version of a future `Hello`, input merging between ticks, per-recipient
event filtering, and snapshot contents matching the arena and its hash.

Observed locally on 2026-11-15: `./scripts/check.sh` passed (25 `arena-sim`
tests, 129 `arena-client` tests with 1 pre-existing ignore, 204 Vitest
tests), and WASM Clippy with warnings denied passed. No browser was
launched. The practice client's only behavioral change is phase ordering,
which a lone player cannot observe; human rendered review is not required
for this step but has not been performed.
