# Spectator CPU and Bandwidth Measurements (2026-11-22)

Measured the server CPU, tick latency, memory (peak RSS), and outbound payload bandwidth cost of live watch-only spectators using release builds of `arena-server` and `arena-bot` (required by [`planning/issues/spectating.md`](../../planning/issues/spectating.md) and referenced by [ADR 0050](../decisions/0050-spectating-and-anti-ghosting-delay.md)).

## Method

- **Binaries:** `cargo build --release -p arena-server -p arena-bot --locked` on local Linux loopback (`127.0.0.1`).
- **Isolation:** One fresh `arena-server --dev --bind 127.0.0.1:<port> --spectator-delay-seconds 2 --max-spectators 4 --connections-per-address 400` process per condition (so finished/disconnected matches from prior runs cannot contaminate ticks or RSS).
- **Load:** `25 matches × 3v3` (`150` active combat bots, 40 ms RTT, 10 ms jitter, 12.0 s run, `--seed 42`) with `0`, `1`, `2`, and `4` watch-only spectators per match (`0`, `25`, `50`, and `100` concurrent spectators).
- **Why `--spectator-delay-seconds 2` for a 12 s benchmark:** Production defaults to **5 s (150 ticks)** anti-ghosting delay ([ADR 0050](../decisions/0050-spectating-and-anti-ghosting-delay.md)). Using 2 s in the 12 s benchmark ensures ~10 s of steady-state delayed 30 Hz streaming per run while exercising the full delayed-queue buffering and zero-copy `Arc<[u8]>` fanout path.

## Observed Results (Isolated 25 × 3v3 Matches, 12.0 s)

| Spectators / match (total) | Total conns | Server CPU (cores: user / sys) | Mean / Max tick (µs) | Peak RSS (MiB) | CPU µs / snap | Spectator snaps | Spectator payload (wall avg kbit/s) | Active 30 Hz est. (kbit/s) | Private events | Hash checks / mismatches | Overruns / dropped |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `0` (`0`) | 150 | `0.13` (`0.06` / `0.08`) | `5.1` / `66` | `24.9` | `30.8` | `0` | — | — | `0` | `42,795` / `0` | `0` / `0` |
| `1` (`25`) | 175 | `0.15` (`0.06` / `0.09`) | `7.2` / `128` | `28.4` | `30.9` | `7,513` | `31.4` | `~37.6` | `0` | `50,540` / `0` | `0` / `0` |
| `2` (`50`) | 200 | `0.14` (`0.07` / `0.07`) | `6.6` / `102` | `32.4` | `25.8` | `14,924` | `31.2` | `~37.5` | `0` | `57,415` / `0` | `0` / `0` |
| `4` (`100`) | 250 | `0.17` (`0.08` / `0.09`) | `7.8` / `87` | `38.4` | `24.8` | `29,924` | `31.2` | `~37.4` | `0` | `72,630` / `0` | `0` / `0` |

*Note: Spectator wall-average kbit/s (`31.2–31.4 kbit/s`) divides total payload bytes received by the full 12.0 s scenario window, which includes the 2.0 s initial anti-ghosting delay before live tick snapshots begin streaming (~10.0 s active streaming, ~300 snapshots/spectator). Active steady-state 30 Hz 3v3 spectator payload matches the player 3v3 full snapshot size (~157.7 B/snap = ~37.5–37.8 kbit/s, minus private `Rejected` events).*

## Marginal Cost per Spectator (0 → 100 Watchers)

- **CPU:** `+0.04` cores across `+100` spectators (`~0.4` millicores per spectator; match tick mean moved `+2.7 µs` from `5.1 µs` to `7.8 µs`, 0.02% of the `33,333 µs` 30 Hz tick budget). Per-snapshot CPU drops from `30.8 µs/snap` at 0 spectators to `24.8 µs/snap` at 4 spectators/match because each delayed snapshot is serialized once into an `Arc<[u8]>` and fanned out zero-copy (`Bytes::from_owner`) across every spectator in that match.
- **Memory (Peak RSS):** `+13.5 MiB` across `+100` spectators (`~138 KiB` per spectator connection, including Tokio/WebSocket buffers). The per-match delay buffer itself is shared across all spectators in the match (150 ticks × ~160 B ≈ `24 KiB` per match at the default 5 s delay).
- **Bandwidth:** `~37.5 kbit/s` outbound payload per 3v3 spectator at steady-state 30 Hz (`~150 kbit/s` at the default cap of 4 spectators/match, before WebSocket/TLS framing).
- **Correctness & privacy:** `0` tick overruns, `0` dropped frames, `0` unexpected closes, `0` hash mismatches across `72,630` cross-seat + spectator hash checks, and `0` private `WireEvent::Rejected` events across all `29,924` spectator snapshots.

## Reproduction

```bash
cargo build --release -p arena-server -p arena-bot --locked
for S in 0 1 2 4; do
  PORT=$((18950 + S))
  ./target/release/arena-server --dev --bind 127.0.0.1:$PORT \
    --spectator-delay-seconds 2 --max-spectators 4 --connections-per-address 400 &
  SRV_PID=$!
  sleep 0.5
  ./target/release/arena-bot --server 127.0.0.1:$PORT \
    --matches 25 --team-size 3 --spectators $S --seconds 12 \
    --rtt 40 --jitter 10 --seed 42 --server-pid $SRV_PID
  kill $SRV_PID && wait $SRV_PID 2>/dev/null || true
done
```
