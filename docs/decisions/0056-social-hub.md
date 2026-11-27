# ADR 0056: The social hub

Date: 2026-11-26. **Status: proposed** (requested by the user 2026-11-26;
the open questions in [social-hub](../../planning/issues/social-hub.md) are
answered provisionally below, for the user to confirm). **Partly
superseded by [ADR 0057](0057-hub-on-the-world-server.md)** (2026-11-26):
the hub moves to the world server with the arena's rules and full HUD;
where it runs, movement and the movement messages below give way to it.

Builds on [ADR 0046](0046-queues-parties-and-home-screen.md) (the queue
connection, parties), [ADR 0050](0050-spectating-and-anti-ghosting-delay.md)
(watch-only state), [ADR 0055](0055-accounts-oauth-login-in-arena-door.md)
(accounts, friends, blocks) and the chat server
([chat](../../planning/issues/chat.md)).

## Context

Players enter the game from the launcher page. The user wants them to land
in an open-world hub where they see each other, move around, chat, invite
and add friends, and queue for matches from there, and to come back to it
after a match with their party intact. Today a match is a short-lived
session on the match server with fixed seats (at most six fighters), and
everything social (identity, friends, blocks, parties, presence, chat)
lives in the front door's queue task, reached over the queue WebSocket.

## Decision

### Where it runs

- **In the front door, beside the queue.** A hub is social state: who is
  there, their names, looks, friends and blocks, and chat. The door already
  holds all of it in one task and every hub player already has a queue
  connection, so the hub rides on that connection as more JSON messages.
  Putting it on the match server would need identity, friends and blocks
  there, rosters above six fighters, and matches that never end.
- **Pure rules, separate from I/O.** Movement and interest live in
  `arena_sim::hub`: integer millimetres, a fixed hub tick, no clocks or
  sockets, so the browser predicts with exactly the server's code. The
  door's `hub` module only keeps instances and routes messages. If one door
  cannot carry the hubs, the module moves into its own process later
  without changing the rules or the messages.

### Instances

- **Capacity.** Each hub instance (a "channel") holds up to 40 players
  (dozens, not hundreds: the user's open question; a constant, tested).
- **Placement.** Joining puts a player in the fullest instance that still
  has room (so instances stay lively), or a new one. A player may ask to
  join a friend's instance; that works when it has room and neither has
  blocked the other. Party members join the instance of a member already
  there when it has room.
- **Lifetime.** An empty instance closes. Instances are numbered from 1
  and the number is shown ("Hub 2").

### Movement and authority

- **Intent only.** A client sends its movement intent (a direction on the
  ground, each axis −1000..1000 as in the arena, and its facing), never a position. The door
  integrates every player at 30 hub ticks per second (the arena's), sending
  states 10 times a second, keeps them inside the plaza and outside its
  obstacles, and sends the results. Speed, teleport and wall hacks have
  nothing to send.
- **No combat.** Nothing in the hub damages, casts or collides with other
  players (bodies pass through each other, so nobody can block a doorway).
- **Walking and jumping are the arena's own code** (amended 2026-11-26
  after two playtests: a 10 Hz re-derivation made jumps coarse and the
  animation stutter). `arena_sim::motion` is the per-tick movement both
  the arena and the hub run, at the arena's 30 ticks a second; only what
  stops a body differs (pillars and rooms, or the plaza's edge and
  columns). The door owns jumps; others see each player's height and
  upward speed and draw the arc with the same gravity.
  Duels and training dummies are later work.
- **Prediction.** The client sends one numbered move per hub tick while
  walking or turning; the door applies each once, in order (a player with
  none waiting stands), and acknowledges the last one applied. The client
  predicts with `arena_sim::hub` and, on each state, starts from the
  door's place and replays its moves after the acknowledged one, so both
  agree exactly; a small correction fades out on screen. (Amended
  2026-11-26 after a playtest: the first version applied the newest intent
  every tick, and client and door drifted apart on starts, stops and
  turns, which showed as rubber-banding.)
- **Rules.** `arena_sim::hub` (implemented with this proposal): the
  arena's motion at 30 hub ticks per second (240 mm a tick), an
  80 m plaza, a fountain and four columns that bodies are pushed out of,
  one spawn place per player of a full instance south of the fountain, and
  the 40 m interest radius.

### Messages

JSON on the queue WebSocket, beside the queue's own (every field checked
at decode; anything malformed is refused like any queue message):

