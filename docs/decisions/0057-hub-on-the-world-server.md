# ADR 0057: The hub runs on the world server, with the arena's rules

Date: 2026-11-26. **Status: accepted** (decided by the user 2026-11-26).
Supersedes the "Where it runs", "Movement and authority" and "Messages"
parts of [ADR 0056](0056-social-hub.md); its instances, interest, blocks,
chat, player menu and return-after-a-match rules stay.

## Context

ADR 0056 put the hub in the front door, beside the queue, because it was
social state, and ruled out combat there. The door therefore has its own
movement loop, its own messages (`hub_join`, `hub_move`, `hub_state`) and
the client its own hub mode, and the game's HUD (unit frames, action bars,
nameplates, casting, targeting and focus) was hidden in the hub because no
fighter state existed to drive it.

The user wants the hub to feel exactly like the game: the same UI and the
same functionality as a match (set up bars, keybinds, targeting, focus and
arena frames there, cast spells), with enemies that are not aggressive.
They asked for the better architecture over patching the current one.

## Decision

- **One game, two rule sets.** A hub instance is a world on the match
  server (called the **world server** from now on; the crate rename is a
  separate change) running `arena_sim::Arena` with a **hub rule set**: no
  rounds and no end, players join and leave at any time, everyone a player
  is on the friendly team, and training enemies that never attack. Every
  system the arena has (movement, jumping, targeting, focus, casting,
  cooldowns, auras, combat text) works there because it is the same code.
- **Same pipeline as a match.** Hub players connect to the world server
  with a ticket (`Role::Hub`), send the same input frames and receive the
  same snapshots, so the client and the page run the same HUD in both. The
  page's hub mode, the door's hub movement and the `hub_*` movement messages
  are retired once the new hub works.
- **The door stays social.** Identity, friends, blocks, parties, presence,
  chat and queues stay in the door. The door chooses the instance (the
  placement rules of ADR 0056: fullest with room, beside a friend or party
  member) from an **instance directory** the world servers report to
  (instance, players, capacity, heartbeat), and issues the ticket. Blocks
  hide players both ways: the world server receives the block pairs of its
  players from the door and leaves each out of the other's snapshots.
- **Players are friendly to each other.** They can target each other, and
  helpful spells (for example Mend) work on them; harmful spells hit only
  training enemies. Duels come later (backlog).
- **Grow the instance in steps.** The first hub instance holds 12 players
  (the snapshot and rules grow from 6 fighters to a dynamic roster); 40, as
  ADR 0056 planned, needs interest filtering per player and delta snapshots,
  measured with the bot load test before the cap is raised.
- **State lives in the world.** Positions, health and casts stay in the
  world server's memory; a hub instance is disposable (players reconnect
  and respawn). A shared store (Valkey) for presence, parties and the
  instance directory is for running several doors, later.

## Slices

1. **Rules:** dynamic roster in `arena_sim::Arena` (join and leave by slot,
   stable fighter ids), a hub rule set (no round end, one friendly team,
   non-aggressive training enemies, friendly helpful spells), the plaza as a
   map; headless tests.
2. **Protocol:** a variable roster in snapshots (up to 12), join and leave
   in the relay of handles, looks and admin tags; codec tests and fuzzing.
3. **World server:** persistent hub worlds, `Role::Hub` tickets, the
   instance directory and heartbeats, block pairs from the door.
4. **Door:** placement through the directory and hub tickets; hub chat and
   the player menu keyed by the world's instance; leaving for a match and
   back as today.
5. **Client and page:** the hub is a network session; the full HUD shows;
   the old hub mode, messages and door movement are removed.
6. **Scale:** interest per player and delta snapshots, load test, then 40.

## Consequences

