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
The match server's `/metrics`, `/readyz`, `/healthz` and `/matches/{id}`
(which the door asks, through `arena-door --match-status http://server:8080`,
whether a match has ended) stay on the private network.

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

- **Caching.** Caddy serves the client with `Cache-Control: no-cache`, so browsers revalidate each file (an unchanged one is a 304). The client's `pkg/arena_client.js` and `pkg/arena_client_bg.wasm` keep their names across builds; a browser that reused one from an older build failed to start ("function import requires a callable"), seen in a local trial on 2026-11-26.
- **SIGTERM drains.** `docker compose stop server` (or a redeploy) sends
  SIGTERM. The server then stops starting matches and lets running ones
  finish for up to 1210 s. A round lasts at most 20 minutes, plus a 5 s
  linger. Compose waits 1230 s (`stop_grace_period`) before killing it, which
  leaves time for the last profile and spans.
- **Restarting now.** To restart sooner, accept that running matches end:
  `docker compose stop -t 5 server`. A second SIGTERM also stops at once.
- **During a restart.** Players whose match had not started get
  `ServerShutdown` and retry by themselves until the new server is up.
  Players of matches that were cut off see the match as over.
- **The door.** The door keeps queues, parties and lobbies in memory,
  accounts in the `identity-db` PostgreSQL container, and admin-saved maps
  in its `door-data` volume. A restart drops searches and lobbies; players
  reconnect and queue again. Accounts and login sessions survive.
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
| How soon players leave the in-match state | about 10 s after the match ends (asked every 5 s); 21 minutes without `--match-status` | `arena-door --match-status` |
| Trusted proxy (for client addresses) | none | `--trusted-proxy CIDR` on both |

`--trusted-proxy` must name only the proxy. Every client can write
`X-Forwarded-For`, so it is honoured only on connections from that address.
In `compose.yaml` Caddy has a fixed address (`172.30.10.2`) on a private
network.

## Watching a playtest

- `docker compose logs -f server` prints one JSON line per match start, seat
  join, disconnect and end (`ARENA_LOG_FORMAT=json` in `compose.yaml`). Every
  line names the `match_id` the lobby shows, and seat lines carry `seat` and
  `conn_id`, so `docker compose logs server door | grep 0123456789abcdef`
  follows one match from the door to the server. Logs never contain tickets,
  join links or the signing key. Set `RUST_LOG=debug` in `deploy/.env` for
  more, or run the binaries without `ARENA_LOG_FORMAT` for readable lines.
- Server counters: `docker compose exec caddy wget -qO- http://server:8080/metrics`,
  and the front door's at `http://door:8090/metrics`.
- Players can add `?debug=1` before the `#` in their link to see ping,
  corrections, queue and server tick rate in the diagnostics overlay.
  Record those numbers in the playtest notes (issue `remote-playtests`).

## Accounts

Upgrading to the release that added accounts changes the ticket format.
Tickets and links issued before the upgrade stop working, as they do when
`ARENA_TICKET_KEY` changes. Upgrade between playtests.

Players log in through WorkOS AuthKit with email and password, an emailed
code, Google or GitHub
([ADR 0055](decisions/0055-accounts-oauth-login-in-arena-door.md)). Accounts
live in the `identity-db` PostgreSQL container, a database used for nothing
else. Login is off until you set it up; the game is then guest-only, as
before.

1. **Publish a privacy notice** before offering login. Say that:
   - WorkOS holds each player's email and login methods;
   - the game stores the WorkOS user id, the chosen name, friends, blocks
     and login times, and nothing else;
   - how to delete an account.

   The game ships one at `web/privacy.html` (served as `/privacy.html`
   and linked from the launcher page). Add who runs the server and how to reach
   them, and review it against your own logs and jurisdiction.
2. **Create a WorkOS application** (dashboard.workos.com; the first million
   monthly active users are free). Use the Staging environment for trials;
   production needs its own Google and GitHub OAuth apps, since WorkOS's
   default credentials work only in staging. In the application's
   **Redirects** tab, set:
   - redirect URI `https://ARENA_HOST/api/auth/callback`;
   - default sign-out URI `https://ARENA_HOST/`;
   - initiate-login URI `https://ARENA_HOST/api/auth/login`.

   Under **Authentication**, keep Email + Password, and turn on Magic Auth
   and the Google and GitHub providers you want.
