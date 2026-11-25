# Match protocol

**Status: implemented by the server; no browser client yet.** This follows
[ADR 0044](decisions/0044-authoritative-match-server.md). The
`arena-protocol` crate (`crates/arena-protocol`) is the source of truth for
message layouts, limits and decoding, and its tests pin golden frames.
`arena-server` (`crates/arena-server`) implements the transport, tickets,
limits and match lifecycle below. The browser network mode is not written
yet.

## Principles

- Clients send **intent**, never results. The server never accepts an actor
  ID, coordinates, health, damage, resource, cast progress or cooldown state from a client.
- The server derives the sender's actor (seat) from its join ticket only.
- Every message is versioned. A `protocol` mismatch is refused at `Hello`.
- Everything is bounded: frame size, messages per tick, queue depth and the
  time allowed before `Hello`.

## Transport

- Secure WebSocket (`wss://`; `ws://` on loopback in development) at
  `/play`. One connection per player per match. The server disables
  Nagle's algorithm (`TCP_NODELAY`) so small frames are not held back.
- **Binary frames only**, each one message, encoded with `postcard`
  (`serde`). Text frames close the connection.
- The browser's TypeScript runtime owns the socket. It sends the `Hello`
  and recognizes `Error` frames itself, so a background tab whose engine
  is paused keeps its seat. All other frames pass raw into WASM through
  the bridge.
- A JSON rendering of every message type exists for diagnostics (server logs
  with a debug flag, browser debug overlay). It is never accepted on the wire.

## Time

- The server ticks at 30 Hz (`TICKS_PER_SECOND`). Tick numbers are `u64` and
  start at 0 for each match.
- Each tick applies **one** queued input per seat, oldest first, so input
  `n` is exactly one tick of rules and a client can predict its own
  movement by replaying its unacknowledged inputs. A tick with nothing
  queued is idle for that seat. A clump of inputs simply drains one per
  tick. Only if more than four inputs stay waiting for a whole second is
  the excess folded into one tick with `InputFrame::merge_into` (the
  latest movement, facing and non-empty combat action win, and a jump from any of
  them is kept), so a client that sends too fast cannot build up delay.
  Each snapshot's `queued` tells the recipient how many of its inputs are
  still waiting. Clients pace their input interval to keep the queue's
  lowest point over a second at one (`prediction::Pacer`).
- Clients interpolate other fighters' snapshots behind the newest one
  received, adapting smoothly from ~42 ms (`1.25` ticks) on low-jitter links up
  to ~167 ms (`5` ticks) under jitter or stalls (`prediction::InterpDelay`,
  [ADR 0049](decisions/0049-bounded-targeting-lag-comp-and-adaptive-interp.md)).
  A client predicts its own fighter by replaying its unacknowledged inputs on
  the latest snapshot (`arena_protocol::prediction`).

## Client → server

| Message | Fields | Notes |
| --- | --- | --- |
| `Hello` | `protocol: u16`, `ticket: bytes (≤ 512)`, `appearance: Option<WireAppearance>` | Must be the first message, within 5 s of connecting. Stays variant 0 with `protocol` first in every version; `peek_hello_protocol` reads it from any version so the server can answer `VersionMismatch`. `appearance` carries bounded ASCII identifiers (`ancestry`, `body`, `skin_tone`, `hair_color`, `facial_hair_color`, `eyes`, `build: 0..=100`, `≤ 24` unique `groups`, `≤ 8` unique `variants`; every identifier `1..=32` `[A-Za-z0-9_-]`, never free-form paths) and is locked per seat on first join. |
| `Input` | `seq: u32`, `movement: [i16; 2]`, `facing: u16`, `jump: bool`, `action: WireAction`, `view_tick: u64` | `seq` increases by 1 per message; gaps are allowed, repeats and regressions are dropped. Axes must lie in ±1000; the rules reject anything else. `action` is `None`, `Cast { spell, target }` (`Pulse`, `RiftBolt`, `Quell`, `Mend`), or `CancelCast`. `view_tick` is the monotonic server tick of remote fighters rendered on the client (`0` = unspecified / present tick only); the server never trusts coordinates and clamps `view_tick` monotonically to at most `MAX_LAG_COMP_TICKS` (6 ticks = 200 ms) behind the current tick for cast-start geometry validation ([ADR 0049](decisions/0049-bounded-targeting-lag-comp-and-adaptive-interp.md)). |
| `Ping` | `nonce: u32` | Echoed in `Pong` for RTT measurement. |