- The hub is no longer a lookalike: anything added to the game works there.
- The world server gains long-lived worlds and a dynamic roster; match
  replays and hashes keep their current form (a match's roster is fixed).
- Work done for ADR 0056 that stays: placement, interest radius, blocks
  both ways, hub chat, the player menu, the plaza, `arena_sim::motion`.

## Progress

- **Slice 1 (rules), 2026-11-27, `auto/hub-world-rules`:** `Rules::{Match,
  Hub}` on `arena_sim::Arena`. `Arena::hub()` builds a world on
  `hub::plaza()` (the plaza as an arena layout: the columns as hexagonal
  pillars inscribed in them, no rooms) with a training enemy on each of the
  four `hub::TRAINING_POSTS`. `join()` puts a player in the first vacant
  slot (or a new one) with a fresh fighter id that is never reused;
  `leave(slot)` vacates it and interrupts casts aimed at the leaver so
  nothing lands on the next occupant; the slot's lag-compensation history is
  reset on arrival and on standing up. Players are team 0 and training
  enemies team 1, so helpful spells work on players and harmful ones only on
  training enemies; inputs for training enemies and vacant slots are
  refused. A world has no round end and no clock; anyone who falls stands
  up at home with full health after `HUB_RESPAWN_TICKS` (5 s). The roster
  holds `MAX_ROSTER` = 16 fighters: `HUB_PLAYER_CAP` = 12 players and the
  four training enemies. `Arena::restore_hub` rebuilds a world from
  snapshot state with the roster checks the wire decoder will reuse. A
  match's rules, events and hashes are unchanged (a seeded 50-match 3v3
  headless run gives the same combined state hash before and after).
- **Slice 2 (protocol), 2026-11-27, `auto/hub-world-protocol`:**
  `PROTOCOL_VERSION` 12. `Welcome.hub` (the player's slot in `actor`, team
  size 0, no clock); `Snapshot.roster` (`WireRoster`: each slot's occupant,
  fighter id, home and respawn tick, and the next id), validated with the
  rules' own `valid_hub_roster` so a decoded world always restores
  (`Snapshot::hub_arena`); worlds hold 1 to 16 fighters, no result,
  scoreboard or `Finished`. Instead of the per-seat `Appearances`,
  `Handles` and `Tags`, a world relays `Member` (id, handle, tags, look) for
  everyone present on arrival and for each newcomer, and `MemberLeft` when
  someone goes, keyed by fighter id so a reused slot never shows the
  leaver. `Predictor::reconcile_hub` predicts a player's movement in a
  world. Deltas are not used for worlds yet (slice 6). `RULES_VERSION` stays
  8: match outcomes are unchanged; later changes to hub rules bump it, as
  clients predict with them. For slice 3: blocked players are shown to each
  other as vacant slots, so the world server rewrites each such view (aims
  at the hidden slot dropped, events naming it withheld, the hash
  recomputed). For slice 5: the page's outbox must never evict or merge
  `Member`/`MemberLeft` (variants 7 and 8), which are additive.
- **Slice 3 (world server), 2026-11-27, `auto/hub-world-server`:**
  `Role::Hub` tickets (`ticket::issue_hub`: instance in `match_id`, team
  size 0, the door's player id); hub worlds on the match server
  (`world::World`, pure state, and `worlds.rs`, the registry and each
  world's task), started by their first ticket and ended after a minute
  empty; a dropped connection keeps its place for 10 s and a reconnect with
  the same player id takes it back. The instance directory is an exchange
  the door starts (as it already asks `/matches/{id}`): `POST
  /worlds/report`, signed with the ticket key and timestamped against
  replays, carries the door's block pairs and answers each world's
  instance, players and capacity. Blocked players see each other's slots
  as vacant (with the hash of that view), never each other's member entry
  or events, and cannot aim at each other. Metrics: `arena_worlds_active`,
  `arena_world_players`. Amends the Decision's wording: the door asks the
  world servers (one exchange a second) rather than the world servers
  calling the door, so no world server needs the door's address and the
  door's list of servers stays its own configuration. The server keeps the
  last block list for worlds that start between reports; the door must
  report before handing a ticket to someone with a new block (slice 4).
- **Slice 4 (door), 2026-11-27, `auto/hub-world-door`:** `door/worlds.rs`
  (pure placement: ADR 0056's rules over the door's placements and the
  directory's reports, 12 a world, confirmation by the directory within
  20 s, departures dropped, block pairs per world, a ticket player id per
  queue connection); the queue socket's `world_join {friend?}` and
  `world_leave {}`, answered by `world {instance, server, ticket, expires}`,
  held until the directory exchange after the placement answers; the
  exchange runs once a second while anyone is placed, one at a time, with
  a clock that only rises; `hub_left` when the directory stops showing a
  player or a match ticket takes them; `hub` chat by world; the door's own
  hub stays until slice 5 removes it.

