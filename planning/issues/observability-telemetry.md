# Production-Grade Observability & Telemetry (OpenTelemetry + Self-Hosted Grafana LGTM)

Add Datadog-style structured logging, metrics, distributed tracing (APM), dashboards, alerting, continuous profiling, and browser error/session telemetry that run identically **locally and in production** with **$0 software licensing cost** and zero vendor lock-in.

## Architecture & Stack Choice

Instrument once with **OpenTelemetry (OTLP)** + **Prometheus `/metrics`**, and run the open-source **Grafana LGTM** stack locally and in self-hosted production:

- **Instrumentation (Apache 2.0):** Rust `tracing` + `tracing-subscriber` + `tracing-opentelemetry` + `opentelemetry-otlp` (`arena-server`, `arena-door`), lock-free Prometheus `/metrics`, and OpenTelemetry Web / Grafana Faro for scrubbed browser telemetry.
- **Collection & Storage (OSS — AGPLv3 / Apache 2.0):**
  - **Local dev:** single-container `grafana/otel-lgtm` behind an opt-in Docker Compose profile (`docker compose --profile obs up`) bound strictly to `127.0.0.1` (`3000` Grafana UI, `4317` OTLP gRPC, `4318` OTLP HTTP).
  - **Self-hosted prod:** Grafana + Prometheus + Loki (logs) + Tempo (traces) + Pyroscope (continuous CPU profiling) fed by Grafana Alloy or OpenTelemetry Collector.
  - **Portability:** switching to managed Grafana Cloud or Datadog later requires only changing `OTEL_EXPORTER_OTLP_ENDPOINT` and exporter credentials—no Rust or TypeScript code changes.

## Hard Boundaries & Game-Server Constraints

1. **Keep `crates/arena-sim` 100% clean.** Never add `tracing`, OpenTelemetry, or OS clocks to `arena-sim`. Simulation remains pure and deterministic.
2. **Never trace the 30 Hz tick loop or per-frame WebSocket inputs.** At 2,000+ concurrent bots (~10,000 ticks/sec, ~60,000 inputs/sec), per-tick or per-packet spans destroy CPU/memory and blow up trace ingestion.
   - **Hot path (30 Hz ticks & frames) → Lock-free atomic metrics & histograms only.**
   - **Control plane & anomalies → OTel spans & structured events only.**
3. **Never leak secrets or tokens.** Never log `ARENA_TICKET_KEY`, raw HMAC join tickets, or raw URL hashes/query parameters in `arena-server`, `arena-door`, Caddy, or browser telemetry.
4. **Keep observability off the public internet.** `/metrics`, OTLP receivers, and Grafana stay on loopback or the private Docker network (accessed via SSH tunnel, Tailscale, or WireGuard), never exposed unauthenticated through Caddy.
5. **Zero overhead when disabled.** When `OTEL_EXPORTER_OTLP_ENDPOINT` is unset (ordinary `cargo run`, CI, unit tests), no OTLP exporter runs and no network sockets are opened.
6. **Pin all dependencies** in `Cargo.toml` / `deploy/compose.yaml` and preserve workspace `unsafe_code = "forbid"`.

## Phased Rollout & Acceptance Criteria

Record the architectural design as a numbered ADR in `docs/decisions/` when starting Phase 1.

### Phase 1 — Structured Logging, Tick Histograms, `arena-door` `/metrics` & Local LGTM

