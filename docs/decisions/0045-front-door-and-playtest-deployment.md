# ADR 0045: Front door, ticket-started matches and playtest deployment

Date: 2026-11-15. **Status: mixed.** Ticket-started matches, the front door
(`arena-door`), the `Origin` and per-address checks and the container image
are **accepted and implemented**, with automated tests. The single-machine
deployment (Caddy, Compose) is **proposed**: it was started locally
against a self-signed `localhost` certificate (see Verification) but has
not run on a public host. No remote playtest has happened yet. Hosting stays
open until one has (issue `remote-playtests`).

Builds on [ADR 0044](0044-authoritative-match-server.md) (stateless front
door, stateful match servers, signed join tickets, draining).

## Context

Only the loopback-only `--dev` mode could create matches, and the server
spoke plain `ws://`. Remote players need a way to get join links over the
internet, TLS, and protection for a public endpoint: other websites should
not be able to use the server through their visitors, and one address should
not be able to hold every connection or create matches endlessly.

ADR 0044 proposed a stateless front door that issues signed tickets, with
match servers that derive everything from the ticket. The open question was
how the front door tells a match server to start a match.

## Decision

### A match starts when its first ticket arrives

Tickets now also carry the team size (`Claims.team_size`). A match server
that receives a valid ticket for a match it does not host starts that match,
sized by the claims, and attaches the player. The front door therefore only
needs the signing key. It never calls the match server, and adding match
servers needs no new protocol between them.

- **Refusals.** Claims for a size the rules cannot run, or a seat outside
  the match, are refused with `BadTicket` and start nothing.
- **Ended matches stay ended.** The server remembers each ended match until
  its tickets expire, so a rejoin gets `MatchOver` rather than a fresh
  match. This memory is per process: after a restart, a still-valid ticket
  for an ended match starts a new one. Nobody else joins it, so it ends
  after the 60 s start timeout.
- **Draining.** A draining server starts no new match. The first ticket of
  an unknown match gets `ServerShutdown`, which browsers retry with backoff,
  so on a single machine players join once the replacement process is up.
  Matches it already hosts keep running and accept rejoins.
- **Start window.** The 60 s start window now opens when the first player
  connects, not when the links are created. Links can be passed around
  without racing a timer.
- **Development.** `POST /dev/match` now only issues tickets. The bots and
  `scripts/dev-match.sh` work unchanged.

The ticket format changed (one more byte), but tickets are opaque to the
browser, which stores and sends them as bytes. The wire protocol and its
version are unchanged.

### The front door: `arena-door`

The front door is a second binary in `arena-server` (it shares the ticket and
ingress code). It is stateless:

- **`POST /api/matches`** with `{"team_size": 1..=3}` returns the match id,
  the expiry and one join link per seat. Each link is the game page with
  `server`, `ticket` and `expires` in the URL fragment, the format the
  browser already reads. Responses are `Cache-Control: no-store`. Request
  bodies are capped at 1 KiB, and a JSON content type is required.
- **`GET /lobby/`** is a small page: choose 1v1/2v2/3v3, create the match,
  and copy or open one link per seat. Seat 1 is marked "you". It is served
  with a strict content security policy (scripts, styles and requests only
  from the door, no framing) and `Referrer-Policy: no-referrer`. Its colors
  are palette variables, and the door generates the palette stylesheet
  from `arena-palette`.
- **No accounts.** Whoever creates a match hands out the links. Each link
  is a bearer credential for one seat, as before.
- **Logging.** The door logs one line per issued match (id and size, never
  tickets or client addresses) and exposes `/metrics` counters.

### Origins and per-address limits

- **`Origin` allowlist.** Browsers always report the page's origin on
  WebSocket handshakes and `POST`s. The match server's `/play` (with
  `--allow-origin`) and the door's API (by default, the game page's origin)
  refuse other origins with 403. Requests without an `Origin` are not from
  browsers and are allowed; tickets and limits still apply to them. Without
  `--dev`, the match server requires `--allow-origin` or an explicit
  `--any-origin`.
- **Client address.** Behind the proxy, every connection comes from the
  proxy. `--trusted-proxy CIDR` makes both binaries use the address the
  proxy appended last to `X-Forwarded-For`, and only when the peer is that
  proxy. Earlier entries can be forged by the client and are ignored. A
  client IPv6 address counts by its /64.
