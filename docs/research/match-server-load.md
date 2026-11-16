# Match server latency and load baseline

Date: 2026-11-15. **Conclusion: on one development machine, one
`arena-server` process held about 2,000 concurrent 3v3 bot players at 30 Hz
on about 4.5 logical CPUs. Every tick was on time, and server latency added
no more than one tick to the simulated network. Horizontal per-match scaling
(ADR 0044) is not needed for CPU at a few thousand players, but remains the
plan for regions and failure isolation.** These are loopback measurements on a
shared machine, not real network or production hardware results.

## Setup

- AMD Ryzen 7 9800X3D (8 cores, 16 logical CPUs), Linux 6.6 under WSL2.
  Release builds of `arena-server` and `arena-bot` from the ADR 0044 change.
- Server: `arena-server --dev`, real 30 Hz tick, default limits.
- Bots: `arena-bot` (ADR 0044, Phase 4). Each bot seat sends 30 inputs and 1
  ping per second, chases the nearest enemy, attacks in range and
  occasionally jumps. Both directions pass through a simulated link (base
  RTT plus uniform jitter per direction, TCP ordering kept). Bots measure
  ping RTT, input-to-acknowledgement latency, snapshot spacing and payload
  bandwidth. They also cross-check that every seat of a match receives the
  same state hash for the same tick. A group starts a new match whenever a
  round ends.
- The load runs pinned the server to logical CPUs 0–7 and the bots to 8–15
  with `taskset`. Server CPU is user plus system time from `/proc` divided by
  wall time.

## Two players at increasing latency (20 s each, 1v1)

| Link | Ping RTT p50 / p99 | Input → ack p50 / p99 | Snapshot gap p50 / p99 | Hash checks / mismatches | Errors / lost |
| --- | --- | --- | --- | --- | --- |
| 50 ms + up to 10 ms jitter each way | 63 / 71 ms | 67 / 104 ms | 33 / 43 ms | 445 / 0 | 0 / 0 |
| 100 ms + up to 20 ms | 132 / 150 ms | 139 / 168 ms | 34 / 52 ms | 439 / 0 | 0 / 0 |
| 200 ms + up to 40 ms | 259 / 296 ms | 276 / 302 ms | 33 / 67 ms | 432 / 0 | 0 / 0 |

An input is acknowledged after one simulated round trip plus up to one tick
(33 ms). Snapshot spacing stays at the tick interval; its tail is jitter. A
20 s run with each bot dropping out every 5 s rejoined 4 of 6 times, and the
other 2 rejoins correctly found the round already won (`MatchOver`). The
remaining player saw the leaver as disconnected in 66 snapshots, with no
errors or hash mismatches.

## Load (30 s each, 3v3, 50 ms + up to 10 ms jitter)

| Bots | Server CPU (of 8) | Server peak RSS | Input → ack p50 / p99 | Snapshot gap p99 | Late ticks | Dropped frames | Hash checks / mismatches |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 1,002 | 1.68 | 149 MiB | 78 / 104 ms | 43 ms | 0 | 0 | 565,756 / 0 |
| 2,004 | 4.49 | 288 MiB | 79 / 105 ms | 44 ms | 0 | 0 | 1,142,336 / 0 |
| 3,000 | 6.82 | 420 MiB | 114 / 196 ms | 98 ms | 0 | 0 | 1,740,358 / 0 |

Payload per bot was about 20.7 kbit/s inbound (3v3 snapshots at 30 Hz),
before WebSocket, TCP and TLS overhead.

At 3,000 bots both processes were near their eight logical CPUs (the bots
used 5.3). The bots sent only about 1.9 M of the roughly 2.7 M inputs
expected, so this run shows where this machine saturates, not the server's
own limit. An unpinned run on all 16 CPUs gave similar server CPU figures
(0.29 at 300 bots, 1.67 at 1,002, 4.63 at 2,004).

## Prediction

Date: 2026-11-15, same machine, release builds, after the server switched
to one input per tick (ADR 0044). The bots predict their own movement with
the browser's `Predictor` and count corrections: reconciliations that
moved the prediction.

| Run | Predicted inputs | Corrections | Per 1000 | Size p50 / max |
| --- | --- | --- | --- | --- |
| 1v1, 0 ms, 20 s | 890 | 0 | 0.0 | — |
| 1v1, 50 ms + up to 10 ms each way | 880 | 0 | 0.0 | — |
| 1v1, 100 ms + up to 20 ms | 876 | 0 | 0.0 | — |
| 1v1, 200 ms + up to 40 ms | 862 | 0 | 0.0 | — |
| 50 × 1v1, 100 ms + 20, 30 s | 58,250 | 1 | 0.02 | 242 mm |
| 50 × 3v3, 100 ms + 20, 30 s | 200,610 | 24 | 0.1 | 267 / 760 mm |
| 50 × 3v3, 100 ms + 20, drop-outs every 5 s | 178,863 | 23 | 0.1 | 246 / 900 mm |
| 50 × 3v3, 200 ms + 40, drop-outs every 5 s | 176,154 | 55 | 0.3 | 242 / 1,200 mm |

Two bugs were found and fixed while measuring:

- Inputs sent while a match was still waiting were not recorded, but the
  server applied any that arrived after it started. This caused a
  correction for each input in flight at every round start: 4–9 per 1v1
  run.
- A rejoin kept the seat's old acknowledgement. That caused 18.8
  corrections per 1000 inputs in the drop-out run, down to 0.1 after the
  fix.

A run with jumps disabled ruled out jumping as the cause of the first
problem. The remaining corrections match real disagreements: a fighter
killed while its prediction still moves (up to latency ÷ 33 ms steps,
hence the larger maxima at 200 ms), and a late input mid-jump, where an
idle tick still advances the arc (at most a step plus a tick of rise or
fall).

## Findings

- **Consistency:** about 3.4 M cross-seat hash comparisons in the pinned
  load runs alone (and every other run) found no divergence. Every player of a match was told the same state.
- **The simulation is not the cost.** The slowest single match tick
  (including encoding and queuing six snapshots) was 4.5–11.7 ms at the
  worst moment of a run, with no tick ever late. Most server CPU therefore
  likely goes to per-message WebSocket work, though the server has not been
  profiled.
- **Server CPU grows faster than linearly** (1.68 → 4.49 cores for 2× the
  players). Likely causes, not yet profiled: one socket write and one
  cross-task wake-up per snapshot, and contention on a shared, virtualized
  machine. Candidate optimizations, if needed: batch or rate-reduce
  snapshots, send deltas, and write snapshots straight from the match task.
- **Bug found and fixed by the bots:** when a match ended while a player's
  input was in flight, the connection could close without delivering
  `MatchOver`. Connections now deliver the match's queued final message
  before closing (`forward` in `arena-server`). The pinned runs above follow
  the fix and saw zero unexpected closes. Earlier unpinned runs saw 2–5 per
  run.
- **Not covered:** real networks, packet loss, TLS, browsers, other operating
  systems and dedicated hardware. Repeat the load runs on the intended host
  before sizing a deployment.

## Reproduce

```sh
cargo build --release -p arena-server -p arena-bot
taskset -c 0-7 ./target/release/arena-server --dev --bind 127.0.0.1:18090 &
taskset -c 8-15 ./target/release/arena-bot --server 127.0.0.1:18090 \
  --matches 167 --team-size 3 --seconds 30 --rtt 50 --jitter 10 --server-pid $!
```
