# ADR 0054: Observability: structured logs, histograms and a local LGTM stack

Date: 2026-11-25. **Status: proposed (Phase 1 implemented; Phases 2 and 3 planned).**

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

Phases 2 (OTLP traces, log shipping, alerts) and 3 (browser telemetry,
profiling) stay as written in the issue and will amend this record.

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