## Server → client

| Message | Fields | Notes |
| --- | --- | --- |
| `Welcome` | `protocol`, `actor: u8`, `team_size: u8`, `tick: u64`, `tick_rate: u8`, `round_ticks: u64`, `rules: u32`, `map: u64`, `spectator: bool` | When `spectator` is `true`, `actor` is `0`, `tick` is the latest already-eligible delayed tick (or `0`, never the live tick), and the connection is watch-only. `round_ticks` is the round's tick limit (`1..=MAX_ROUND_TICKS`, default 20 minutes), `rules` is the rules version and `map` a digest of the layout, so the remaining clock, replays and mismatches are explainable. |
| `Snapshot` | `tick: u64`, `ack: u32`, `queued: u8`, `hash: u64`, `result`, `fighters` (≤ 6), `scoreboard` (`Option<Vec<FighterScore>>`), `events` (≤ 256) | `ack` is the last input `seq` applied, and `queued` how many of the recipient's inputs are still waiting. `hash` is `Arena::state_hash()`. `result` is `None` while running, or `Won { team }`, `Draw` (simultaneous elimination) or `TimeLimitDraw` (round clock expired with both teams standing). `fighters` carries each fighter's full rule state in actor order (position, height, vertical velocity, momentum, health, resource, `gcd_remaining`, `lockout_remaining`, per-spell `cooldowns`, `active_cast`, facing), so a client can rebuild the arena and check `hash`, plus `connected`: whether that fighter's player is attached (not part of the rules or `hash`). `scoreboard` is `None` while the match runs (1 byte on the wire) and `Some(scores)` in server actor order on the final snapshot (`result.is_some()`), with one `FighterScore` (`damage_done`, `damage_taken`, `healing_done`, `healing_received`, `killing_blows`) per seat accumulated authoritatively by the match server (not part of `hash`). `events` are tick-tagged events since the previous snapshot: `Jumped`, `CastStarted`, `CastCompleted`, `CastInterrupted`, `Hit`, `Healed`, `Finished`, and `Rejected` only for the actor it names. `Moved` is not sent because positions show it. |
| `Pong` | `nonce: u32` | |
| `Error` | `code` | Sent before the server closes: `VersionMismatch`, `BadTicket`, `MatchFull`, `MatchOver`, `RateLimited`, `Malformed`, `HelloTimeout`, `ServerShutdown`, `Replaced` (a newer connection with the same ticket took the seat). Stays variant 3 (`[3, code]`). |
| `Appearances` | `Vec<Option<WireAppearance>>` (`2..=6`, even) | Sent on join (when any seat has a locked look) and broadcast whenever a seat locks a new look on first join, in server actor order. Never carries paths; clients validate against their local kit catalog and fall back to defaults. |

Snapshots go out every tick to start with. Live `PROTOCOL_VERSION = 3` combat
measurements ([netcode-measurements-2026-11-21.md](research/netcode-measurements-2026-11-21.md))
confirm that full 30 Hz snapshots remain small enough to avoid delta-state
complexity or rate reduction (which degrades targeting fairness):

- Live 30 Hz active-combat payload sizes (2026-11-21, before WebSocket and TLS
  framing): one `Input` is 15–16 bytes in v3 (+1–3 bytes in v4 for `view_tick`,
  still ≤ 20 bytes); snapshots average
  **64.0 B** in 1v1 (1.92 KB/s), **110.5 B** in 2v2 (3.31 KB/s), and
  **156.6 B** in 3v3 (**4.70 KB/s** = 37.6 kbit/s per player, 140–190 B range).