| Direction | Message | Meaning |
| --- | --- | --- |
| client → door | `hub_join {appearance, friend?}` | Enter a hub instance with this look (identifier-only and bounded, as `WireAppearance`), optionally beside a friend (`Name#number`). |
| client → door | `hub_move {seq, x, z, facing}` | One hub tick of walking (`x`, `z` in −1000..1000, `facing` a binary angle) and its increasing sequence number. |
| client → door | `hub_leave` | Leave the hub. |
| door → client | `hub {instance, you, x, z}` | Joined: the instance number, this player's hub id and place. |
| door → client | `hub_state {tick, ack, x, z, players, enter, exit}` | Each hub tick: the last intent applied (`ack`), this player's place, `[id, x, z, facing]` for each player in view, `enter` (players who came into view, or whose name or look changed: `{id, handle, admin, appearance}`) and `exit` (ids that left view or the hub). |
| door → client | `hub_left {reason}` | This player left the hub (`left`, or `match` when a ticket took them into one). |
| door → client | `refused {code}` | A join or move was turned down: `in_hub`, `not_in_hub`, `hub_full`, `not_friends`. |

- **One message per tick, nothing lost.** Joining a full instance would
  otherwise send 40 messages into a 32-message outbox. Each tick sends one
  `hub_state` per player, carrying the changes to its view; the door
  records a view change only when that message was accepted by the
  connection's outbox, so a dropped state is simply re-sent, changes
  included, on the next tick. Renames and new admin roles reach viewers as
  an `enter` for a player already in view.
- **Moves have their own budget.** This amends ADR 0046's single budget of
  10 messages per second per queue connection, counted before decoding: a
  `hub_move` counts against a separate budget of 20 per second, and moves
  over it are dropped rather than closing the connection. The client
  sends at most one move per hub tick (10 per second); every other
  message keeps the old budget.
- **Following is for friends.** `hub_join {friend}` only works for an
  accepted friend (or a party-mate), so nobody can follow a stranger into
  an instance; otherwise it is refused `not_friends`. Friend lookups share
  the per-account invite budget.
- **Room for what matters.** Hub states never take the last slots of a
  connection's outbox, which stay for tickets, ready checks and chat; a
  state that does not fit waits for the next tick. Moves past four times
  their budget close the connection.

### What each client receives

- **Interest.** A player receives the players within 40 m of them in
  their instance (at most 39). Players coming into view arrive once with
  their handle and look (`enter`); after that only positions travel
  (`players`, 10 per second, compact arrays), and leaving view is an
  `exit`.
- **Budget.** At 39 neighbours, a `hub_state` without changes is about
  1.2 KB, so about 12 KB/s per client, well under a 32 KB/s budget per
  client. A look is at most about 3 KB, so arriving in a full instance
  costs one larger message.
- **Blocks hide.** A player never sees, and is never seen by, a player
  either of them blocked, in the hub or its chat. A block made while both
  are present takes each out of the other's view on the next tick
  (`exit` both ways); unblocking brings them back (`enter`). Blocks are
  between accounts, so guests (who cannot be blocked) are never hidden.
- **Radius.** On the 80 m graybox plaza, 40 m filters little; it bounds
  the cost once hubs grow.

### Interacting

- **Chat.** A `hub` channel reaches the instance; whispers, parties and
  public chat work as today, with the same cleaning, budgets and mutes.
- **Player menu.** Clicking a player offers Whisper and Invite to party
  (friends who are online, the same rules as the Friends list), Add friend
  (others) and Block. Report comes later.
- **Queues and lobbies.** They work from the hub as from anywhere; the
  center panel shows invitations and match found.

### Leaving and returning

- **To a match.** A seat or spectator ticket from the queue takes the
  player out of the hub (others see them leave) and into the match.
- **Back.** After the match the client asks to rejoin. The door remembers
  each connection's last instance and prefers it (after a party-mate's),
  so a party that queued together lands together. The party survives
  because the queue connection does.

### The map

- **Graybox first.** A flat 80 × 80 m plaza with a few obstacle columns,
  defined in `arena_sim::hub` for now. The dressed hub is an art
  production track ([ADR 0052](0052-art-scope-and-quality-bar.md)).

## Consequences

- The door's queue task does more work per tick: 10 Hz integration and
  interest for up to 40 players per instance. A load test with bots fills
  an instance and measures bytes sent and tick time.
- Hub positions are not part of any match replay or hash.
- The browser gains a hub mode: the client renders the plaza, the players
  in view with their looks and nameplates, and the local player predicted.
- Open for the user: instance size, whether to allow duels, and what the
  hub should contain (vendors, dummies, a queue board).
