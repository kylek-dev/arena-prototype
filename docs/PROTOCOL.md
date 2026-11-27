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
| `Input` | `seq: u32`, `movement: [i16; 2]`, `facing: u16`, `jump: bool`, `action: WireAction`, `view_tick: u64`, `cast_buffer: u8` | `seq` increases by 1 per message; gaps are allowed, repeats and regressions are dropped. Axes must lie in ±1000; the rules reject anything else. `action` is `None`, `Cast { spell, target }` (`Pulse`, `RiftBolt`, `Quell`, `Mend`), or `CancelCast`. `view_tick` is the monotonic server tick of remote fighters rendered on the client (`0` = unspecified / present tick only); the server never trusts coordinates and clamps `view_tick` monotonically to at most `MAX_LAG_COMP_TICKS` (6 ticks = 200 ms) behind the current tick for cast-start geometry validation ([ADR 0049](decisions/0049-bounded-targeting-lag-comp-and-adaptive-interp.md)). `cast_buffer` is the player's cast buffer in ticks for this frame's `Cast` (`0` = off); the rules cap it at 12 ticks ([ADR 0058](decisions/0058-cast-buffer.md)). |
| `Ping` | `nonce: u32` | Echoed in `Pong` for RTT measurement. |

## Server → client

| Message | Fields | Notes |
| --- | --- | --- |
| `Welcome` | `protocol`, `actor: u8`, `team_size: u8`, `tick: u64`, `tick_rate: u8`, `round_ticks: u64`, `rules: u32`, `map: u64`, `spectator: bool`, `hub: bool` | `hub` is `true` for a hub world ([ADR 0057](decisions/0057-hub-on-the-world-server.md)): `actor` is the player's slot (`< 16`), `team_size` is `0`, `round_ticks` is `MAX_ROUND_TICKS` (a world has no clock), never with `spectator`. When `spectator` is `true`, `actor` is `0`, `tick` is the latest already-eligible delayed tick (or `0`, never the live tick), and the connection is watch-only. `round_ticks` is the round's tick limit (`1..=MAX_ROUND_TICKS`, default 20 minutes), `rules` is the rules version and `map` a digest of the layout, so the remaining clock, replays and mismatches are explainable. |
| `Snapshot` | `tick: u64`, `ack: u32`, `queued: u8`, `hash: u64`, `result`, `fighters` (≤ 6 in a match, ≤ 16 in a hub world), `scoreboard` (`Option<Vec<FighterScore>>`), `roster` (`Option<WireRoster>`), `events` (≤ 256) | `roster` is `None` in a match and, in a hub world, one `WireSlot` per fighter (`occupant`: `Vacant`, `Player` or `TrainingEnemy`; `id`, the fighter id, never reused in that world, `0` when vacant; `home: [i32; 2]`, `home_facing`, `respawn_at: Option<u64>`) plus `next_id`. A world snapshot has any number of fighters `1..=16`, no `result`, no `scoreboard` and no `Finished` event, and its roster must pass `arena_sim::arena::valid_hub_roster` (the rules' own check: vacant slots exactly as the rules leave them, training enemies with no cast or held spell and standing still (no height, vertical speed or momentum), distinct ids, nothing aimed at a vacant slot); the roster is part of `hash`. Hub snapshots are never sent as deltas yet. `ack` is the last input `seq` applied, and `queued` how many of the recipient's inputs are still waiting. `hash` is `Arena::state_hash()`. `result` is `None` while running, or `Won { team }`, `Draw` (simultaneous elimination) or `TimeLimitDraw` (round clock expired with both teams standing). `fighters` carries each fighter's full rule state in actor order (position, height, vertical velocity, momentum, health, resource, `gcd_remaining`, `lockout_remaining`, per-spell `cooldowns`, `active_cast`, facing, `buffered_cast`), so a client can rebuild the arena and check `hash`, plus `connected`: whether that fighter's player is attached (not part of the rules or `hash`). `scoreboard` is `None` while the match runs (1 byte on the wire) and `Some(scores)` in server actor order on the final snapshot (`result.is_some()`), with one `FighterScore` (`damage_done`, `damage_taken`, `healing_done`, `healing_received`, `killing_blows`) per seat accumulated authoritatively by the match server (not part of `hash`). `events` are tick-tagged events since the previous snapshot: `Jumped`, `CastStarted`, `CastCompleted`, `CastInterrupted`, `Hit`, `Healed`, `Finished`, and `Rejected` only for the actor it names. `Moved` is not sent because positions show it. |
| `Pong` | `nonce: u32` | |
| `Error` | `code` | Sent before the server closes: `VersionMismatch`, `BadTicket`, `MatchFull`, `MatchOver`, `RateLimited`, `Malformed`, `HelloTimeout`, `ServerShutdown`, `Replaced` (a newer connection with the same ticket took the seat). Stays variant 3 (`[3, code]`). |
| `Appearances` | `Vec<Option<WireAppearance>>` (`2..=6`, even) | Sent on join (when any seat has a locked look) and broadcast whenever a seat locks a new look on first join, in server actor order. Never carries paths; clients validate against their local kit catalog and fall back to defaults. |
| `Handles` | `Vec<Option<Handle>>` (`2..=6`, even) | Each seat's `Name#1234` from the first ticket for that seat that names one, in server actor order; `None` for a seat not joined yet or joined by a lobby link. Guest handles are not unique: two guests may share one. Sent on join (when any seat has one) to the new connection and spectators, and broadcast when a seat's handle is first locked. A `Handle` is 16 name bytes (3–16 ASCII letters and digits starting with a letter, zero-padded) and a `u16` number `1..=9999`; anything else fails decoding. Clients never name themselves. |
| `Tags` | `Vec<Tags>` (`2..=6`, even; `Tags { admin: bool }`) | Each seat's tags from the same ticket as its handle, in server actor order. Sent right after `Handles` whenever any seat has a tag. The door sets `admin` from deploy configuration. |
| `Member` | `id: u32` (not `0`), `handle: Option<Handle>`, `tags: Tags`, `appearance: Option<WireAppearance>` | Hub worlds only, in place of `Appearances`, `Handles` and `Tags`: who holds fighter id `id`, from their ticket and `Hello`. Sent for everyone present when a connection is welcomed and to everyone for each arrival, before the snapshot that first shows them; keyed by id, so a slot reused by someone new is never shown with the leaver's name or look. Training enemies have none. |
| `MemberLeft` | `id: u32` (not `0`) | Hub worlds only: fighter id `id` left; its slot may show someone else, with a new id, from the next snapshot. |

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
| Over a field limit (ticket, `Welcome` team/seat/rate/`round_ticks`/hub fields, fighters, `scoreboard` present without `result` or length ≠ fighters, a hub `roster` the rules could not produce, a `Member` or `MemberLeft` id `0`, events, result team, or combat/cast invariants) | — | Close with `Malformed` |

Closing completes the WebSocket close handshake: after its `Error` and Close
frames the server keeps reading, and discards, whatever the client had
already sent until the client answers the Close, hangs up, or 1 s passes. The
door's `/queue` socket closes the same way. Dropping the TCP socket while a
flooding client's frames are still unread would make the kernel reset the
connection, which can discard the `Error` before the client reads it.

## Versioning

`PROTOCOL_VERSION` (currently 12: hub worlds, `Welcome.hub`, `Snapshot.roster`, `Member` and `MemberLeft`, [ADR 0057](decisions/0057-hub-on-the-world-server.md); 11: `InputFrame.cast_buffer` and `FighterState.buffered_cast`, [ADR 0058](decisions/0058-cast-buffer.md); 10 added `ServerMessage::Tags`; 9 added `ServerMessage::Handles`; 8 gave `WireAppearance` `facial_hair_color` and 24 part groups for the armour slots, [ADR 0053](decisions/0053-finished-character-kits.md)) changes with any
incompatible layout change; the golden-frame test fails until it is bumped
deliberately.
`RULES_VERSION` (currently 8: cast intents shortly before the GCD or a cast ends
are held and fired when it ends, [ADR 0058](decisions/0058-cast-buffer.md); 7: a
timed cast that does not complete refunds the GCD it started; 6 charged timed casts' resource and health costs on completion
instead of at cast start; [ADR 0048](decisions/0048-server-authoritative-casting-and-resources.md))
changes when a rules change
could alter a replayed outcome, in a match or in a hub world (clients predict with both). `Welcome.map` identifies the layout.

## Join tickets

A ticket is the postcard encoding of
`{match_id, player, role, team_size, expires, settings, account, handle, admin}`
(`account` is the logged-in player's account id, or none for guests and lobby
links; `handle` is the `Name#1234` to show: the account's own, or a
temporary `Guest#NNNN` from the queue, and none for lobby links; `admin`
marks an account holding the admin role and is never set without one;
`role` is `Player { seat }`, `Spectator` or `Hub` (a place in hub world `match_id`, with `team_size` 0 and no settings; the world server picks the slot, [ADR 0057](decisions/0057-hub-on-the-world-server.md)), so player, spectator and hub credentials are never interchangeable; `expires` in Unix
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

Accounts are optional (see [Accounts](#accounts)). The front door (`arena-door`, `POST /api/matches`
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
second, and messages are at most 4 KiB. Unknown fields or types close
the connection with `error`.

| From the browser | Meaning |
| --- | --- |
| `{"type":"hello","guest":"<32 hex>"}` | First, within 5 s. One connection per guest; a newer one replaces it. |
| `{"type":"create_party"}` | Get an invite code (leaving someone else's party for a new one). |
| `{"type":"join_party","code":"K7P2QX"}` | Join a friend's party (up to 3 players, not while it searches). |
| `{"type":"leave_party"}` | Back on one's own; the next member leads. |
| `{"type":"queue","mode":"1v1"\|"2v2"\|"3v3"}` | Leader only. 1v1 takes one player; 2v2 and 3v3 take players alone or parties up to the team size ([ADR 0047](decisions/0047-accepting-matches-and-queue-status.md)). |
| `{"type":"cancel"}` | Any member stops the party's search. |
| `{"type":"chat","channel":"public"\|"match"\|"team"\|"spectator","text"}` | Chat (see [Chat](#chat)). Refused `rate_limited`, `too_long`, `empty`, `no_such_channel`, `log_in` (guests in `public`), `muted`, `no_name`, `not_in_match`, `not_spectating` or `spectators_only`. |
| `{"type":"whisper","to":"Name#1234","text"}` | Chat to a friend. Refused like `chat`, or `not_logged_in`, `no_such_friend`, `offline`, `in_match` (they are playing). |
| `{"type":"ignore"\|"unignore","handle":"Name#1234"}` | Stop or resume hearing whoever is connected as that handle, on this connection (up to 200; refused `no_such_player`, `too_many`). The ignore follows the account or guest, not the name. |
| `{"type":"invite","handle":"Name#1234"}` | Logged-in players: invite a friend to this player's custom lobby, else party (a player on their own gets a party first). Refused `not_logged_in`, `no_such_friend` (no such handle and not a friend look alike), `offline`, `rate_limited` (5 at once, then one per 6 s per account), or as joining would be. |
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
| `party` `{code, members, leader, queue, mates}` | The party, whenever it changes (`code` is `null` on one's own). `mates` lists the other members' handles, leader first (`null` for one the door cannot name); it is sent again when a member's name changes. The page shows mates as teammates and never shows the code. |
| `queued` `{mode, waiting, waited_ms, expected_ms}` | Searching; sent on queueing and every 2 s. `waited_ms` is this search's time on the door's clock; `expected_ms` is the median of the queue's last 20 waits, or `null` with fewer than 3. |
| `ready_check` `{mode, accepted, total, you, remaining_ms}` | A match to accept within 15 s; sent again whenever someone accepts. |
| `requeued` `{mode}` | Someone else did not accept; back at the front of the queue. |
| `stopped` `{reason}` | `cancelled`, `member_left`, `declined`, `not_accepted`, `member_declined` or `member_not_accepted`. The connection stays open. |
| `lobby` `{code, mode, settings, spectator_slots, you, host, can_start, members}` | Current custom lobby roster (`members`: `{id, slot, host, you, handle}`, where `handle` is the member's `Name#1234` or temporary `Guest#NNNN`, sent again when a member's name changes; older doors omit it), settings, and readiness. |
| `lobby_left` `{reason}` | `left` or `kicked`. |
| `invite` `{from, kind, code}` | A friend invites this player: `kind` is `party` or `lobby`; accepting is `join_party` / `join_lobby` with `code`, declining is doing nothing. |
| `invite_sent` `{to}` | The invite reached `to`. |
| `chat` `{channel, from, to?, admin, text}` | A chat message. `from` and `admin` are the door's (the sender's handle and admin tag); `to` is set on the sender's own copy of a whisper. `channel` is `public`, `match`, `team`, `spectator` or `whisper`. |
| `friends_changed` `{}` | Requests or friendships changed; fetch `GET /api/friends`. |
| `refused` `{code}` | `not_leader`, `party_too_big`, `party_full`, `party_busy`, `no_such_party`, `in_lobby`, `no_such_lobby`, `lobby_full`, `not_host`, `slot_full`, `lobby_not_full`, `lobby_too_big`, `no_such_member`, `invalid_settings`, `spectators_disabled`, `spectators_occupied` or `rate_limited`; for `invite` also `not_logged_in`, `no_such_friend`, `offline` or `unavailable`; nothing changed. |
| `match` `{mode, match_id, seat, spectator?, server, ticket, expires}` | This player's seat ticket (or watch-only spectator ticket when `spectator: true`). The connection stays open. |
| `error` `{code, message}` | Then the door closes: `bad_request`, `rate_limited`, `replaced` or `shutdown`. |

## Accounts

Login is optional and handled by the front door
([ADR 0055](decisions/0055-accounts-oauth-login-in-arena-door.md),
`crates/arena-server/src/identity/`, `door/auth.rs`). It is on when the door
has `ARENA_SESSION_KEY`, and then also needs `ARENA_DATABASE_URL` and the
WorkOS application (`ARENA_WORKOS_CLIENT_ID`, `ARENA_WORKOS_API_KEY`).
Players log in on WorkOS AuthKit's hosted page with email and password, an
emailed code, Google or GitHub.

| Request | Answer |
| --- | --- |
| `GET /api/auth/me` | `{account, admin, login, handle, rename_at}`: the account id in hex or `null`, whether deploy configuration makes it an admin, and whether this door offers login. |
| `GET /api/auth/login[?sign_up]` | 303 to WorkOS AuthKit (`provider=authkit`), with state and an S256 PKCE challenge; `sign_up` opens the sign-up screen. Sets `__Host-arena_oauth` (signed state and verifier, 10 minutes). Also the dashboard's initiate-login URI. |
| `GET /api/auth/callback?code&state` | Checks the state against that cookie, trades the code (with the verifier) for the WorkOS user id, and sets `__Host-arena_session`. Always a 303 to the game page; 400 when anything is off, including WorkOS's `error`. |
| `POST /api/auth/logout` | Clears this browser's session cookie. Answers `{"logout_url"}` (WorkOS's page that ends its session there and returns to the game; the page should open it), or 204 when there is no WorkOS session to end. |
| `POST /api/auth/logout_everywhere` | Ends every session of the account; answers like `logout`. |
| `POST /api/auth/delete` | Deletes the user at WorkOS, then the account; its sessions end at once. 502 `unavailable` (nothing deleted) when WorkOS cannot be reached. |
| `POST /api/auth/handle` `{"name"}` | Picks or changes the account's `Name#1234`. Answers `{"handle"}`, or `{"error"}`: `invalid_name` or `blocked_name` (422), `name_full` (409), `too_soon` with `rename_at` (429), `not_logged_in` (401). |

How the cookies behave:
- Both are `HttpOnly; Secure; SameSite=Lax; Path=/`, signed with
  `ARENA_SESSION_KEY`, and the session lasts 30 days.
- A session counts only if its account still exists at the same generation.
  Logging out everywhere and deleting raise the generation or remove the
  row.
- A `POST`, and the queue WebSocket's handshake, use the session only with
  an allowed `Origin`, and never when the door runs with `--any-origin`.
- The queue signs the session's account and handle into the player's seat
  or spectator ticket. A player without a handle, and every guest, gets a
  temporary `Guest#NNNN` (not unique): `NNNN` is the hello's guest id
  modulo 9999, plus one, so a browser keeps its guest name across
  connections (and after an admin clears its account's name) and its
  launcher page shows it before connecting (`guestHandle` in
  `web/src/domain/matchmaking.ts`). Guest names carry no rights. Because
  the name lasts as long as the browser's guest id, others can loosely link
  one guest's sessions; it reveals about 13 of the id's 128 bits. Links from `POST /api/matches` carry
  neither. A new handle or a revoked account reaches open queue connections
  at once.
- **Handles.** The player picks the name: 3–16 ASCII letters and digits,
  starting with a letter. Names containing a reserved or blocked word (after
  undoing digit swaps such as `4dm1n`) are refused; operators extend the list
  with `ARENA_BLOCKED_NAMES`. The door picks a random free number, so
  `Name#1234` is unique, comparing names without case. The first pick is
  allowed at any time and starts a 7-day clock; a different name is allowed
  again when it runs out. Changing only the letter case keeps the number and
  the clock. Plain substring matching refuses some innocent names (for
  example `Badminton` contains `admin`); that trade-off is tested.
  `GET /api/auth/me` also returns `handle` and `rename_at`.
- Login routes are rate-limited per address. Account ids, codes and tokens
  are never logged.

## Admin tools

The admin role comes from deploy configuration (`ARENA_ADMIN_ACCOUNTS`) and
is checked by the door on every request; the page hiding a button is never
the gate (social-identity step 5, `door/admin.rs`). `GET /api/auth/me`
reports `admin`, and match tickets carry it for the tag shown beside the
handle.

| Request | Answer |
| --- | --- |
| `GET /api/admin/maps` | `{maps: [{id, name}]}`: maps saved by admins. |
| `GET /api/admin/maps/<id>.json` | The map file. |
| `PUT /api/admin/maps/<id>.json` | Validates the body with `arena_map` (as the editor and build do) and stores it: `{id, name}`, or 422 `{error: "invalid_map", problems}` / `invalid_id`. At most 256 KiB; ids are lowercase letters, digits and dashes. |
| `POST /api/admin/mute` `{"handle", "minutes"}` | Stops a logged-in player's chat for up to a week (`0` unmutes); 204, or 404 `no_such_player`. Mutes last until the door restarts. |
| `POST /api/admin/handles/clear` `{"handle"}` | Removes a player's handle (204, or 404 `no_such_player`); they pick again with no wait. |

Everyone else gets 401 `not_logged_in` or 403 `not_admin`. Writes need an
allowed `Origin`, and with `--any-origin` the tools are off (cookies never
act there). Maps live in `maps/` beside the account store, written through
a synced temporary file; matches still play the built-in colosseum. A
change to `ARENA_ADMIN_ACCOUNTS` applies to new tickets after the door
restarts; tickets already issued keep their tag until they expire (2 hours),
as a cleared handle stays shown in a match already running.

## Friends

Logged-in players with a handle can befriend each other (social-identity
step 4, `door/friends.rs`, `identity/friends.rs`). Players are addressed by
`Name#1234` only.

| Request | Answer |
| --- | --- |
| `GET /api/friends` | `{friends: [{handle, relation, presence}], blocked: [handle]}`. `relation` is `friend`, `incoming` or `outgoing`; friends also get `presence`: `offline`, `online`, `searching`, `in_lobby` or `in_match` (a match ticket in the last 21 minutes, until the match server reports the match ended; see Chat). |
| `POST /api/friends/<request\|accept\|decline\|remove\|block\|unblock>` `{"handle"}` | `request` answers `{"relation"}` (`outgoing`, or `friend` when they had asked you); the others answer 204. Errors are `{"error"}`: `no_such_player` (404; also when they blocked you), `yourself` (422), `bad_request` (422), `already_friends`, `already_asked`, `too_many` (50 waiting requests, or 200 friends on either side), `nothing_there`, `pick_a_name_first` (409), `not_logged_in` (401), `rate_limited` (429). |

Acting needs an allowed `Origin`. Rate limits are per account:
- requests: 10 at once, then one per 6 minutes;
- other actions: 30 at once, then one per 2 s;
- list reads: 10 at once, then one per 3 s.

A block is invisible to the blocked player:
- `request` to the blocker answers like an unknown handle.
- `accept`, `decline`, `remove` and `unblock` answer `nothing_there` for
  unknown handles too.
- `block` succeeds on unknown handles.
- Neither a block nor an unblock sends the other player `friends_changed`.

Every other change reaches both players' open queue connections. Deleting an
account deletes its friendships, requests and blocks. "Log out everywhere"
also stops that account's open connections from inviting or being shown as
it. A plain logout affects only the browser it came from: other open tabs
keep receiving invites until they close.

## Chat

Chat travels over the queue WebSocket, so the game page needs nothing
else (planning/issues/chat.md, `door/chat.rs`). The client sends only text
and a channel; the door names the sender (`from`, `admin`) and picks who
hears it:

| Channel | Who hears it |
| --- | --- |
| `public` | Everyone connected to the door who is not playing a live match. Logged-in players only. |
| `match` | The players of the sender's match. |
| `team` | The sender's team in that match; outside one, their custom lobby or else their party, except members now playing a match. |
| `spectator` | The spectators of the sender's match. Players never hear it, and a spectator can post (or whisper) nowhere else. |
| `whisper` | One friend, on every open tab of theirs not playing a live match (logged-in players only). |
| `hub` | Everyone in the sender's hub instance ([ADR 0056](decisions/0056-social-hub.md)). Logged-in players only. |

One rule keeps spectators from feeding players hidden information
(ghosting), even from a second tab or account: **a player in a live match
hears only `match` and `team`, and only players of that match post there.**

A player is "in a match" from the moment the door issued their ticket from
the queue or a custom lobby until the match ends, they search again, or 21
minutes pass. With `--match-status`, the door asks the match server
(`GET /matches/{id}`, 16 hexadecimal digits, on the private network) every
5 seconds about each match it seated; `{"state":"ended"}` clears that
match's players and spectators at once (`live` and an unknown match, 404,
keep them seated until the 21 minutes run out). The match ends 5 seconds
after its round result (plus the spectator delay when it has spectators).
A match closed by a server stop is never reported `ended`: it may start
again on the next process. Players who joined by a lobby link have no `match`,
`team` or `spectator` channel.

Rules:
- Text is cleaned (control characters and line separators become spaces,
  zero-width and bidi-override characters are dropped, combining marks stop
  at three per character, the ends are trimmed) and holds 1–200
  characters. Queue messages may be 4 KiB, so 200 characters fit in any
  script.
- Words containing a blocked abuse word (the `[abuse]` list in
  `identity/blocked-names.txt` and `ARENA_BLOCKED_NAMES`) are masked with
  asterisks.
- Each connection may post 5 messages at once, then one a second; `public`
  also counts per account across its tabs. Chat leaves 8 slots of each
  player's outbox free, so a busy channel never pushes out a ticket or
  ready check.
- Unknown fields close the connection, so a client cannot claim a sender,
  tag or channel such as `system`, which is reserved for the server and the
  page.
- A player stops hearing someone they ignore (per connection, by account or
  guest id) or blocked (per account). An admin can mute an account; the
  mute holds on tabs that log out.

## Hub worlds through the door

Since [ADR 0057](decisions/0057-hub-on-the-world-server.md) the hub is a
world on the world server; the door only places players (`door/worlds.rs`):

| Direction | Message | Meaning |
| --- | --- | --- |
| client → door | `world_join {friend?}` | Enter a hub world beside a friend or party-mate (`Name#number`) when given. The look goes to the world server in `Hello`. |
| client → door | `world_leave {}` | Leave the hub world (the page closes its world connection). |
| door → client | `world {instance, server, ticket, expires}` | A place in hub world `instance` (16 hex digits): connect to `server` (the world server's `/play`) and send `Hello` with `ticket` (hex, a `Role::Hub` ticket) before `expires` (Unix seconds). Sent once the directory exchange after the placement has answered, so the world server already knows the newcomer's blocks. |
| door → client | `hub_left {reason}` | Out of the hub world: `left` (asked, or the world server stopped showing them: their world connection ended, or they never connected within 20 s), or `match` when a seat or spectator ticket was issued. |

Placement follows the rules below (beside a followed friend, else a
party-mate's world, else the connection's last world, else the fullest with
room, else a new world), with 12 players a world; players the world still
holds (within their reconnect grace) count toward its room. Refusals are the
same codes, plus `hub_unavailable` while the world server is draining. The
door asks the directory once a second while anyone is placed (and at once
after a placement), sending the blocked pairs among players sharing a world;
the `hub` chat channel reaches the speaker's world (only placements the
world has confirmed listen). Tickets are released only by an answer; when
the world server does not answer, they wait and are refused
(`hub_unavailable`) after 20 s, and unconfirmed placements end then too. A
rename or logout takes the player out of their world (`hub_left
{reason: "renamed"}`) so the page asks again with the new name; one more
exchange after the last player leaves tells the world server. Each queue connection
keeps one ticket player id while it is open, so the page reconnecting to its
world takes its own place back.

## News

The launcher page's news (`door/news.rs`): plain text, never rendered as
HTML.

- `GET /api/news`: `{news: [{id, title, body, posted, author?}]}`, newest
  first, at most 50; public, `Cache-Control: no-store`.
- `POST /api/admin/news` `{title, body}`: admins only (the role is checked
  on every request, as for the other admin tools; an allowed `Origin` is
  required). Text is cleaned (control and bidi-override characters dropped,
  line breaks kept in the body, trimmed); titles hold 1–80 characters and
  bodies 1–2000. 201 with the item, 422 `{error: "invalid_news", problem}`
  (`empty_title`, `long_title`, `empty_body`, `long_body`), 400 `bad_request` for a body over
  16 KiB or not of this shape, 401/403 as for admin tools.
- `DELETE /api/admin/news/<id>` (16 hexadecimal digits): 204, or 404.
- Posts and deletes share the door's per-address limit for login routes.
- `news.json` items that fail these checks (a hand-edited file) are dropped
  when the door starts; a file that cannot be read is set aside
  (`news.json.unreadable-<time>`) and logged, never overwritten.
- Kept in `news.json` in the `--news-dir` directory, written atomically (in
  memory without one); the oldest beyond 50 go.

## Browser telemetry

The game page reports a few scrubbed facts to the front door's
`POST /api/telemetry` ([ADR 0054](decisions/0054-observability-logs-metrics-and-local-lgtm.md),
`web/src/domain/telemetry.ts`, `crates/arena-server/src/door/browser.rs`). Each
request is one JSON object tagged by `kind`, at most 2 KB, from an allowed
`Origin`, within a per-address rate limit. Unknown kinds or fields are refused
(422), and the door answers 204 with no body.

| `kind` | Fields besides `page` |
| --- | --- |
| `error`, `rejection`, `wasm_panic` | `message` |
| `webgl_context_lost` | none |
| `startup` | `first_state_ms` |
| `match_summary` | `spectator`, `seconds`, `rtt_p95_ms`, `corrections`, `reconnects`, `hash_mismatches`, `underruns`, `tick_rate`, `frame_ms` |

`page` is the origin and path only. Both the page and the door remove URL
passwords (`//user:pass@`), any query or fragment, and any hex run of 16 or
more digits from text, so join tickets cannot travel. Both scrubbers are
tested against the shared cases in `tests/fixtures/scrub-cases.json`.

A page load sends at most 20 error reports, each distinct message once. It
also sends one summary per finished match, with a separate cap of 200. In
`match_summary`, `frame_ms` is the median of the engine's most recent frames
(up to 3600) when the match ends. A rejoin after the link has ended starts a
new summary.

Nothing is sent when the browser sends Do Not Track or Global Privacy
Control. `?telemetry=off` also turns reports off, and the choice is
remembered on that device until `?telemetry=on`.

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

## Hub worlds and the instance directory

A `Role::Hub` ticket joins hub world `match_id` on the world server, which
starts it on the first such ticket and ends it after a minute with nobody in
it (a later ticket starts a fresh one). The player id in the ticket is the
front door's id for that person: a second connection with it takes the same
place back (the older one gets `Replaced`), and a player whose connection
drops keeps their place for 10 s. A full world (12 players) answers
`MatchFull`.

`POST /worlds/report` on the world server (private network only; Caddy
routes only `/play` there) is the instance directory. The door sends JSON
`{now_ms, blocks: [[player, player], ...], players?: [player, ...]}` with header `x-arena-signature`,
the hex HMAC-SHA256 under the ticket key of `"world-directory"`, a zero byte
and the body. `now_ms` must be within a minute of the server's clock and
above the last accepted request's (`401` and `409` otherwise). The answer is
`{draining, worlds: [{instance: "<16 hex>", players: [player ids], capacity: 12}]}`
(`draining`: the server is stopping, place nobody new there). The door sends
`max(now, last + 1)` so a corrected clock is never refused, and reports
before handing out a ticket whose holder has a new block. The server keeps
the last accepted lists and every new world starts with them. `players`, when
given, lists every player id the door has placed: nobody else may join a
world (`BadTicket`) or stay in one (closed with `MatchOver`), since a ticket
outlives the placement it was issued for (leaving the hub, taking a match,
a rename or logout, the door forgetting a placement).
The blocks apply at once: blocked players see each other's slots as vacant,
get `MemberLeft` for each other (and `Member` again when unblocked), never
see events about each other, and cannot aim spells at each other (a cast at
a hidden slot is refused as at an empty one, and anything already aimed
when the block arrives ends); each blocked view carries the hash of the
world as that player sees it. Presence is not fully hidden: `next_id`, the
newcomer's slot and a training enemy's health falling without a `Hit` can
hint that someone unseen is there; identity, place and actions are hidden.

Every `Welcome` starts the member list afresh: the server sends the whole
list after it, so a page drops what it knew. A `Member` or `MemberLeft` the
connection's outbox cannot take closes it (they are not repeated, unlike
snapshots), and the reconnect brings the list again. A reconnect keeps the
first connection's handle and look. In a world, `connected` only means
something for player slots; training enemies are never "disconnected".
Hub tickets expire like match tickets (two hours); the page asks the door
for a new one to reconnect after that.

