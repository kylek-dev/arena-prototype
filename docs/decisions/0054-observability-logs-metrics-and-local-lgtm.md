# ADR 0054: Observability: structured logs, histograms and a local LGTM stack

Date: 2026-11-25. **Status: proposed (Phases 1, 2 and 3 implemented).**

## Context

The match server and the front door wrote free-text `eprintln!` lines and
served Prometheus counters with no latency distribution: `/metrics` had the
slowest tick and a cumulative total, but nothing that gives p95 or p99 tick
time, and the front door counted matches but not queue waits or ready checks.
Playtests on one machine (ADR 0045) need operators to filter a match's story
end to end and to see tick latency without an external service.
[`planning/issues/observability-telemetry.md`](../../planning/issues/observability-telemetry.md)
sets the boundaries: `arena-sim` stays free of telemetry, nothing traces the
30 Hz loop or individual inputs, tickets and keys never reach telemetry,
observability stays off the public internet, and it costs nothing when off.

## Decision

Phase 1 of the issue, in `arena-server` (both binaries):

- **Logs are `tracing` events on stderr.** `ARENA_LOG_FORMAT=pretty` (default)
  or `json` (set by `deploy/compose.yaml`, one object per line like Caddy's
  log); `RUST_LOG` filters and defaults to `info`. An unknown format falls back
  to pretty with a warning rather than silencing the process
  (`crates/arena-server/src/logging.rs`).
- **Shared field names.** Lifecycle events carry `match_id` (16 hex digits, as
  the lobby shows it), `seat`, `conn_id`, `team_size`, `mode`, `error_code` and
  `reason`. Events are written with explicit fields; no span wraps the match
  task, because entering a span on every poll of the 30 Hz loop is the hot-path
  cost the issue forbids.
- **No credentials in logs.** Nothing logs a ticket, a join link or the key.
  `--debug-json` wire logging puts each message under `payload` and redacts
  the ticket of a repeated `Hello`. An
  end-to-end test (`crates/arena-server/tests/logging.rs`) captures every JSON
  line of a door-issued match with wire logging on and asserts that no ticket
  (as hex or as serialized bytes), key or link fragment appears.
- **Lock-free fixed-bucket histograms** (`metrics::Histogram<N>`): one relaxed
  `fetch_add` on the bucket and one on the sum, no locks or allocation,
  aligned to its own cache lines. `arena_tick_duration_microseconds` records
  running rule ticks, so its `_count` and `_sum` equal `arena_ticks_total` and
  `arena_tick_microseconds_total`. Buckets: 1, 2, 5, 10, 20, 50, 100, 250,
  500, 1 000, 2 500, 5 000, 10 000, 16 667, 33 333, 66 667 and 100 000 µs. A
  3v3 bot load measured a mean tick of about 8 µs, so the low buckets are
  where real p50/p95 values fall.
- **Front door `/metrics`** adds players queued per mode, open queue
  connections, parties with invite codes, custom lobbies, searches by party
  size, tickets signed, ready checks started/completed/failed, accepts,
  declines and `arena_door_queue_wait_milliseconds` (1 s to 10 min). The matchmaker keeps
  plain counters and a bounded buffer of found waits; the queue task copies
  them out after each step.
- **Process metrics** on both services: `process_resident_memory_bytes`,
  `process_cpu_seconds_total` and `process_open_fds`, read from `/proc/self` at
  scrape time only. The match server also reports `arena_draining`.
- **Local LGTM.** An opt-in `obs` Compose profile runs `grafana/otel-lgtm`
  (pinned) with every port on `127.0.0.1`, a Prometheus config that scrapes
  `server:8080` and `door:8090` on the private network, and three provisioned
  dashboards under `deploy/observability/dashboards/`. Caddy still never
  routes to `/metrics`.

### Phase 2: traces, shipping and alerts

- **Opt-in OTLP traces.** `logging::init` builds an OTLP/HTTP span exporter
  (`opentelemetry-otlp`, blocking HTTP client, batch processor) only when
  `OTEL_EXPORTER_OTLP_ENDPOINT` is non-blank; otherwise there is no exporter,
  thread or socket. The binaries start telemetry before the Tokio runtime and
  flush it after, because the blocking client must not run in an async
  context. Exported spans are filtered to the services' own targets at
  `info`, independent of `RUST_LOG`.
- **Control-plane spans only**, created with `control_span!`:
  - `http.request` from `tower-http`. It records the matched route (or
    `unmatched`) but never the query, and skips `/metrics`, `/healthz` and
    `/readyz`. Its failure events are at debug level, so the 503s from
    `/readyz` while draining add no error lines.
  - `queue.search` per player, with `queue.step` children from queue entry to
    `ticket.mint`. A search ends when the player is matched, stops, leaves or
    is replaced by a new connection. The queue task asserts in debug builds
    that no search outlives its connection.
  - `party.create` and `party.join`. Invite codes are never recorded.
  - `lobby.start`.
  - `ws.hello` for the Hello and ticket check.
  - One `match` span per match, never entered while ticking. Seat and
    spectator steps are its children.
  - `server.drain`.

  Nothing is traced per tick or per input.
