Let players queue for a match from inside the game instead of passing join links around: **1v1, solo 2v2 and solo 3v3** (each player queues alone; the matchmaker builds both teams). Premade 2v2 and 3v3 follow in `party-queues`.

Today the lobby page (ADR 0045) hands out one link per seat. A link keeps its ticket in the URL fragment, which no server ever sees, but it is still a seat credential people copy and paste. With a queue, the ticket goes straight from the front door to the player's browser and is never shown.

Acceptance criteria:
- **Guest identity without accounts:** a random guest id kept by the browser, which the front door uses to keep one queue entry per player. Record it in a decision record, including how it later maps to accounts (Stage 5).
- **Matchmaker:**
  - A queue per mode in the front door. Matching is first come, first served for now (no rating yet).
  - Players stay in the queue while connected (a WebSocket or long-poll to the door), and leave it by cancelling or closing the tab.
  - When a match forms, each player receives their own ticket directly and the client joins automatically.
  - A found player who does not connect within a short window is dropped, and the others go back to the front of the queue.
  - The queue makes the door stateful: start with one matchmaker instance, and record how it scales later.
- **In-game UI:** the game menu gains "Play online": choose a mode, see the queue time and how many are queued, cancel, then the existing match banner takes over. The client stores tickets as it does today (`sessionStorage` and `localStorage`). Human browser review is required.
- **Guarding the queue:** per-address limits on queue entries, as for match creation. A player who leaves matches early may get a short queue cooldown later; record the decision whichever way it goes.
- **Tests and tools:**
  - Headless matchmaker tests: forming teams, cancelling, no-shows and requeueing, and no player in two matches.
  - An `arena-bot --queue MODE` option, so load runs and solo playtests can fill queues with bots.
- **The lobby page:** decide whether `/lobby/` and its links stay as a playtest tool or are replaced by custom matches.

**Status (2026-11-17).** Solo 2v2 and solo 3v3 were folded into 2v2 and 3v3, and found matches are now accepted or declined within 15 s, with a queue bar showing the time waited and the expected wait ([ADR 0047](../../docs/decisions/0047-accepting-matches-and-queue-status.md)). Not yet decided: a sound or system notification when a match is ready.

**Status (2026-11-16).** Implemented with team queues and parties (see `party-queues`), the home screen and queueing bots ([ADR 0046](../../docs/decisions/0046-queues-parties-and-home-screen.md)). The lobby page stays until custom matches. Still open:
- Human browser review.
- A queue cooldown for players who leave matches early (not decided).
- Scaling the matchmaker beyond one door.
