Follow-ups from the network and prediction work (ADR 0044, `docs/research/match-server-load.md`) that need measurements before acting.

Status (2026-11-21):
- **Done:** Real-network link simulation (`--loss-pct` TCP HoL stalls, `--clump-ms` relay clumping) and browser-parity pacing (`Pacer` + `TickRate::observe_frame`) landed in `arena-bot` (PR #69).
- **Measured ([netcode-measurements-2026-11-21.md](../../docs/research/netcode-measurements-2026-11-21.md)):**
  - **Server CPU scaling:** Profiled 300 → 2,004 3v3 bots (`0.27 → 3.70` cores, down 17.6% from 4.49 on 2026-11-15, 0 overruns). Match ticks are only 2.1% of CPU; superlinear growth is in non-tick userspace per-connection task/WS work (`0.11 → 2.48` cores). Keep current transport until >2,000 players/process is needed.
  - **Targeting fairness:** Measured 7.94% `OutOfRange` disagreements at 0 ms RTT + 100 ms interp (vs 0.00% at 0 ms interp), 9.1–9.3% at 50–100 ms RTT, 15.5% at 200 ms RTT, and 18.4% under 2% TCP loss + 40 ms clumping. Actionable candidate for a bounded lag-comp / range-forgiveness or adaptive-interp ADR.
  - **Snapshot bandwidth:** Live v3 3v3 active combat averages 156.6 B/snap (4.70 KB/s = 37.6 kbit/s per player). Lossless timer-decay deltas (`arena_protocol::delta::SnapshotDelta`) cut 3v3 payload by 57.9% (to 1.98 KB/s with `hash`). Keep full 30 Hz snapshots for now (do not lower rate to 15 Hz, which worsens targeting fairness).

Remaining candidates:
- Evaluate bounded server-side targeting lag compensation / range forgiveness and/or adaptive low-jitter interpolation delay (ADR) based on the 2026-11-21 fairness measurements.
- Evaluate WebTransport only if real WAN playtests show TCP head-of-line stalls matter beyond the simulated loss findings.
- Keep the development diagnostics (`--dev` per-seat lines, overlay rows) useful as the protocol grows.