3. **In `deploy/.env`**, set:
   - `ARENA_SESSION_KEY=$(openssl rand -hex 32)`, which must differ from the
     ticket key;
   - `ARENA_WORKOS_CLIENT_ID` (`client_...`) and `ARENA_WORKOS_API_KEY`
     (`sk_...`);
   - `ARENA_IDENTITY_DB_PASSWORD=$(openssl rand -hex 24)`. Compose requires
     it even when login is off;
   - optionally `ARENA_ADMIN_ACCOUNTS`, a comma-separated list of account
     ids (an id is shown as `account` at `/api/auth/me` once logged in).
     Admins get the admin tag and the admin tools (map storage for the
     editor, clearing a player's handle, chat mutes). A change applies to
     the admin tools when the door restarts, and to tags on tickets issued
     after that. Maps admins save are kept in `/var/lib/arena/maps/` on the
     `door-data` volume;
   - optionally `ARENA_BLOCKED_NAMES`, the path inside the container to a
     file of extra words handles may not contain (one per line, `#` for
     comments), added to `crates/arena-server/src/identity/blocked-names.txt`.
4. Start or restart: `docker compose up -d identity-db door`. The door waits
   for the database's health check and creates its tables on first start.

Back up the accounts with `pg_dump` while everything runs; restore into an
empty database before starting the door:

```sh
docker compose exec -T identity-db pg_dump -U arena_identity -Fc arena_identity \
  > "identity-$(date +%F).dump"
# Restore (door stopped, empty database):
docker compose exec -T identity-db pg_restore -U arena_identity -d arena_identity \
  < identity-YYYY-MM-DD.dump
```

Back up admin-saved maps by copying the `door-data` volume's `maps/`
directory.

A player deletes their account with `POST /api/auth/delete`. The door
deletes the user at WorkOS first (if WorkOS cannot be reached, nothing is
deleted and the player can retry), then the account. Their sessions end at
once, and any queue connection still open stops putting the account
into tickets. Tickets already issued stay valid until they expire (2 hours
by default), and backups keep the row until you discard them.

## Observability

Opt-in dashboards for a playtest machine ([ADR 0054](decisions/0054-observability-logs-metrics-and-local-lgtm.md)):

```sh
# In deploy/.env, to export traces as well:
#   OTEL_EXPORTER_OTLP_ENDPOINT=http://alloy:4318
#   ARENA_ALERT_WEBHOOK_URL=https://discord.com/api/webhooks/...   (optional)
docker compose -f deploy/compose.yaml --env-file deploy/.env --profile obs up -d
```

This adds two containers:

- **`lgtm`** (`grafana/otel-lgtm`): Grafana, Prometheus, Loki, Tempo and an
  OpenTelemetry collector, capped at 1 GB of memory.
- **`alloy`** (`grafana/alloy`, capped at 256 MB): ships the game host's
  telemetry to it (`deploy/observability/alloy.alloy`):
  - It scrapes `server:8080/metrics` and `door:8090/metrics` every 15 s and
    remote-writes them to Prometheus.
  - It reads the `server`, `door` and `caddy` containers' logs through the
    Docker socket and pushes them to Loki, labelled `service_name` and
    `level`.
  - It forwards OTLP traces to Tempo.

The socket is mounted read-only, but access to the Docker API is still
root-equivalent, so Alloy has no public port.

All ports of both are bound to `127.0.0.1`. Open Grafana at
`http://127.0.0.1:3000` on the machine, or from elsewhere through a tunnel
such as `ssh -L 3000:127.0.0.1:3000 HOST`. Never publish it through Caddy.

Three dashboards are provisioned in the **Arena** folder:

| Dashboard | Shows |
| --- | --- |
| Match Server Health | active matches, connections, spectators, drain state, matches created and finished, errors by code, refusals, RSS, CPU, open descriptors |
| Tick Latency & Input Jitter | tick duration p50/p95/p99 and mean from `arena_tick_duration_microseconds`, slowest tick, overruns, input arrivals per tick, starved/folded/stale/overflowed inputs, dropped frames, bytes out |
| Front Door & Matchmaking | players queued per mode, queue connections, parties, lobbies, queue wait p50/p95, ready checks and accepts/declines, matches and tickets issued, refusals, searches by party size |

Grafana in this image signs everyone in as an administrator without a
password. That is acceptable only because its port is on loopback.

**Logs.** In Grafana's Explore, use Loki:
`{service_name="server"} | json | match_id="0123456789abcdef"` follows one
match on the server, and `{service_name=~"server|door"} | json | level="WARN"`
shows the anomaly warnings. Each kind is reported at most once every 10 s per
process, with `suppressed_reports` counting what was held back:
- tick overruns and frames dropped for slow clients;
- match connections refused for bad tickets, floods or malformed frames;
- queue connections refused for floods or malformed messages.

**Traces.** Only control-plane steps are traced, never ticks or inputs. When
`OTEL_EXPORTER_OTLP_ENDPOINT` is set, the services export:

- `http.request`: one per request (except `/metrics`, `/healthz` and
  `/readyz`), with the method, the route (`/api/queue`; `unmatched` for
  unknown paths; never the query), and the status.
- `party.create` and `party.join` (never the invite code).
- `queue.search`: one per player, from entering a queue to its outcome
  (`matched` with the `match_id`, or a stop reason). Its `queue.step`
  children are `ready_check.offer`, `ready_check.accept`,
  `ready_check.decline`, `queue.requeue` and `ticket.mint`.
- `lobby.start`: a custom lobby starting its match.
- `ws.hello`: a match connection's Hello and ticket check, with the
  `match_id`, role and outcome.
- `match`: one per match, from its first connection to its end, with the
  result. Its children are `seat.join`, `seat.disconnect`, `seat.refuse`,
  `seat.replace`, `spectator.join` and `spectator.refuse`.
- `server.drain`: the drain after SIGTERM.

JSON log lines written inside one of these spans carry it under `span`, with
`trace_id` and `span_id`. Alloy stores the `trace_id` as Loki structured
metadata, which the bundled Loki datasource links to the trace in Tempo.
With the variable unset, nothing is exported and no exporter thread or
socket exists. On stop the services flush their last spans for at most
`OTEL_EXPORTER_OTLP_TIMEOUT` (3 s in `compose.yaml`).

**Alerts.** Grafana evaluates eight provisioned rules every minute
(`deploy/observability/alerts.yaml`, folder **Arena**):

| Rule | Fires when |
| --- | --- |
| Tick overruns | more than 0.1 overruns/s for 5 min |
| Bad ticket or rate-limited spike | more than 1 such refusal/s for 5 min |
| Match server not scraped | its `up` below 1, or missing, for 2 min |
| Front door not scraped | the same for the door |
| Server draining | the server reports `arena_draining` (info) |
| Drain approaching grace period | draining for 18 min of the 1230 s grace |
| High memory | resident memory above 1.5 GB for 10 min |
| High open file descriptors | more than 20 000 for 10 min |

Notifications go to the `arena-webhook` contact point at
`ARENA_ALERT_WEBHOOK_URL`, a Discord or Slack incoming webhook or any URL
that accepts Grafana's webhook JSON. Unset, they go to a discard address.

**Browser reports.** The game page posts scrubbed reports to
`/api/telemetry` on its own origin, which Caddy routes to the door. Reports
cover uncaught errors, rejections, WASM panics, WebGL context loss, startup
time, and one network and performance summary per finished match
([PROTOCOL.md](PROTOCOL.md#browser-telemetry)). The door logs each as one
line: `{service_name="door"} | json | target="arena_door::browser"`. It also
counts them in `arena_door_browser_reports_total{kind}`, shown on the Front
Door dashboard. Nothing is stored beyond the logs. Client addresses are used
for the rate limit and never logged. The lines are at `info` level: they
are claims from clients, which anyone can send within the rate limit.
Refusals are counted in `arena_door_browser_reports_refused_total{reason}`,
and malformed reports in `arena_door_browser_reports_rejected_total`. No
real browser has been seen sending one yet; the live check used hand-made
requests, and a browser check is pending.

**Profiling.** The image is built with the `profiling` feature. With
`ARENA_PYROSCOPE_URL=http://lgtm:4040` in `deploy/.env`, the match server
samples its CPU at 100 Hz (`ARENA_PYROSCOPE_HZ`) and sends flame graphs to
Pyroscope; read them under Grafana's Explore, Profiles. It is off when the
variable is empty, which is the default. On stop it waits at most 11 s for
its last upload (the agent finishes its 10 s cycle first);
`stop_grace_period` allows for it.

### Production topology

Telemetry costs nothing in licences at every stage; only where it runs
changes.

- **Early playtests ($0 extra):** the `obs` profile on the game host as
  above. Its memory limits (1 GB for `lgtm`, 256 MB for `alloy`) keep it from
  starving the match loop. Prometheus keeps 7 days
  (`PROMETHEUS_EXTRA_ARGS` in `compose.yaml`); Loki and Tempo keep the
  image's defaults, and their data can be cleared by recreating the
  `lgtm-data` volume.
- **Public or long playtests (about $5–12 a month):** run `lgtm` on its own
  small VPS, reached over a private network (WireGuard, Tailscale or the
  provider's VPC), so dashboard queries cannot compete with the 30 Hz loop
  and the history survives a game-server crash. On the game host, run only
  `alloy` and point it there:
  `ARENA_OBS_OTLP_ENDPOINT=http://OBS:4318`,
  `ARENA_OBS_REMOTE_WRITE_URL=http://OBS:9090/api/v1/write`,
  `ARENA_OBS_LOKI_URL=http://OBS:3100/loki/api/v1/push`. Publish those
  ports only on the private interface of the observability VPS.
- **Managed (Grafana Cloud, Datadog or another OTLP vendor):** set the same
  three Alloy variables to the vendor's endpoints and add their credentials
  as Alloy `basic_auth` or headers. The Rust services keep sending to
  `http://alloy:4318`, so no code changes.

### Observability trial

Run on 2026-11-25 on the development machine (WSL2, Docker 29.1.3), with
`ARENA_HOST=localhost`, the `obs` profile and the `server`, `door` and `lgtm`
services (no Caddy). For synthetic traffic a scratch override published the
door and server on loopback ports and a Node script queued six guests
(one declined ready check, two 1v1 matches found after 1.5 s and 3 s), created
one lobby match and held every seat for 70 s. Observed:

- **Logs.** Both containers wrote one JSON object per line
  (`"message":"arena-server listening on ws://0.0.0.0:8080/play"`).
- **Scraping.** `up` was 1 for `arena-server` and `arena-door`.
  `arena_tick_duration_microseconds_count` equalled `arena_ticks_total`
  (6 248). The door reported 6 tickets, 4 found waits and a queue-wait p95
  of 3.8 s. The ready-check counts included one check left over from an
  earlier aborted run of the script.
- **Dashboards.** All three were provisioned in the **Arena** folder, the
  home page opened Match Server Health, and every panel query returned data
  through Grafana's Prometheus datasource. A second run after the final
  changes (28 panels, finer tick buckets, party sizes) again returned data
  for every panel: tick p50 about 11 µs and p99 about 48 µs, and 6 solo
  searches.
- **Memory.** `lgtm` used 550 MiB of its 1 GiB limit, while the server and
  door used about 3 MiB each.

The same day, release builds were compared under the same load: 60 3v3
`arena-bot` matches of 20 s with seed 7, against `arena-server --dev`, run
alternately twice. Mean running-tick time was 8.28 and 8.80 µs before this
change and 8.43 and 8.22 µs after it, with 0 overruns in every run. That
difference is within run-to-run noise, so no regression was seen.

Not observed: a browser looking at the dashboards (they were checked through
Grafana's HTTP API), or a production host.

Phase 2 run, the same day and setup, with
`OTEL_EXPORTER_OTLP_ENDPOINT=http://alloy:4318`, the `alloy` service and a
small HTTP catcher on the host as `ARENA_ALERT_WEBHOOK_URL`. It ended with a
fresh rebuild and empty volumes. Observed through Grafana's HTTP API:

- **Metrics through Alloy.** `up` was 1 for both jobs (remote write, no
  scraping by Prometheus), and `tokio_workers` was present.
  `PROMETHEUS_EXTRA_ARGS` set retention to `1w`.
- **Logs.** Loki had `service_name` `server` and `door` with lowercase
  `level` values. Server lines inside traced spans carried `trace_id` as
  structured metadata. Fetching one of those ids from Tempo returned its
  `match` trace with two `seat.join` and two `seat.disconnect` children.
  Grafana's Loki datasource links `trace_id` to Tempo
  (`matcherType: label`); the link was not clicked in a browser.
- **Traces.** Tempo held `ws.hello`, `http.request` and `match` traces from
  `arena-server`, and `queue.search`, `http.request` and `party.create` traces
  from `arena-door`.
- **Alerts.** All eight rules loaded and evaluated healthy. With the
  previous rule set, stopping the server raised a firing critical
  "Service not scraped" alert for `job="arena-server"`. Grafana posted it to
  the webhook about 3.5 minutes after the stop. On the final rule set,
  stopping the door raised "Front door not scraped" (critical), posted
  3 min 33 s after the stop. An earlier attempt went unseen because the
  first catcher (busybox `nc`) closed connections before reading them.
- **Found and fixed.** A queue client closing its socket normally had been
  answered as `bad_request`. With the new warnings it would have logged a
  refusal on every tab close, so a Close frame now just ends the connection.

Phase 3 run, the same day, with `ARENA_PYROSCOPE_URL=http://lgtm:4040`
and the image built with `profiling`:

- **Browser reports.** Hand-made posts to `/api/telemetry` returned 204 for a
  report whose page and message carried a ticket and a URL password. They
  returned 422 for malformed JSON and 403 for a foreign `Origin`. The door's
  log line read `page: https://<redacted>@localhost/<redacted>`, and the
  ticket appeared nowhere in the door's container log or in Loki. The
  counters and the Front Door "Browser reports" panel showed the report.
  No real browser sent one; that check is pending.
- **Profiling.** Under bot traffic, Pyroscope listed `arena-server`. On
  `docker stop`, the drain waited for the three running matches, then the
  profiler stopped within its bound (5.7 s). A first attempt with a 3 s
  bound timed out, because the agent finishes its 10 s cycle first; the bound
  is now 11 s, and `stop_grace_period` is 1230 s.

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
