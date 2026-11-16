Deploy the match server so people can play together over the internet. Today only the loopback-only `--dev` mode creates matches, and the server speaks plain `ws://`. See ADR 0044 (scaling topology; shutdown and draining).

Acceptance criteria:
- Serve the browser client and `wss://` match connections over TLS (for example a reverse proxy in front of `arena-server`).
- A minimal front door that creates matches and hands each player a join ticket signed with `ARENA_TICKET_KEY`, replacing `scripts/dev-match.sh` for remote players. No accounts yet.
- Check the WebSocket `Origin` and add per-address connection and match-creation limits; keep tickets out of URLs and logs.
- Package the server (container image) and record how to run it: `--drain-seconds 1210` with a termination grace period a few seconds longer, and no eviction of instances hosting matches (Agones allocation or `safe-to-evict: false`).
- Run a playtest between two remote players and record ping, corrections, starved/folded inputs and how it felt.
- Repeat the load baseline on the intended host (`docs/research/match-server-load.md`).

Decide hosting (single VM vs managed containers vs Agones) from that playtest, not before.