- **Log ↔ trace.** `control_span!` records the OpenTelemetry `trace_id` and
  `span_id` into the span's fields, and JSON lines include the current span,
  so a log line names its trace. Alloy lifts `trace_id` into Loki structured
  metadata for Grafana's link to Tempo. The ids are also exported as span
  attributes, which duplicates the native ids; that is accepted for
  simplicity.
- **Throttled anomaly warnings** (`anomaly.rs`). Each kind is reported at most
  once every 10 s per process, with `suppressed_reports` counting the ones
  held back:
  - tick overruns;
  - frames dropped for slow clients;
  - `BadTicket`, `RateLimited` and `Malformed` refusals, each throttled
    separately;
  - queue refusals for floods or malformed messages.

  Inside a match, they are also span events on the match span.
- **Shipping.** A Grafana Alloy container in the same profile does three
  things:
  - It scrapes both `/metrics` endpoints and remote-writes them, so
    Prometheus in `lgtm` no longer scrapes.
  - It reads the `server`, `door` and `caddy` container logs from the Docker
    socket and pushes them to Loki's own API. That API keeps the
    `service_name` label; Loki's OTLP intake would replace it with the
    resource's name.
  - It forwards OTLP traces.

  Destinations come from three environment variables, so a remote or managed
  stack needs no code or config change.
- **Runtime gauges.** `tokio_workers`, `tokio_alive_tasks` and
  `tokio_global_queue_depth` (stable Tokio metrics). Task poll histograms
  would need `tokio_unstable` and are left out.
- **Alerts.** Eight provisioned Grafana rules post to a webhook contact point
  (`ARENA_ALERT_WEBHOOK_URL`).
- **Deferred: metric exemplars.** The issue mentions linking metrics to
  traces through exemplars. Our histograms are rendered as plain Prometheus
  text with no exemplars. Exemplars on the tick histogram would also need a
  trace context on the tick path, which the hard boundaries forbid. Logs and
  traces are linked; metrics and traces are not.
- **Topology** for early, public and managed setups is documented in
  `docs/DEPLOYMENT.md`.

### Phase 3: browser reports and profiling

- **Scrubbed browser reports, not a RUM SDK.** Grafana Faro would add a
  third-party bundle and a second collector path. Instead the page sends
  small JSON reports to the door's `POST /api/telemetry`, which logs them;
  Alloy already ships door logs to Loki. The page reports:
  - uncaught errors and unhandled rejections;
  - WASM panics (`RuntimeError`/`unreachable`);
  - WebGL context loss;
  - the time to the engine's first state;
  - one summary per finished match (p95 RTT, corrections, reconnects, hash
    mismatches, underruns, median tick rate, median engine frame).

  There is never per-frame telemetry.
- **Scrubbing on both sides.** `page` is the origin and path only, and text
  loses URL passwords, any query or fragment, and any hex run of 16 or more
  digits. Shared fixture cases keep the two scrubbers identical. Unit tests in
  `npm test` and `cargo test` prove that a join link's ticket never reaches a
  report or a log line. The door also enforces:
  - a closed schema with no unknown fields;
  - a 2 KB body limit;
  - a per-address rate limit and an origin check;
  - bounded numbers.

  The page sends at most 20 error reports per load. Match summaries have a
  separate cap of 200, so a long queue session keeps reporting. Nothing is
  sent under Do Not Track or Global Privacy Control, or after
  `?telemetry=off`, which is remembered on the device. Failures the page
  catches itself, such as an engine that fails to load, are reported too.
  The door logs reports at `info`, because they are client claims.
- **Opt-in Pyroscope profiling.** An optional `profiling` feature pulls in
  `pyroscope` 2.1.5 with its pprof backend and without TLS, since the server
  is on the private network. The Docker image builds it in.
  - It starts only when `ARENA_PYROSCOPE_URL` is set, sampling at 100 Hz
    (about 1% CPU by Pyroscope's estimate; not measured here).
  - It is started before and stopped after the Tokio runtime, like the OTLP
    exporter. Stopping waits at most 11 s, the agent's 10 s cycle plus the
    upload, so an unreachable Pyroscope cannot outlast the container's grace
    period. That period is now 1230 s.
  - Its `log` output is bridged into the process logs.
  - Default builds leave it out. `scripts/check.sh` runs Clippy on it
    separately.

## Consequences

- `arena-server` gains `tracing` and `tracing-subscriber` (pinned). No
  exporter, socket or background task exists unless configured; with no
  `obs` profile, nothing changes but log formatting.
- Log lines are now machine-filterable; text that operators grepped for
  (`match … started`) changed wording, documented in `docs/DEPLOYMENT.md`.
- Quantiles come from fixed buckets, so precision is limited to the bucket
  edges. Ticks are truncated to whole microseconds, so sub-microsecond ticks
  count as 0 µs.
- Verified on 2026-11-25 (see `docs/DEPLOYMENT.md`, "Observability trial"):
  the `obs` profile scraped both services and every dashboard query returned
  data; an A/B `arena-bot` load showed no tick-latency regression.
- The process metrics assume Linux's fixed `USER_HZ` of 100 and are omitted
  where `/proc` is missing.
