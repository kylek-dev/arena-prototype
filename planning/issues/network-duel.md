After the browser spike and combat contract, add the first server adapter.

Acceptance criteria:
- Axum/Tokio server runs shared rules at fixed ticks.
- Versioned commands/snapshots, server-derived actor identity, bounded input queues and message/rate limits.
- Clients interpolate authoritative snapshots; measure before adding prediction.
- Test two clients at 50/100/200 ms RTT with jitter, reconnect/disconnect and malformed messages.
- Capture correction counts, queue backlog and observed input latency.

No database or ranked matchmaking required.
