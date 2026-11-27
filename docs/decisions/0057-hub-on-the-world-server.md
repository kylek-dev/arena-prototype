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
