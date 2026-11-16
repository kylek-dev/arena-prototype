Follow-ups from the network and prediction work (ADR 0044, `docs/research/match-server-load.md`) that need measurements before acting.

Candidates:
- Real-network conditions: add packet loss and reordering to the bots' simulated link; over TCP a lost packet stalls everything behind it. Evaluate WebTransport only if measurements show it matters.
- Bandwidth: snapshot deltas or a lower snapshot rate (about 3 KB/s per player at 3v3 today).
- Server CPU grows faster than linearly with players: profile it; candidates are per-message writes and the cross-task wake-up per snapshot.
- Targeting fairness: the attacker sees other fighters about 100 ms in the past, but range and line of sight are judged on the server's present state. Measure how often "out of range" disagrees with what the player saw before considering lag compensation.
- Bots do not pace or measure the server's tick rate like the browser; give them the same behaviour so load runs match real clients.
- Keep the development diagnostics (`--dev` per-seat lines, overlay rows) useful as the protocol grows.
