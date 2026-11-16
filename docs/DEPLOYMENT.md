# Deploying a playtest server

How to run the match server, the front door and the browser client on one
machine for remote playtests ([ADR 0045](decisions/0045-front-door-and-playtest-deployment.md)).
This setup is **proposed**. It ran locally behind a self-signed certificate
(see [Local trial](#local-trial)) but has not yet served a public playtest.

## What runs

| Part | Role | Reached at |
| --- | --- | --- |
| Caddy | TLS, static browser client (`dist/`), reverse proxy | `https://HOST/` |
| `arena-door` | Creates matches and join links; lobby page | `/lobby/`, `/api/matches` |
| `arena-server` | Hosts matches over WebSockets | `wss://HOST/play` |

The match server and door share `ARENA_TICKET_KEY`. Only Caddy is published.
The match server's `/metrics`, `/readyz` and `/healthz` stay on the private
network.

## Requirements

- A Linux machine with Docker and Docker Compose, with ports 80 and 443 open.
- A DNS name pointing at it. Caddy obtains a certificate for it
  automatically.
- A browser client build: `./scripts/build-web.sh` produces `dist/`. Check
  that the character assets in it may be redistributed before you serve it
  publicly; see [asset provenance](../assets/README.md).

## Steps

```sh
cp deploy/.env.example deploy/.env
# Edit deploy/.env: ARENA_HOST, and ARENA_TICKET_KEY from `openssl rand -hex 32`.
./scripts/build-web.sh
docker compose -f deploy/compose.yaml --env-file deploy/.env up -d --build
```

Open `https://HOST/lobby/`, choose a size and create a match. Open your own
link, then send each other link privately to one player. Anyone who has a
link can take that seat. The match starts when every seat has joined, and
the links expire after two hours.

Keep `deploy/.env` out of Git (it is ignored) and out of shared screenshots.
Changing the key invalidates every link already handed out.

## Updating and stopping

- **SIGTERM drains.** `docker compose stop server` (or a redeploy) sends
  SIGTERM. The server then stops starting matches and lets running ones
  finish for up to 1210 s. A round lasts at most 20 minutes, plus a 5 s
  linger. Compose waits 1215 s (`stop_grace_period`) before killing it.
- **Restarting now.** To restart sooner, accept that running matches end:
  `docker compose stop -t 5 server`. A second SIGTERM also stops at once.
- **During a restart.** Players whose match had not started get
  `ServerShutdown` and retry by themselves until the new server is up.
  Players of matches that were cut off see the match as over.
- **The door.** The door is stateless. Restart it at any time.
- **Caddy.** A configuration reload keeps match WebSockets open for up to
  21 minutes. A restart of Caddy drops them, and players reconnect
  automatically.
- **On a cluster.** Do not evict instances hosting matches. Use Agones
  allocation or `cluster-autoscaler.kubernetes.io/safe-to-evict: "false"`,
  and set `terminationGracePeriodSeconds` a few seconds above
  `--drain-seconds` ([ADR 0044](decisions/0044-authoritative-match-server.md#shutdown-and-draining-implemented-deployment-guidance-proposed)).

## Limits and settings

| Setting | Default | Where |
| --- | --- | --- |
| Open connections per client address | 12 | `arena-server --connections-per-address` |
| Matches created per client address | burst 5, then 1 per 30 s | `arena-door` (code: `DoorConfig`) |
| Allowed page origins (`/play`) | required without `--dev` | `arena-server --allow-origin` |
| Allowed page origins (`/api/matches`) | the `--page` origin | `arena-door --allow-origin` |
| Join link lifetime | 2 hours | `arena-door --ticket-minutes` |
| Trusted proxy (for client addresses) | none | `--trusted-proxy CIDR` on both |

`--trusted-proxy` must name only the proxy. Every client can write
`X-Forwarded-For`, so it is honoured only on connections from that address.
In `compose.yaml` Caddy has a fixed address (`172.30.10.2`) on a private
network.

## Watching a playtest

- `docker compose logs -f server` prints one line per match start, seat
  join, disconnect and end. It never prints tickets.
- Server counters: `docker compose exec caddy wget -qO- http://server:8080/metrics`.
- Players can add `?debug=1` before the `#` in their link to see ping,
  corrections, queue and server tick rate in the diagnostics overlay.
  Record those numbers in the playtest notes (issue `remote-playtests`).

## Local trial

Run on 2026-11-15 on the development machine (WSL2, Docker 29.1.3), with
`ARENA_HOST=localhost:8443`, which makes Caddy use its own local
certificate authority. Observed:

- **Image.** `docker build -f deploy/Dockerfile` succeeded: 125 MB, with
  both binaries.
- **Stack.** All three services started. The first attempt used
  `--flag=value` arguments, which the binaries do not parse. The compose
  file now passes values separately.
- **Static client.** The client page (`text/html`), its `.wasm`
  (`application/wasm`) and `/lobby/` were served over HTTPS. The server's
  `/metrics` was not reachable through Caddy (404).
- **Origins.** `POST /api/matches` gave 403 from a foreign origin and 200
  from `https://localhost:8443`. Its links carried
  `server=wss://localhost:8443/play`.
- **Joining.** A scripted client opened two links' seats over `wss://`
  through Caddy: upgrade 101, then `Welcome`. The server logged both seats
  joining. A foreign `Origin` on `/play` got 403.
- **Draining.** `docker compose stop` sent SIGTERM, and the server logged
  that it was draining its running matches for up to 1210 s. The container's
  stop timeout was 1215 s.

Not observed: a public host and certificate, a browser playing through the
proxy, latency over real networks, or a remote playtest.
