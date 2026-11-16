# ADR 0046: Queues, parties and the home screen

Date: 2026-11-16. **Status: accepted and implemented**, with automated tests.
Human browser review of the home screen and the Play online panel is
pending; see [UI review](../UI-REVIEW.md).

Builds on [ADR 0045](0045-front-door-and-playtest-deployment.md): the
front door signs tickets, and a match starts on its first ticket.

## Context

Remote players could only join matches through per-seat links created on
the lobby page and passed around by hand. Each link carries its ticket in
the URL fragment, which no server sees, but it is still a credential that
people copy and paste. The user asked for a queue system with 1v1, 2v2,
3v3, solo 2v2 and solo 3v3, for queueing as a team with friends, and for
setting up the character, keybinds and spells before queueing.

## Decision

### Queues and modes

- **Solo queues.** `1v1`, `solo-2v2` and `solo-3v3` take only players
  queueing on their own.
- **Team queues.** `2v2` and `3v3` take parties of up to the team size.
  They fill each team from other parties or single players in that queue
  (user decision, 2026-11-15: "parties fill up"), so two friends in 3v3
  get a third teammate. Party members always share a team.
- **Forming matches.** Matches form first come, first served (there is no
  rating yet). Parties are taken in queue order; each goes to the fuller
  team it still fits in, and the party at the front is always included. A
  party too big for the room left waits for the next match without holding
  up the parties behind it.
- **Ready check.** When a match forms, every player must confirm within
  10 s. The game confirms automatically, so this only catches tabs that
  are gone.
  - Players who did not confirm are dropped, and their parties stop
    searching.
  - Parties that lost a member also stop searching.
  - Complete parties return to the front of the queue in their order.
- **Seats.** Team sides and the seat order within each team are random.

### Parties

- **Size and invites.** A party holds up to three players. It has a
  six-character invite code (no 0/O or 1/I/L) and an invite link
  (`#party=CODE`, in the fragment). The code only lets someone into the
  party; it never grants a seat. Seat tickets go to each player directly.
- **Leading.** The leader (the first member) queues the party, and any
  member may cancel the search. When the leader leaves, the next member
  leads. A party that is searching takes no new members.
- **After a match.** The queue connection stays open through a match, so
  the party is still together when the match ends and can queue again.

### Guest identity

- **What it is.** Each browser keeps a random 128-bit guest id in
  `localStorage`. One guest has one queue connection: opening the game in
  another tab replaces the older connection (`replaced`).
- **What it is not.** The id only keeps one entry per browser. It proves
  nothing about a person: there are no accounts yet. When accounts come
  (Stage 5), the account takes over this role.

### The front door owns the matchmaker

- **Where it runs.** The queues live in `arena-door`
  (`crates/arena-server/src/matchmaker.rs`, a clock-free state machine,
  and `door/queue.rs`, its WebSocket at `/api/queue`). The front door is
  therefore no longer stateless: run one door instance. Scaling the
  matchmaker beyond one process (a shared store, or a matchmaker service
  that the doors call) is future work.
- **Restarts.** Restarting the door drops every party and search
  (`shutdown`), and players queue again.
- **Limits.** The queue connection checks the page `Origin`. It allows 12
  connections per address and 10 messages per second, and it must say
  `hello` within 5 s. Messages are parsed strictly. Refusals such as
  `not_leader`, `party_too_big`, `party_full`, `party_busy` and
  `no_such_party` keep the connection open; errors close it.
- **Protocol.** The message protocol is in [PROTOCOL.md](../PROTOCOL.md#queues-and-parties).

### Browser

- **Home screen.** The start screen becomes the home screen. From it,
  players can customize the character, open Keybinds (Settings) and
  Spells, practice, or play online. Keybinds and spells are saved in the
  browser and apply to online matches as they are. Play online is also in
  the game menu outside a match.
- **Play online panel.** Players invite friends (copy an invite link) or
  join with a code, then pick a queue. Queues a party cannot enter are
  disabled with the reason, and members see that the leader chooses. A
  banner shows the search time and players queued, with Cancel, and
  practice continues meanwhile.
- **Joining.** A found match joins through the existing match link, as a
  rejoin does, and leaves the home screen if the player is still there.
  The ticket is never shown.
- **Development.** `vite` dev and preview forward `/api` (including
  WebSockets) to a local door, so the page reaches the door on its own
  origin, as it does behind the deployment's proxy.

### The link lobby stays for now

`/lobby/` and `POST /api/matches` remain as a playtest tool until custom
matches (issue `custom-matches`) replace them.

### Character looks: next

The user decided that other players should see each player's chosen look
as part of this work. It is planned as the next step (issue
`networked-appearance`): the look is sent with the player's seat and
locked for the match. It is not implemented yet.

## Verification (2026-11-16)

- **Matchmaker unit tests.** Solo matching; parties formed by code and
  queued by their leader; refusals; parties kept together and teams filled
  with others; a party too big for the room left waiting without blocking;
  party caps and passing the lead; a member disconnecting; no-shows and
  requeueing; one connection per guest. A randomized test of 30,000
  requests checks every index after each step.
- **Door integration tests.** Over real WebSockets, against a match server
  sharing the key:
  - four solo players are matched and each joins their own seat;
  - two friends queue for 2v2 as a party and land on one team, with two
    strangers on the other;
  - no-shows are dropped and the others requeued;
  - foreign origins, bad messages, unknown codes and second tabs are
    refused;
  - stopping the door tells connected players.
- **Bots.** `arena-bot --queue MODE --door HOST:PORT --bots N` queues bots
  alongside people. A test fills a solo 2v2 through the door with four
  bots.
- **Without a browser.** A scripted client and a bot were matched for 1v1
  through `vite preview`'s `/api` proxy, and both joined the match server.
  This was run before parties were added, and the bot test covers the
  current protocol.
- **Browser tests.** 238 JavaScript tests pass, including: parsing door
  messages; the queue runtime over fake sockets (queueing, ready checks,
  parties, refusals, connection loss, invite links); and the home screen
  and game menu controls.
- **Not observed.** A browser using the home screen, parties or queues.
