# Architecture

## Implemented today

`arena-headless` supplies a scripted list of commands to `arena-sim`. The simulation contains a deliberately tiny one-dimensional duel: bounded movement, range, damage, cooldown, and match completion. The script prints tick-tagged events. Rules run at a proposed 30 Hz and use integer state. Commands resolve in supplied order; ties are not simultaneous. The test replays the same input twice and compares events and state.

The browser spike adds `arena_sim::arena`: planar movement with integer coordinates, normalized movement limits, wall/pillar collision, range and line-of-sight checks, cooldowns and round completion. `arena-client` presents these rules in a Bevy 3D scene with character-relative movement and a plain HTML HUD. Browser input is sampled before fixed updates; positions interpolate for rendering. See [ADR 0002](decisions/0002-browser-spike.md).

Browser input intent is held in `web/input.js`, a deterministic state machine with no DOM, clock, or renderer dependency. `web/shell.js` binds DOM events and cursor capture to that state and forwards the existing positional input array to Rust. Node unit tests exercise transitions directly; Rust tests exercise combat rules. Rendering and browser integration are reviewed manually. See [ADR 0004](decisions/0004-unit-first-validation.md).

The optional glTF character is a presentation child of the existing player entity, normalized to the arena scale. A generated asset manifest lets clean clones use the procedural fallback without fetching a missing/private download. Loading state is exposed through the existing debug snapshot; collision and game rules do not depend on model geometry. See [asset provenance](../assets/characters/README.md).

Durable replay files, interrupts, teams, networking and server validation are not implemented. The public simulation API is provisional.

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

Headless and local browser adapters use the shared rules today; the server boundaries remain planned. The server will derive actor identity from an authenticated session. Never accept arbitrary actor IDs, damage, health or client-authored coordinates as authority. Add per-tick command budgets and length/rate limits at ingress. Decide fair command ordering explicitly before competitive play; a stable actor order alone could create bias.

Keep simulation time separate from rendering and transport. Browser prediction may reuse rules but does not decide outcomes. Snapshot interpolation and future reconciliation must expose correction metrics. Version replay content, rules and protocol so results remain explainable after balance changes.

A future development bridge should expose structured state and command injection for browser automation and screenshots. Keep debug capabilities out of public competitive sessions. Do not require editor interaction or an external AI service to run tests.
