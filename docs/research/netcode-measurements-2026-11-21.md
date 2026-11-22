# Netcode follow-up measurements: CPU scaling, targeting fairness, and snapshot bandwidth

Date: 2026-11-21. Follow-up to [match-server-load.md](match-server-load.md) and
[`planning/issues/netcode-hardening.md`](../../planning/issues/netcode-hardening.md),
measured on `PROTOCOL_VERSION = 3` / `RULES_VERSION = 4` (after PR #61 shared
snapshot encoding / padded metrics and PR #67 server-authoritative casting,
cooldowns, resources, and interrupts).

## Summary of decisions

1. **Server CPU scaling (do not rewrite transport yet):** One `arena-server`
   process held **2,004 concurrent 3v3 bots on 3.70 cores** (down 17.6% from
   4.49 cores on 2026-11-15 despite larger v3 state), with **0 tick overruns**,
   **0 dropped frames**, and **0 hash mismatches** across 710,788 cross-seat
   checks. Match ticks (`Arena::advance` + `EncodedSnapshotBody` + channel send)
   account for only **2.1% of server CPU** (0.076 of 3.70 cores; 10.6 µs mean
   tick). The superlinear growth from 300 to 2,004 bots (`39.7 → 85.6 µs/snap`)
   is concentrated in **non-tick userspace connection-task work** (`0.11 → 2.48`
   cores: per-connection Tokio tasks, WebSocket framing/unmasking, and
   cross-task wake-ups), not simulation or match count. If single-process
   density beyond 2,000 players is ever needed, writing snapshots directly from
   the match task to split sinks is the clear target.
