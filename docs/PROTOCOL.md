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
  ID, coordinates, health, damage or cooldown state from a client.
- The server derives the sender's actor (seat) from its join ticket only.
- Every message is versioned. A `protocol` mismatch is refused at `Hello`.
- Everything is bounded: frame size, messages per tick, queue depth and the
  time allowed before `Hello`.

## Transport

- Secure WebSocket (`wss://`; `ws://` on loopback in development) at
  `/play`. One connection per player per match.
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
- Inputs that arrive between ticks are merged into the actor's slot for the
  next tick: the latest movement and facing win, `jump` is set if any input
  set it, and the latest `attack` target wins. This keeps the rules' one
  command of each kind per tick.
- Clients interpolate snapshots behind the newest one received (about
  100 ms to start). There is no client prediction in the first milestone.

## Client → server

| Message | Fields | Notes |
| --- | --- | --- |
| `Hello` | `protocol: u16`, `ticket: bytes (≤ 512)` | Must be the first message, within 5 s of connecting. Stays variant 0 with `protocol` first in every version; `peek_hello_protocol` reads it from any version so the server can answer `VersionMismatch`. |
| `Input` | `seq: u32`, `movement: [i16; 2]`, `facing: u16`, `jump: bool`, `attack: Option<u8>` | `seq` increases by 1 per message; gaps are allowed, repeats and regressions are dropped. Axes must lie in ±1000; the rules reject anything else. |
| `Ping` | `nonce: u32` | Echoed in `Pong` for RTT measurement. |

## Server → client

| Message | Fields | Notes |
| --- | --- | --- |
| `Welcome` | `protocol`, `actor: u8`, `team_size: u8`, `tick: u64`, `tick_rate: u8`, `rules: u32`, `map: u64` | `rules` is the rules version and `map` a digest of the layout, so replays and mismatches are explainable. |
| `Snapshot` | `tick: u64`, `ack: u32`, `hash: u64`, `result`, `fighters` (≤ 6), `events` (≤ 256) | `ack` is the last input `seq` applied. `hash` is `Arena::state_hash()`. `fighters` carries each fighter's full rule state in actor order (position, height, vertical velocity, momentum, health, `ready_at`, facing), so a client can rebuild the arena and check `hash`, plus `connected`: whether that fighter's player is attached (not part of the rules or `hash`). `events` are tick-tagged events since the previous snapshot: `Jumped`, `Hit`, `Finished`, and `Rejected` only for the actor it names. `Moved` is not sent because positions show it. |
| `Pong` | `nonce: u32` | |
| `Error` | `code` | Sent before the server closes: `VersionMismatch`, `BadTicket`, `MatchFull`, `MatchOver`, `RateLimited`, `Malformed`, `HelloTimeout`, `ServerShutdown`, `Replaced` (a newer connection with the same ticket took the seat). |

Snapshots go out every tick to start with. Bandwidth will be measured before
switching to deltas or a lower rate.

Encoded sizes from the crate's tests on 2026-11-15, before WebSocket and TLS
framing: one `Input` is 14 bytes. An event-free snapshot at the spawn
positions is 43 bytes for 1v1 and 99 bytes for 3v3, about 3 KB/s per
player at 30 Hz for 3v3. A worst-case snapshot (6 fighters, 256 events,
maximal integers) is 3,582 bytes, well under the 16 KiB server frame limit.
These are message sizes, not network measurements.

## Limits (initial values)

| Limit | Value | On violation |
| --- | --- | --- |
| Client frame size | 1 KiB | The WebSocket layer closes the connection without an `Error` |
| Inputs queued per seat between ticks | 8 | Drop the oldest and count it |
| Messages (inputs and pings) per connection per fixed 1 s window | 2 × tick rate (60) | Close with `RateLimited` |
| Outgoing frames queued per connection | 64 | Drop the frame and count it (snapshots carry full state) |
| `Hello` deadline | 5 s | Close |
| Undecodable message (truncated, unknown variant, invalid tag, trailing bytes) | — | Close with `Malformed` |
| Over a field limit (ticket, fighters, events) | — | Close with `Malformed` |

## Versioning

`PROTOCOL_VERSION` (currently 1) changes with any incompatible layout change;
the golden-frame test fails until it is bumped deliberately. Until the first
browser client ships, version 1 is a draft and may still change without a
bump (the per-fighter `connected` flag was added this way).
`RULES_VERSION` (currently 1) changes when a rules change could alter a
replayed outcome. `Welcome.map` identifies the layout.

## Join tickets

A ticket is the postcard encoding of `{match_id, player, seat, expires}`
(`expires` in Unix seconds) followed by a 32-byte HMAC-SHA256 tag over those
bytes, under a key shared by whatever issues tickets and the match servers
(`ARENA_TICKET_KEY`). The signature is checked in constant time before the
claims are parsed. Development tickets are about 57 bytes and valid for two
hours, which covers the match, so a client reconnects with the same ticket.

The first milestone has no accounts: `POST /dev/match?team_size=1|2|3`
creates a match and returns its id and one hex ticket per seat. It exists
only with `--dev`, which also refuses to listen beyond loopback.

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
- **Finished:** after the snapshot carrying the result, the match stays open
  for 5 s, then closes every connection with `MatchOver`. Connecting to an
  ended match also yields `MatchOver`.
- **Overload:** a match whose tick runs late delays the next one rather than
  bursting missed ticks, so the rules never run faster than real time;
  `arena_tick_overruns_total` counts it.
- **Shutdown:** SIGTERM or Ctrl+C makes `/readyz` fail and refuses new
  matches, lets running ones finish for up to `--drain-seconds`, then closes
  them with `ServerShutdown`.

Clients must show a disconnected fighter as disconnected (greyed unit
frame and nameplate labelled "Disconnected") and should rejoin by
themselves: see [client recovery](decisions/0044-authoritative-match-server.md#disconnects-and-recovery).