- Offline lossless field-mask + timer-decay delta encoding
  (`arena_protocol::delta::SnapshotDelta`) cuts 3v3 payload bandwidth by 57.9%
  (to 1.98 KB/s keeping `hash`), providing a verified option if future Effects
  or larger rosters push full snapshots higher.
- A worst-case snapshot (6 fighters, 256 events, maximal integers) stays well
  under the 16 KiB server frame limit. These are message payload sizes, not
  network-layer measurements.

## Limits (initial values)

| Limit | Value | On violation |
| --- | --- | --- |
| Client frame size | 1 KiB | The WebSocket layer closes the connection without an `Error` |
| Inputs queued per seat | 32 (about a second) | Drop the oldest and count it |
| Messages (inputs, pings, and WebSocket control pings/pongs) per connection per fixed 1 s window | 2 × tick rate (60) | Close with `RateLimited` |
| Outgoing frames queued per connection | 64 | Drop the frame and count it (snapshots carry full state) |
| `Hello` deadline | 5 s | Close |
| Undecodable message (truncated, unknown variant, invalid tag, trailing bytes) | — | Close with `Malformed` |
| Over a field limit (ticket, `Welcome` team/seat/rate/`round_ticks`, fighters, `scoreboard` present without `result` or length ≠ fighters, events, result team, or combat/cast invariants) | — | Close with `Malformed` |

Closing completes the WebSocket close handshake: after its `Error` and Close
frames the server keeps reading, and discards, whatever the client had
already sent until the client answers the Close, hangs up, or 1 s passes. The
door's `/queue` socket closes the same way. Dropping the TCP socket while a
flooding client's frames are still unread would make the kernel reset the
connection, which can discard the `Error` before the client reads it.

## Versioning

`PROTOCOL_VERSION` (currently 8: `WireAppearance` gains `facial_hair_color` and allows 24 part groups for the armour slots; [ADR 0053](decisions/0053-finished-character-kits.md)) changes with any
incompatible layout change; the golden-frame test fails until it is bumped
deliberately.
`RULES_VERSION` (currently 6: timed casts charge their resource and health costs
on completion instead of at cast start; [ADR 0048](decisions/0048-server-authoritative-casting-and-resources.md))
changes when a rules change
could alter a replayed outcome. `Welcome.map` identifies the layout.

## Join tickets

A ticket is the postcard encoding of
`{match_id, player, role, team_size, expires, settings}` (`role` is `Player { seat }` or `Spectator`, so player and spectator credentials are never interchangeable; `expires` in Unix
seconds; optional versioned `MatchSettings { version: 1, map, round_seconds, allow_spectators }`
for private custom matches) followed by a 32-byte HMAC-SHA256 tag over those
bytes, under a key shared by whatever issues tickets and the match servers
(`ARENA_TICKET_KEY`). The signature is checked in constant time before the
claims are parsed. Tickets are about 58–68 bytes and valid for two hours,
which covers the match, so a client reconnects with the same ticket. Browsers
treat tickets as opaque bytes.

**A match starts with its first ticket** ([ADR 0045](decisions/0045-front-door-and-playtest-deployment.md)).
A match server that receives a valid ticket for a match it does not host
starts it, sized by `team_size` (and applying validated `settings` when
present, such as the custom round time limit and spectator opt-in), so whatever holds the key
creates matches without calling the server. Claims for a team size outside
1–3, a seat outside the match, a spectator ticket when `allow_spectators` is false, or invalid custom settings get `BadTicket` and
start nothing. A match that has ended stays ended while its tickets are valid
(`MatchOver`), as long as the process runs. A draining server answers the
first ticket of a new match with `ServerShutdown`.

There are no accounts. The front door (`arena-door`, `POST /api/matches`
and the `/lobby/` page) issues one join link per seat, with the ticket in
the URL fragment. With `--dev`, `POST /dev/match?team_size=1|2|3&spectators=0..N` on the
match server returns the match id, one hex ticket per seat, and optional watch-only `spectator_tickets`. It refuses
to listen beyond loopback.

