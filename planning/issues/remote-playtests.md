Deploy the match server so people can play together over the internet. Today only the loopback-only `--dev` mode creates matches, and the server speaks plain `ws://`. See ADR 0044 (scaling topology; shutdown and draining).

Acceptance criteria:
- Serve the browser client and `wss://` match connections over TLS (for example a reverse proxy in front of `arena-server`).
- A minimal front door that creates matches and hands each player a join ticket signed with `ARENA_TICKET_KEY`, replacing `scripts/dev-match.sh` for remote players. No accounts yet.
- Check the WebSocket `Origin` and add per-address connection and match-creation limits; keep tickets out of URLs and logs.
- Package the server (container image) and record how to run it: `--drain-seconds 1210` with a termination grace period a few seconds longer, and no eviction of instances hosting matches (Agones allocation or `safe-to-evict: false`).
- Run a playtest between two remote players and record ping, corrections, starved/folded inputs and how it felt.
- Repeat the load baseline on the intended host (`docs/research/match-server-load.md`).

Decide hosting (single VM vs managed containers vs Agones) from that playtest, not before.

**Status (2026-11-15).** Implemented and tested locally ([ADR 0045](../../docs/decisions/0045-front-door-and-playtest-deployment.md), [DEPLOYMENT.md](../../docs/DEPLOYMENT.md)):
- Ticket-started matches and the `arena-door` front door (lobby page, `POST /api/matches`).
- `Origin` checks and per-address connection and match-creation limits, honouring `X-Forwarded-For` only from a trusted proxy.
- A container image, and a Caddy + Compose single-machine setup with drain 1210 s and a 1215 s stop grace period.
- The stack ran locally over TLS with a self-signed `localhost` certificate: scripted `wss://` joins through Caddy, origin refusals, and a SIGTERM drain.

Still open: a public host and certificate, a browser playtest through the proxy, a remote playtest between two players, and the load baseline on that host.