- **Connections per address.** The match server allows 12 open
  connections per address by default (`--connections-per-address`; 0 means
  unlimited, the `--dev` default for local bots). That is enough for six
  players behind one home router, each with a spare tab. Excess handshakes
  get 429.
- **Creations per address.** The door allows a burst of 5 matches per
  address, then one every 30 s. Excess requests get 429 with `Retry-After`.

Tickets stay out of URLs sent to servers (fragments only) and out of every
log: the door does not log them, Caddy does not log response bodies, and
fragments never reach it.

### Packaging and the proposed playtest deployment

- **Image.** `deploy/Dockerfile` builds one image with both binaries
  (Rust 1.99.1 on Debian trixie, non-root user). The build context is an
  allowlist (`.dockerignore`): the Rust workspace and the two JSON files the
  server compiles in. The browser client is not in the image.
- **Single machine (proposed).** `deploy/compose.yaml` runs Caddy, one
  match server and one door on a private network with fixed addresses.
  Caddy terminates TLS (it obtains the certificate itself), serves `dist/`
  and proxies `/play`, `/api/*` and `/lobby*`. The match server's
  `/metrics` and `/readyz` are not exposed. The match server runs with
  `--drain-seconds 1210` and a 1215 s stop grace period, and both servers
  trust only Caddy's address. Caddy keeps proxied WebSockets open for up to
  21 minutes across a configuration reload (`stream_close_delay`).
- **Operations.** Runbook: [DEPLOYMENT.md](../DEPLOYMENT.md).

## Alternatives considered

- **The front door calls the match server to create matches.** This needs an
  authenticated internal API and a second secret or signed requests, and
  the 60 s start window would open before any player has the link. It was
  rejected for now. It may return when the front door must place matches by
  capacity across several servers, for example with an allocator such as
  Agones.
- **Lobby inside the React client.** A better experience later, but it
  needs client builds and human UI review for what is a playtest tool. The
  standalone page is self-contained.
- **Invite codes with seats assigned on join.** This needs state in the
  front door. Per-seat links match the existing development flow and
  storage.

## Consequences

- One process can run on a single VM behind Caddy. More match servers need
  only the same key, plus a door that chooses a server per match. That
  choice is not implemented: the door has one `--match-server`.
- The door does not check whether the match server is ready. During a
  restart, players retry on `ServerShutdown` until the new process is up.
- Hosting (single VM, managed containers or Agones) is still to be decided
  from a remote playtest, as is the load baseline on the intended host.

## Verification

- **Automated tests.** Unit tests cover `Cidr` parsing and matching,
  `X-Forwarded-For` trust (forged entries, untrusted peers, garbled
  headers), origin normalization, per-address slots (released on drop,
  IPv6 /64), the creation token bucket, team size in tickets, join link
  encoding, the palette stylesheet, the lobby's palette-only colors and the
  production flag checks.
- **Server integration tests.** Refused tickets start no match. A match
  starts with its first ticket and stays over once ended. A draining
  server answers `ServerShutdown` to a new match while its own match
  continues and accepts rejoins. The `/play` origin check gives 403, and
  the per-address limit gives 429 and frees the slot when a connection
  closes.
- **Front door integration test.** Links from `POST /api/matches` start and
  join a match on a production-mode match server that shares the key. The
  test also checks 403 for a foreign origin, 400/413/415 for bad requests,
  429 with `Retry-After`, and the lobby's CSP, assets and metrics.
- **Local runs (2026-11-15).** A 20-match 3v3 bot run with rejoins every
  3 s (60 ms RTT, 10 ms jitter) had 181 of 181 rejoins succeed, 0 hash
  mismatches, 0 errors and 0 overruns. In production mode, a missing
  `--allow-origin` and a `ws://` front door URL were refused with
  explanations.
- **Container run (2026-11-15).** The image and Compose stack ran locally
  on WSL2 behind Caddy with a self-signed `localhost` certificate. The
  [deployment runbook](../DEPLOYMENT.md#local-trial) records what was
  checked. Not yet observed: a public host, a real certificate, browsers
  joining through the proxy, or a remote playtest.
