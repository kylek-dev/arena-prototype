Follow-ups from the network and prediction work (ADR 0044, `docs/research/match-server-load.md`) that need measurements before acting.

Status (2026-11-21):
- **Done:** Real-network link simulation (`--loss-pct` TCP HoL stalls, `--clump-ms` relay clumping) and browser-parity pacing (`Pacer` + `TickRate::observe_frame`) landed in `arena-bot` (PR #69).
- **Measured ([netcode-measurements-2026-11-21.md](../../docs/research/netcode-measurements-2026-11-21.md)):**
  - **Server CPU scaling:** Profiled 300 → 2,004 3v3 bots (`0.27 → 3.70` cores, down 17.6% from 4.49 on 2026-11-15, 0 overruns). Match ticks are only 2.1% of CPU; superlinear growth is in non-tick userspace per-connection task/WS work (`0.11 → 2.48` cores). Keep current transport until >2,000 players/process is needed.
  - **Targeting fairness:** Measured 7.94% `OutOfRange` disagreements at 0 ms RTT + 100 ms interp (vs 0.00% at 0 ms interp), 9.1–9.3% at 50–100 ms RTT, 15.5% at 200 ms RTT, and 18.4% under 2% TCP loss + 40 ms clumping. Actionable candidate for a bounded lag-comp / range-forgiveness or adaptive-interp ADR.
  - **Snapshot bandwidth:** Live v3 3v3 active combat averages 156.6 B/snap (4.70 KB/s = 37.6 kbit/s per player). Lossless timer-decay deltas (`arena_protocol::delta::SnapshotDelta`) cut 3v3 payload by 57.9% (to 1.98 KB/s with `hash`). Keep full 30 Hz snapshots for now (do not lower rate to 15 Hz, which worsens targeting fairness).

- **Done (ADR 0049):** Implemented bounded server-side target-position history lag compensation (`MAX_LAG_COMP_TICKS = 6` = 200 ms, monotonic server-clamped `InputFrame.view_tick`, `PROTOCOL_VERSION = 4`, `RULES_VERSION = 5`) and adaptive remote interpolation delay (`arena_protocol::prediction::InterpDelay`, `1.25–5.0` ticks) in `arena-sim`, `arena-protocol`, `arena-server`, `arena-client`, and `arena-bot` ([ADR 0049](../../docs/decisions/0049-bounded-targeting-lag-comp-and-adaptive-interp.md)). Re-measured on the exact 20×2v2 release matrix: `OutOfRange` disagreements dropped from 7.94–9.26% to **0.18–0.57%** across 0–100 ms RTT, from 15.46% to **6.42%** at 200 ms RTT, and from 18.40% to **8.12%** under 2% TCP loss + 40 ms clumping (0 hash mismatches, 0 overruns, 2.2–3.3 µs mean tick).

Remaining candidates:
- Evaluate WebTransport only if real WAN playtests show TCP head-of-line stalls matter beyond the simulated loss findings.
- Keep the development diagnostics (`--dev` per-seat lines, overlay rows) useful as the protocol grows.
- Validate the 200 ms lag-compensation ceiling and adaptive interpolation feel in real human WAN playtests around Colosseum pillars before marking ADR 0049 `Accepted`.