**Before the handshake.** `/play` refuses a browser whose `Origin` is not
allowed (`--allow-origin`) with HTTP 403, and a client address holding too
many open connections (`--connections-per-address`) with 429. Both happen
before the WebSocket upgrade, so no `Error` frame is sent.

## Queues and parties

The front door's `GET /api/queue` is a WebSocket of small JSON text
messages ([ADR 0046](decisions/0046-queues-parties-and-home-screen.md)),
separate from the binary match protocol. It is checked against the page
`Origin`, limited to 12 connections per address and 10 messages per
second, and messages are at most 512 bytes. Unknown fields or types close
the connection with `error`.

| From the browser | Meaning |
| --- | --- |
| `{"type":"hello","guest":"<32 hex>"}` | First, within 5 s. One connection per guest; a newer one replaces it. |
| `{"type":"create_party"}` | Get an invite code (leaving someone else's party for a new one). |
| `{"type":"join_party","code":"K7P2QX"}` | Join a friend's party (up to 3 players, not while it searches). |
| `{"type":"leave_party"}` | Back on one's own; the next member leads. |
| `{"type":"queue","mode":"1v1"\|"2v2"\|"3v3"}` | Leader only. 1v1 takes one player; 2v2 and 3v3 take players alone or parties up to the team size ([ADR 0047](decisions/0047-accepting-matches-and-queue-status.md)). |
| `{"type":"cancel"}` | Any member stops the party's search. |
| `{"type":"accept"}` | Accepts the match on offer. |
| `{"type":"decline"}` | Declines it: the check ends for everyone, and this player's party leaves the queue. |
| `{"type":"create_lobby","mode":"1v1"\|"2v2"\|"3v3"}` | Creates a private custom lobby with a 6-char invite code (leaving any idle party). `mode` defaults to `"1v1"`. |
| `{"type":"join_lobby","code":"K7P2QX"}` | Joins a private custom lobby by invite code (never a seat credential). |
| `{"type":"leave_lobby"}` | Leaves the custom lobby; passes host to the next member or deletes it when empty. |
| `{"type":"set_lobby_slot","slot":"team_a"\|"team_b"\|"spectator"}` | Moves self to an open team or spectator slot (host may swap across full teams). |
| `{"type":"move_lobby_member","member":2,"slot":"team_a"\|"team_b"\|"spectator"}` | Host only: moves a member (or swaps across full teams). |
| `{"type":"kick_lobby_member","member":2}` | Host only: removes another member from the lobby (`lobby_left` `kicked`). |
| `{"type":"transfer_lobby_host","member":2}` | Host only: passes the host role to another member. |
| `{"type":"set_lobby_mode","mode":"1v1"\|"2v2"\|"3v3"}` | Host only: changes team size before starting (overflow moves to spectators or refuses `lobby_too_big`). |
| `{"type":"set_lobby_settings","settings":{"version":1,"map":"colosseum","round_seconds":300\|600\|900\|1200,"allow_spectators":true\|false}}` | Host only: updates versioned, allowlisted match settings enforced by the match server. |
| `{"type":"start_lobby"}` | Host only: requires both teams full; rate-limited per address via `door.creations`; issues signed seat tickets and signed watch-only spectator tickets (when allowed) while keeping the lobby open for rematches. |

| From the door | Meaning |
| --- | --- |
| `party` `{code, members, leader, queue}` | The party, whenever it changes (`code` is `null` on one's own). |
| `queued` `{mode, waiting, waited_ms, expected_ms}` | Searching; sent on queueing and every 2 s. `waited_ms` is this search's time on the door's clock; `expected_ms` is the median of the queue's last 20 waits, or `null` with fewer than 3. |
| `ready_check` `{mode, accepted, total, you, remaining_ms}` | A match to accept within 15 s; sent again whenever someone accepts. |
| `requeued` `{mode}` | Someone else did not accept; back at the front of the queue. |
| `stopped` `{reason}` | `cancelled`, `member_left`, `declined`, `not_accepted`, `member_declined` or `member_not_accepted`. The connection stays open. |
| `lobby` `{code, mode, settings, spectator_slots, you, host, can_start, members}` | Current custom lobby roster (`members`: `{id, slot, host, you}`), settings, and readiness. |
| `lobby_left` `{reason}` | `left` or `kicked`. |
| `refused` `{code}` | `not_leader`, `party_too_big`, `party_full`, `party_busy`, `no_such_party`, `in_lobby`, `no_such_lobby`, `lobby_full`, `not_host`, `slot_full`, `lobby_not_full`, `lobby_too_big`, `no_such_member`, `invalid_settings`, `spectators_disabled`, `spectators_occupied` or `rate_limited`; nothing changed. |
| `match` `{mode, match_id, seat, spectator?, server, ticket, expires}` | This player's seat ticket (or watch-only spectator ticket when `spectator: true`). The connection stays open. |
| `error` `{code, message}` | Then the door closes: `bad_request`, `rate_limited`, `replaced` or `shutdown`. |

## Match lifecycle

- **Waiting:** the rules do not advance until every seat has connected once,
  so nobody acts before the others arrive. Connected players receive
  snapshots at tick 0; their inputs are acknowledged but never act. A match
  that has not filled within 60 s ends with `MatchOver`.
- **Running:** each tick merges every seat's queued inputs, advances the
  rules once and sends each connected player a snapshot with only their own
  rejections. A reconnect with the same ticket replaces the older connection,
  which receives `Replaced` (so it does not take the seat back), and
  restarts its `seq`. A match that every
  player has left for 30 s ends.
- **Disconnects:** a disconnected player's fighter stands idle where it
  was, and every snapshot marks it `connected: false`. A disconnect is
  never a forfeit, and nothing else changes. The player can rejoin at any
  time while the match runs and the ticket is valid; the seat, health and
  position are kept, and `connected` turns back on for everyone.
- **Finished:** after the snapshot carrying the result and `scoreboard`, the
  match stays open for 5 s, then closes every seated player connection with `MatchOver`.
  The 5 s linger is intentionally kept short so server tasks and sockets free
  promptly while still giving lagging clients time to receive the final
  snapshot; the browser retains the scoreboard locally after the connection
  closes. Connecting to an ended match also yields `MatchOver`.
- **Spectating ([ADR 0050](decisions/0050-spectating-and-anti-ghosting-delay.md)):**
  a connection authenticated with `Role::Spectator` receives `Welcome { spectator: true, actor: 0, ... }`, never marks any seat joined, has all `ClientMessage::Input` frames strictly ignored, and receives zero private `ArenaEvent::Rejected` events. By default (`--spectator-delay-seconds 0`), live combat ticks deliver in real time and mid-match joins receive the live snapshot immediately; when a non-zero delay (`1..=60` s) is configured, waiting snapshots (`tick == 0`) deliver immediately while live combat ticks (`tick >= 1`) trail live play by that delay, mid-match joins never leak the live snapshot, and finished matches continue draining delayed frames (including final `result` and `scoreboard`) for up to `spectator_delay + linger` before closing spectators with `MatchOver`. Each snapshot is serialized once and fanned out zero-copy across up to `--max-spectators` (default **4**) spectators per match (excess distinct spectators get `MatchFull`; same spectator `player` id replaces the older connection with `Replaced`).
- **Overload:** a match whose tick runs late delays the next one rather than
  bursting missed ticks, so the rules never run faster than real time;
  `arena_tick_overruns_total` counts it.
- **Shutdown:** SIGTERM or Ctrl+C makes `/readyz` fail and refuses new
  matches. Running ones continue (players may rejoin) for up to
  `--drain-seconds`, then are closed with `ServerShutdown`. A second
  signal closes them at once. `--dev` defaults to no drain; without
  `--dev` the drain must be given.

Clients must show a disconnected fighter as disconnected (greyed unit
frame and nameplate labelled "Disconnected") and should rejoin by
themselves: see [client recovery](decisions/0044-authoritative-match-server.md#disconnects-and-recovery).
