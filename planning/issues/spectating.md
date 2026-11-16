Let people watch matches without playing.

Acceptance criteria:
- **Spectator tickets and data:**
  - A spectator ticket is a role in the ticket claims, with no seat, and the match server accepts it as a watch-only connection.
  - Spectators receive snapshots of every fighter, but no other player's private information.
  - The match server ignores any input from a spectator connection.
- **Delay and limits:**
  - Spectator views run a few seconds behind live play, so a spectator cannot feed information to a player (ghosting). Choose and record the delay.
  - Each match has a spectator cap, and the per-address limits apply.
- **Who may watch:** first custom matches whose host allows spectators (`custom-matches`). Later, possibly a list of live public matches.
- **Spectator client:**
  - Free camera and follow-a-player views, unit frames for both teams, the round clock and the result.
  - No action bars or targeting.
  - Human browser review.
- **Protocol and tests:**
  - Protocol changes bump `PROTOCOL_VERSION`.
  - Tests: spectators cannot act, seat tickets and spectator tickets are not interchangeable, the delay holds, and the cap is enforced.
  - The match server's CPU and bandwidth cost per spectator is measured with the bots.
- **Ordering:** build this after `server-casting-resources`, so spectators also see cast bars and resources rather than adding them twice.
