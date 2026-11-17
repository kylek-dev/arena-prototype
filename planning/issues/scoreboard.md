Show a scoreboard when a match ends: every player's damage done, healing done, crowd control and other per-player statistics, with **Leave** and **Queue again** buttons (user request, 2026-11-17).

Today a finished match shows a short "Victory · the match closes shortly" line, and the server closes it 5 s later with `MatchOver`, which returns the player to practice.

Acceptance criteria:
- **Stats from the server.** The match server counts each seat's statistics as the rules resolve, so every player sees the same numbers:
  - damage done and taken, healing done and received, and killing blows;
  - crowd control applied and time spent controlled, once crowd control exists (see `server-casting-resources`).
- **Sending them.** The match's final `Snapshot` (or a new end-of-match message) carries the totals. This bumps `PROTOCOL_VERSION`. Server tests check that the totals match the events.
- **The scoreboard.** In the browser, it shows the result (victory, defeat or draw), one row per player grouped by team with the local player highlighted, and sortable columns.
  - It replaces the "closes shortly" line and stays up after the server closes the match.
  - **Queue again** puts the player, and their party if they lead it, back in the same queue. **Leave** returns to the home screen.
- **Timing.** Decide whether the server should keep a finished match open longer than 5 s while players read the board. The board itself does not need the connection.
- **Review.** Human browser review of the scoreboard.
