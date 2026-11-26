An open-world hub that players enter from the launcher page, where they see each other and interact: move around, chat, invite, add friends, and queue for matches from there (requested by the user 2026-11-26).

Acceptance criteria:
- **Design first (ADR):** a persistent shared world is a new server session type. Settle before code:
  - where it runs (the match server, alongside matches, or a separate process) and how players move between the hub and a queued match and back;
  - capacity per hub instance, sharding into several instances (channels) when full, and joining a friend's instance;
  - interest management (which players each client receives), update rate and bandwidth budget;
  - authority (server-validated movement, no combat or a safe sparring rule) and anti-abuse (speed, spam, harassment, blocks hiding players);
  - the map: a dedicated hub area (art production track) and its graybox first.
- **Seeing each other:** every player in the instance is shown with their chosen look ([networked-appearance](networked-appearance.md)) and `Name#number` nameplate; they move smoothly with interpolation.
- **Interacting:**
  - Chat: the existing channels plus a local (nearby) or hub channel; whispers as today.
  - Clicking a player offers Whisper, Invite to party, Add friend, Block, and Report later.
  - Parties, queues and custom lobbies work from the hub; the center panel shows invites and match found as today.
- **Returning:** after a match, players come back to the hub (their party intact) instead of the home screen.
- **Server and tests:** door and server tests for joining, leaving, instance limits, interest filtering, handoff to a match and back, blocked players hidden; a load test of a full instance (headless bots); human browser review.

Depends on: [launcher-page](launcher-page.md) (entry point), [social-identity](social-identity.md), [chat](chat.md), [party-queues](party-queues.md).

Open questions for the user:
- Instance size (dozens or hundreds), and whether combat is allowed in the hub (duels, sparring dummies).
- The hub's look and size, and what lives there (vendors, training dummies, a queue board).

Status (2026-11-26): [ADR 0056](../../docs/decisions/0056-social-hub.md) proposed, answering the open questions provisionally (40 players per instance, no combat, a graybox plaza). Slices: `auto/hub-rules` (ADR and the pure `arena_sim::hub` rules), then the door's hub service over the queue WebSocket with a bot load test, then the browser (entering from the launcher page, returning after matches, hub chat, the player menu) and the rendered plaza.

Progress (2026-11-26): `auto/hub-rules` (ADR, rules), `auto/hub-door` (door service, tests, load test) and `auto/hub-web` (the page: entering from the launcher page, returning after matches, the H window with the player menu, hub chat, practice switch). The rendered plaza with everyone's look and nameplate is the last slice.