2. **Targeting fairness (actionable follow-up recommended):** With the browser's
   100 ms (`INTERPOLATION_DELAY = 3` ticks) remote interpolation buffer,
   **7.94% of range/LoS-evaluated casts fail with `OutOfRange` disagreements
   even at 0 ms RTT** (vs 0.00% at 0 ms interp), rising to **9.1–9.3% at
   50–100 ms RTT**, **15.5% at 200 ms RTT**, and **18.4% at 100 ms RTT with 2%
   TCP packet loss and 40 ms relay clumping** (receding distance error p99
   `1.36–2.72 m` against Pulse's 6.0 m range). This justifies evaluating bounded
   server-side range tolerance / lag compensation and/or adaptive low-jitter
   interpolation delay via a dedicated ADR.
3. **Snapshot bandwidth (keep full 30 Hz snapshots for now; do not lower rate):**
   Steady-state live v3 3v3 combat averages **156.6 B/snapshot (37.58 kbit/s =
   4.70 KB/s per player, payload only)** — up from the v1 spawn-only estimate of
   3 KB/s after casting/cooldown/resource fields landed, and still modest for
   broadband/mobile links (~17 MB/hour). A verified lossless field-mask +
   timer-decay delta codec (`arena_protocol::delta::SnapshotDelta`) cuts 3v3
   bandwidth by **57.9%** (to **15.81 kbit/s = 1.98 KB/s** keeping `hash`, or
   **13.53 kbit/s / −64.0%** without `hash`), while rate reduction to 15 Hz
   saves 49.5% at the cost of +33 ms interpolation delay (which directly worsens
   targeting fairness) and halved prediction reconciliation cadence. Full 30 Hz
   snapshots remain the right default until future Effects or larger rosters
   materially raise per-snapshot size.

## Setup

- Host: AMD Ryzen 7 9800X3D (8 cores, 16 logical CPUs), Linux under WSL2.
  Release builds (`cargo build --release -p arena-server -p arena-bot --locked`).
- Pinning: `arena-server --dev` pinned to logical CPUs `0–7`; `arena-bot`
  pinned to `8–15` via `taskset` (matching the 2026-11-15 baseline).
- Process profiling: `arena-bot --server-pid <pid>` reads `/proc/<pid>/stat`
  (user vs kernel CPU), `/proc/<pid>/status` (`VmHWM`, `Threads`),
  `/proc/<pid>/task/*/status` (voluntary and non-voluntary context switches
  summed across all OS threads), and start/end deltas of `/metrics`
  `arena_tick_microseconds_total` (`match_tick_cpu_cores`).
- Scope limits: loopback TCP without TLS, synthetic bot movement/casting on the
  default Colosseum layout (`assets/maps/colosseum.json`), and no real browsers
  or WAN links.

---

## 1. `arena-server` CPU scaling & breakdown

### 3v3 scaling ladder (20 s each, 50 ms RTT + up to 10 ms jitter, paced)

| Bots (3v3 matches) | Total CPU (of 8) | Match Tick CPU | Non-Tick User CPU | Kernel Sys CPU | µs / snap | Peak RSS | Vol CS/s (per snap) | Tick mean / max | Overruns | Input → ack p50 / p99 | Snapshot gap p99 | Hash checks / mismatches |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 300 (50) | 0.27 | 0.007 (2.6%) | 0.11 (41%) | 0.15 (56%) | 39.7 | 52.7 MiB | 3,705 (0.55) | 6.2 / 92 µs | 0 | 127.6 / 146.9 ms | 42.2 ms | 110,995 / 0 |
| 600 (100) | 0.70 | 0.016 (2.3%) | 0.39 (56%) | 0.30 (43%) | 53.3 | 93.4 MiB | 5,234 (0.40) | 7.4 / 311 µs | 0 | 127.7 / 147.3 ms | 42.3 ms | 217,180 / 0 |
| 1,002 (167) | 1.47 | 0.032 (2.2%) | 0.88 (60%) | 0.56 (38%) | 67.4 | 146.9 MiB | 6,870 (0.31) | 8.9 / 346 µs | 0 | 127.8 / 147.3 ms | 42.4 ms | 357,418 / 0 |
| 1,500 (250) | 2.65 | 0.053 (2.0%) | 1.71 (65%) | 0.89 (34%) | 80.8 | 217.2 MiB | 8,651 (0.26) | 9.8 / 561 µs | 0 | 128.3 / 148.4 ms | 42.8 ms | 537,881 / 0 |
| 2,004 (334) | 3.70 | 0.076 (2.1%) | 2.48 (67%) | 1.15 (31%) | 85.6 | 281.3 MiB | 10,516 (0.24) | 10.6 / 1,810 µs | 0 | 128.4 / 148.6 ms | 42.8 ms | 710,788 / 0 |

### Constant 300 players across team sizes (20 s each, 50 ms RTT + 10 ms jitter)

| Roster | Matches | Total CPU | Match Tick CPU | Non-Tick User CPU | Kernel Sys CPU | µs / snap | Peak RSS | Tick mean / max |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 1v1 | 150 | 0.26 | 0.007 | 0.11 | 0.15 | 38.8 | 50.9 MiB | 2.0 / 216 µs |
| 2v2 | 75 | 0.27 | 0.007 | 0.10 | 0.16 | 40.5 | 50.8 MiB | 4.2 / 147 µs |
| 3v3 | 50 | 0.27 | 0.007 | 0.11 | 0.15 | 39.7 | 52.7 MiB | 6.2 / 92 µs |

### Analysis

- **Compared to 2026-11-15 baseline:** Total server CPU dropped from `1.68 → 1.47`
  cores at 1,002 bots (−12.5%) and `4.49 → 3.70` cores at 2,004 bots (−17.6%),
  and slowest tick fell from `4.5–11.7 ms` to `0.35–1.81 ms`, reflecting PR #61
  (`EncodedSnapshotBody` + cache-line-isolated `Metrics`) even after PR #67
  expanded combat state and snapshot payloads.
- **Simulation + encoding is not the bottleneck:** Across all scales,
  `match_tick_cpu_cores` stays at **2.0–2.6%** of total server CPU.
- **Connection count, not match count, governs cost:** Tripling match tasks
  (50×3v3 → 150×1v1 at 300 bots) left total CPU flat (`0.27 → 0.26` cores).
- **Source of superlinear scaling:** Moving from 300 to 2,004 bots (6.68×
  players), kernel `sys` CPU scales almost linearly (`0.15 → 1.15` cores, 7.7×),
  while `non_tick_user_cores` grows **22.5×** (`0.11 → 2.48` cores, rising from
  41% to 67% of CPU). Voluntary context switches per snapshot actually fall
  (`0.55 → 0.24`) as worker threads stay busy pulling ready connection tasks
  from Tokio's run queues rather than parking, exposing work-stealing / cross-task
  `mpsc` wake-up and per-connection WebSocket task scheduling overhead at
  ~60,000 snapshots/s + ~60,000 inputs/s.

---

## 2. Targeting-fairness / lag-compensation disagreement rates

Bots play using the browser-parity decision view (`arena_bot::fairness::ViewBuffer`):
own fighter position/facing from local `Predictor`, own combat timers from
`latest`, and remote fighters delayed by `--interp-delay-ms` (default 100 ms = 3
ticks at 30 Hz, matching `arena_client::network::INTERPOLATION_DELAY`). Each
targeted cast's `InputFrame.seq` is matched 1-to-1 against `Snapshot.ack` to
compare what the client saw against the server's authoritative verdict.

To avoid diluting rejection rates with duplicate in-flight inputs rejected on
`GlobalCooldown` or `Cooldown` (which `arena-sim::casting` validates before
`check_target_geometry`), `OOR disagree` divides `OutOfRange` disagreements by
casts that actually reached geometry validation while client-perceived in-range
(`casts_reached_geometry_in_range`).

### Matrix (20 × 2v2 = 80 bots, 25 s each, Colosseum)

| Condition | Resolved / Reached Geom / Accepted | OOR Disagreed / Eligible (**Rate**) | Geom OOR Rate (all casts) | LoS Disagreed | Target-Dead Disagreed | Distance Error p50 / p99 / max | Receding Error p99 / max |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 0 ms RTT, 0 ms interp (control) | 1,368 / 575 / 575 | 0 / 575 (**0.00%**) | 0.07% | 0 / 575 (0.00%) | 85 | 161 / 950 / 957 mm | 628 / 811 mm |
| 0 ms RTT, 100 ms interp | 1,432 / 617 / 568 | 49 / 617 (**7.94%**) | 3.70% | 0 / 617 (0.00%) | 95 | 702 / 1,667 / 1,674 mm | 1,364 / 1,492 mm |
| 50 ms RTT + 10 ms jit, 100 ms interp | 1,808 / 583 / 529 | 54 / 583 (**9.26%**) | 3.04% | 0 / 583 (0.00%) | 103 | 826 / 1,900 / 2,152 mm | 1,684 / 1,867 mm |
| 100 ms RTT + 20 ms jit, 0 ms interp | 2,404 / 528 / 518 | 10 / 528 (**1.89%**) | 0.67% | 0 / 528 (0.00%) | 192 | 545 / 1,657 / 1,912 mm | 1,439 / 1,610 mm |
| 100 ms RTT + 20 ms jit, 100 ms interp | 2,326 / 551 / 501 | 50 / 551 (**9.07%**) | 2.67% | 0 / 551 (0.00%) | 180 | 755 / 2,360 / 2,385 mm | 1,997 / 2,373 mm |
| 200 ms RTT + 40 ms jit, 100 ms interp | 3,251 / 511 / 432 | 79 / 511 (**15.46%**) | 3.51% | 0 / 511 (0.00%) | 297 | 764 / 3,301 / 3,533 mm | 2,690 / 3,275 mm |
| 100 ms RTT + 20 ms jit + 2% loss + 40 ms clump, 100 ms interp | 3,219 / 538 / 439 | 99 / 538 (**18.40%**) | 4.32% | 0 / 538 (0.00%) | 245 | 709 / 3,306 / 3,991 mm | 2,721 / 3,687 mm |

### Analysis

- **The 100 ms interpolation buffer is the largest single contributor at normal
  pings:** Comparing `0 ms RTT + 100 ms interp` (**7.94%**) against `100 ms RTT
  + 0 ms interp` (**1.89%**) shows that the fixed 3-tick remote interpolation
  delay creates ~4× as many `OutOfRange` disagreements as 100 ms of network RTT
  alone. At 7.2 m/s movement speed (`240 mm/tick`), 100 ms of remote staleness
  adds up to `720–1,440 mm` of relative separation before network transit.
- **High latency and TCP head-of-line stalls double the rate:** At 200 ms RTT
  disagreements reach **15.46%** (p99 receding error `2.69 m`), and under 2%
  TCP loss + 40 ms relay clumping at 100 ms RTT they reach **18.40%** (`2.72 m`
  p99 receding error, with 8 folded casts).
- **Target-Dead Disagreed note:** `Target-Dead Disagreed` counts every
  `WireRejection::InvalidTarget` where the client still saw the target alive;
  because `arena-sim::casting` runs `resolve_target` before cooldown and GCD
  checks, this raw count includes repeated in-flight casts sent while a lethal
  hit is still in transit (explaining the 85 count in the 0 ms control).
- **Line-of-sight caveat:** Bot skirmishes on Colosseum occurred primarily in
  the open center corridor (`0` LoS blocks in these 2v2 runs), so LoS
  disagreements were 0.00% here. Human players actively kiting around the four
  hexagonal pillars will encounter LoS disagreements across the same `0.7–2.7 m`
  positional gap.
- **Recommendation:** Worth addressing in a dedicated follow-up ADR (server-side
  bounded range forgiveness / history-rewind lag compensation and/or adaptive
  interpolation delay on low-jitter links). **Follow-up (2026-11-21):**
  implemented in [ADR 0049](../decisions/0049-bounded-targeting-lag-comp-and-adaptive-interp.md),
  which reduced `OutOfRange` disagreements to **0.18–0.57%** across 0–100 ms RTT
  (from 7.94–9.26%), **6.42%** at 200 ms RTT (from 15.46%), and **8.12%** under
  2% TCP loss + 40 ms clumping (from 18.40%).

---

## 3. Snapshot bandwidth: full 30 Hz vs deltas & rate reductions

Measured online during 25 s of active combat per mode (20 matches each, 50 ms
RTT + 10 ms jitter) using `arena_protocol::delta::SnapshotDelta` (round-trip
lossless `postcard` field-mask + timer-decay-predicted delta codec) and exact
20 Hz / 15 Hz event-preserving decimation (`arena_bot::bandwidth`). Bitrates
reflect steady-state active combat (`bytes × 8 / 1000 / (snapshots_30hz / 30)`),
excluding waiting and post-round linger idle gaps so `kbit/s = mean_bytes × 30 × 8 / 1000`.

### Payload bandwidth per player (active 30 Hz combat, before WebSocket, TCP, and TLS overhead)

| Mode (20 matches, 25 s) | Full 30 Hz (mean / min–max B) | Full 30 Hz | Delta 30 Hz (with hash) | Delta 30 Hz (no hash) | Full 20 Hz | Delta 20 Hz | Full 15 Hz | Delta 15 Hz |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| **1v1** (40 bots) | 64.0 B (56–84) | **15.36 kbit/s** (1.92 KB/s) | 8.11 kbit/s (**−47.2%**, 33.8 B) | 5.83 kbit/s (−62.0%) | 10.30 (−33.0%) | 5.55 (−63.9%) | 7.75 (−49.5%) | 4.27 (−72.2%) |
| **2v2** (80 bots) | 110.5 B (98–147) | **26.51 kbit/s** (3.31 KB/s) | 12.20 kbit/s (**−54.0%**, 50.8 B) | 9.93 kbit/s (−62.5%) | 17.77 (−33.0%) | 8.42 (−68.2%) | 13.39 (−49.5%) | 6.53 (−75.3%) |
| **3v3** (120 bots) | 156.6 B (140–190) | **37.58 kbit/s** (4.70 KB/s) | 15.81 kbit/s (**−57.9%**, 65.9 B) | 13.53 kbit/s (−64.0%) | 25.19 (−33.0%) | 10.95 (−70.9%) | 18.98 (−49.5%) | 8.52 (−77.3%) |

### Fighter field change rates per tick (30 Hz)

| Mode (fighter-ticks) | Position `(x, z)` | Facing | Vertical / Momentum | Health | Resource | Timers (raw → unexpected) | Active cast (raw → unexpected) | Snapshots w/ events |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 1v1 (47,242) | 91.1% | 91.9% | 89.7% | 0.9% | 13.0% | 34.2% → **1.5%** | 8.0% → **0.4%** | 6.4% (0.07/snap) |
| 2v2 (189,104) | 89.7% | 92.1% | 88.6% | 0.8% | 12.1% | 34.2% → **1.5%** | 6.9% → **0.3%** | 12.2% (0.15/snap) |
| 3v3 (425,520) | 85.7% | 88.3% | 84.7% | 0.7% | 10.5% | 33.1% → **1.3%** | 6.1% → **0.3%** | 18.5% (0.23/snap) |

### Analysis

- **Why timer-decay deltas save 57.9%:** Raw cooldown/GCD/lockout values change
  on 33.1% of fighter-ticks (and `active_cast` on 6.1%), but predicting normal
  `−tick_delta` countdown cuts transmitted timer updates by **25×** (to 1.3%)
  and active-cast updates by **20×** (to 0.3%). Note also that
  `vertical_or_momentum` changes on 84.7% of fighter-ticks because grounded
  movement updates `fighter.momentum` every step (`arena.rs`) to carry takeoff
  momentum; a future delta codec could omit `momentum` while `height == 0` and
  `vertical_velocity == 0` for further savings.
- **State hash overhead:** Keeping `Snapshot.hash` costs ~2.28 kbit/s (~9.5
  bytes per snapshot at 30 Hz), or 6.1 percentage points of 3v3 savings — worth
  retaining if deltas are ever adopted so cross-seat consistency checks still
  work.
- **Why full 30 Hz snapshots remain preferred today:**
  - At **4.70 KB/s per player** in active 3v3 combat (~17 MB/hour), payload
    bandwidth is already modest for players and server egress (2,004 concurrent
    3v3 bots = ~9.4 MB/s = ~75 Mbit/s total).
  - Dropping snapshot rate to 15 Hz saves ~2.3 KB/s while adding +33.3 ms to
    minimum remote interpolation delay (directly worsening the 7.9–18.4%
    `OutOfRange` disagreements measured in Section 2) and halving own-movement
    reconciliation rate.
  - Full snapshots allow `EncodedSnapshotBody` to encode fighter state once per
    tick for all six seats and allow the server to drop outbound frames cleanly
    for slow clients without maintaining per-seat delta acknowledgement state.
    Revisit `SnapshotDelta` only if future Effects/auras or larger rosters push
    full 3v3 snapshots materially above ~8–10 KB/s.

---

## Reproduce

```sh
cargo build --release -p arena-server -p arena-bot --locked

# Start server pinned to CPUs 0–7
taskset -c 0-7 ./target/release/arena-server --dev --bind 127.0.0.1:18092 --drain-seconds 0 &
SERVER_PID=$!

# 1. Server CPU scaling (e.g. 2,004 bots = 334 × 3v3)
taskset -c 8-15 ./target/release/arena-bot --server 127.0.0.1:18092 --server-pid "$SERVER_PID" \
  --matches 334 --team-size 3 --seconds 20 --rtt 50 --jitter 10 --json

# 2. Targeting fairness (compare --interp-delay-ms 0 vs 100 across RTT/loss)
taskset -c 8-15 ./target/release/arena-bot --server 127.0.0.1:18092 --server-pid "$SERVER_PID" \
  --matches 20 --team-size 2 --seconds 25 --rtt 100 --jitter 20 --interp-delay-ms 100 --json

# 3. Live combat bandwidth (1v1 / 2v2 / 3v3)
taskset -c 8-15 ./target/release/arena-bot --server 127.0.0.1:18092 --server-pid "$SERVER_PID" \
  --matches 20 --team-size 3 --seconds 25 --rtt 50 --jitter 10 --json

kill -TERM "$SERVER_PID" && wait "$SERVER_PID"
```
