Let a player host a private custom match for friends: choose the size (1v1 to 3v3), invite people, arrange teams and start when ready.

Acceptance criteria:
- **Hosting and inviting:**
  - A host creates a custom lobby in the game menu and gets a short invite code or link.
  - The code only lets people into the lobby. It is not a seat credential: seat tickets are issued to each player when the host starts, as in `matchmaking-queues`.
- **Lobby controls:**
  - Players see who is in the lobby and pick or swap teams.
  - The host can move or remove players, change the size before starting, and pass the host role on.
  - Starting requires a full lobby (bots in empty seats can come later).
- **Settings:** a small, versioned set of match options the server enforces (for now the map, when there is more than one; later the round time limit from `round-clock`). Never free-form values.
- **Rematch:** after the match ends, players return to the same lobby, so they can play again without new invites.
- **Spectator slots:** room in the lobby for spectators, used once `spectating` lands.
- **Server and tests:**
  - Lobby state lives with the matchmaker, with the same per-address limits.
  - Tests for joining with codes, team moves, the host leaving, a full lobby, starting, and rematch.
  - Human browser review of the lobby UI.