- **Structured logging (`tracing`):**
  - Replace all `eprintln!` calls in `arena-server` and `arena-door` with `tracing::{info, warn, error, debug}!`.
  - Support `ARENA_LOG_FORMAT=pretty` (default locally for readable terminal logs) and `ARENA_LOG_FORMAT=json` (default in Docker/prod, matching Caddy's JSON stdout logs in `deploy/Caddyfile`), with `RUST_LOG` filtering.
  - Attach consistent structured fields (`match_id`, `seat`, `conn_id`, `team_size`, `error_code`, `reason`) so any match lifecycle can be filtered end-to-end.
  - Verify via tests/code review that tickets and signing keys are never emitted in log fields.
- **Lock-free tick-duration histograms (`crates/arena-server/src/metrics.rs`):**
  - Add a zero-allocation, cache-line-padded, fixed-bucket Prometheus histogram `arena_tick_duration_microseconds` (alongside existing `arena_tick_max_microseconds`, `arena_tick_microseconds_total`, and `arena_tick_overruns_total`) so dashboards report true p50/p95/p99 tick latency.
- **`arena-door` Prometheus `/metrics`:**
  - Expose a private `/metrics` endpoint on `arena-door` covering active queues, queue wait duration histogram, active parties, matchmaker pops/accepts/declines, tickets minted, and origin/rate-limit refusals.
- **Local LGTM Compose profile & provisioned dashboards:**
  - Add an opt-in `obs` profile to `deploy/compose.yaml` running `grafana/otel-lgtm` bound to `127.0.0.1` plus Prometheus scrape configuration for `server:8080/metrics` and `door:8090/metrics`.
  - Commit provisioned Grafana dashboard JSON files (e.g. under `deploy/observability/dashboards/`) for:
    1. **Match Server Health** (active matches, connections, errors by code, memory/CPU, drain state).
    2. **Tick Latency & Input Jitter** (tick duration p50/p95/p99, tick overruns, input arrival buckets `0/1/2/3/4+`, starved/folded/stale inputs, dropped frames, bytes out).
    3. **Front Door & Matchmaking** (queue depth, wait time, party sizes, ready-check accept/decline rates, rate-limit/origin refusals).
- **Verification:** `./scripts/check.sh` passes; `arena-bot` load baseline confirms no tick-latency regression; local `docker compose --profile obs up` serves populated dashboards (manual check recorded honestly in docs).

### Phase 2 — OpenTelemetry Distributed Tracing (OTLP), Log Shipping & Alerts

- **OTLP tracing in `arena-server` and `arena-door`:**
  - Wire `tracing-opentelemetry` + `opentelemetry-otlp` (enabled only when `OTEL_EXPORTER_OTLP_ENDPOINT` is set) and `tower-http` HTTP tracing layers for Axum routes.
  - Emit spans on control-plane boundaries: `POST /api/matches`, party create/join, queue enter → matchmaker pop → ready-check accept → ticket mint, WS upgrade + origin/rate-limit check, `Hello`/ticket verification, seat join/disconnect/reconnect/replace, match start/end, and SIGTERM drain.
  - Emit structured anomaly events on tick overruns (`> 33.3 ms`), `RateLimited` / `BadTicket` / `Malformed` errors, and dropped send-queue frames.
  - Inject `trace_id` and `span_id` into JSON logs so Grafana links logs ↔ traces ↔ metrics exemplars.
- **Log & metric shipping for Docker/prod:**
  - Add Grafana Alloy (or OpenTelemetry Collector) config in `deploy/observability/` to ship Caddy, `arena-server`, and `arena-door` JSON stdout logs to Loki, scrape `/metrics` to Prometheus, and forward OTLP traces to Tempo.
  - Include Tokio runtime / process metrics (RSS, CPU, open FDs, task poll metrics).
- **Provisioned alert rules & production topology docs:**
  - Provision baseline alert rules (tick overrun rate, `BadTicket`/`RateLimited` spike, readiness failing, drain approaching grace timeout, high memory/FD usage) with configurable webhook receiver (Discord/Slack/email).
  - Document self-hosted production guidance in `docs/DEPLOYMENT.md`:
    - **Early playtests ($0 extra):** co-located LGTM with strict memory limits (~512 MB–1 GB) and short retention (7 days).
    - **Live/public playtests (~$5–$12/mo):** dedicated small VPS over private network / WireGuard / Tailscale so observability storage/queries cannot compete with the 30 Hz match loop and survives game-server crashes.
    - **Managed swap:** env-var override to forward OTLP/Prometheus/Loki to Grafana Cloud or Datadog.

### Phase 3 — Browser Error/Session Telemetry (RUM) & Continuous Profiling

- **Scrubbed browser telemetry (`web/`):**
  - Integrate Grafana Faro Web SDK (or a thin, rate-limited `/api/telemetry` ingest forwarded to Loki/Tempo) to capture uncaught JS errors, unhandled rejections, WebGL2 context loss, WASM panics, and startup render-warmup timing.
  - Emit **one summary beacon per completed match** (reusing `web/src/runtime/diagnostics.ts` stats: p95 RTT, total reconciliation corrections, reconnect count, median engine cadence)—never per-frame telemetry.
  - Enforce URL hash/query scrubbing in unit tests (`npm test`) so join tickets and tokens can never leave the browser in telemetry payloads.
- **Continuous CPU profiling (Pyroscope):**
  - Add opt-in continuous profiling (`pprof` / Pyroscope push, ~1% overhead, disabled by default) for `arena-server` load tests and production playtests so tick p99 spikes map directly to Rust flamegraphs.
