# Architecture

## Implemented today

`arena-headless` supplies a scripted list of commands to `arena-sim`. The simulation contains a deliberately tiny one-dimensional duel: bounded movement, range, damage, cooldown, and match completion. The script prints tick-tagged events. Rules run at a proposed 30 Hz and use integer state. Commands resolve in supplied order; ties are not simultaneous. The test replays the same input twice and compares events and state.

This does not yet include 3D movement, targeting UI, serialization, durable replay files, collision, interrupts, teams, network security, or a browser build. The public simulation API is provisional.

## Intended boundaries

```mermaid
flowchart LR
    B[Browser input and renderer] -->|intent with sequence numbers| S[Server session and validation]
    S --> R[Shared rules crate]
    R -->|events and snapshots| S
    S -->|authoritative snapshots| B
    H[Headless scenarios and bots] --> R
    S -->|completed match record| D[Future persistence]
```

Only the headless-to-rules boundary exists today. The server will derive actor identity from an authenticated session. Never accept arbitrary actor IDs, damage, health or client-authored coordinates as authority. Add per-tick command budgets and length/rate limits at ingress. Decide fair command ordering explicitly before competitive play; a stable actor order alone could create bias.

Keep simulation time separate from rendering and transport. Browser prediction may reuse rules but does not decide outcomes. Snapshot interpolation and future reconciliation must expose correction metrics. Version replay content, rules and protocol so results remain explainable after balance changes.

A future development bridge should expose structured state and command injection for browser automation and screenshots. Keep debug capabilities out of public competitive sessions. Do not require editor interaction or an external AI service to run tests.
