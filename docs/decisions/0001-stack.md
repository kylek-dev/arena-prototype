# ADR 0001: Browser arena technology direction

Date: 2026-10-31. **Status: proposed**, except the initial pure Rust simulation and Cargo layout now implemented. This recommendation remains open for discussion.

## Decision drivers

Original 3D tactical arena combat; browser delivery; high AI development usage; reproducible headless validation; small-team operations; short iteration cycles. No engine or language guarantees that AI can complete every task. Typed code, good diagnostics, narrow interfaces, and observable tests reduce friction.

## Recommendation

Evaluate **Rust + Bevy + WASM** first, with **Axum + Tokio** for an authoritative match server. Share a pure Rust rules crate between native tests, server, and eventually client prediction. Use ordinary HTML/CSS for surrounding account/lobby pages; decide whether the in-match HUD belongs in Bevy or the DOM during the client spike.

Wenilla pins Bevy 0.18.1, while Bevy's current quick start shows 0.19. Do not inherit a version merely because Wenilla uses it. Pick an engine version and compatible plugins during the browser spike, pin the lockfile, and use matching documentation. No Bevy dependency has been selected yet.

| Option | Why it fits | Main cost / question |
| --- | --- | --- |
| Rust + Bevy + WASM | Shared rules, strong compiler feedback, text-first ECS, native headless tooling | Compile time, changing engine APIs, WASM diagnostics and payload; browser tooling needs deliberate work |
| TypeScript + Babylon.js | Direct browser tooling, quick iteration, integrated 3D engine | Shared Rust rules need a WASM boundary, or rules must move to TypeScript; server authority still needs implementation |
| TypeScript + Three.js | Flexible renderer and web ecosystem | More game framework, UI, animation and tooling integration to build ourselves |
| Godot + GDScript/C# | Integrated scene/animation authoring | Browser export constraints and scene/editor workflows need an AI automation evaluation; do not assume native features carry over |

The preference for Bevy is an engineering judgment based on the user's interest in Wenilla and shared simulation, not a measured claim of superior AI coding performance. If the spike misses its gates, evaluate Babylon.js before investing in custom engine work.

## Proposed stack by layer

- Fixed-step simulation: dependency-light Rust; ticks and seeded randomness explicit. ECS stays an adapter until needed.
- Rendering: Bevy browser client; original glTF/GLB assets and ordinary textures. Primitive shapes until gameplay proves fun.
- Networking: same-origin secure WebSockets initially; versioned input/snapshot messages, sequence numbers, bounded queues, server validation, and interpolation. Add local prediction/reconciliation only with latency evidence. WebSocket is reliable/ordered and can suffer head-of-line delay; benchmark before considering WebTransport.
- Server: one authoritative match process initially, Axum/Tokio adapters; no Kubernetes or microservices requirement.
- Persistence: no database for the first guest matches. PostgreSQL/SQLx proposed once accounts and match history exist. Never write every simulation tick to SQL.
- UI: simple web lobby first; add a UI framework only when justified. Choose HUD placement from actual accessibility and integration tests.
- Hosting: static assets over HTTPS and one regional match server. Measure bandwidth, CPU, crash recovery, and cost before scaling.
- Authoring: text ability definitions with schema validation, source art and reproducible exports, explicit asset licenses. AI tools should be optional interchangeable contributors.

## Gates before accepting the full stack

Build a browser scene with a character, camera, movement and targeting. Record cold download bytes, time to interact, frame-time percentiles, incremental build time, and console errors on explicitly named machines and current Chrome/Firefox/Safari where available. Proposed targets: 60 fps on an agreed desktop baseline and interactive within 10 seconds on an agreed connection; these targets are negotiable until the baseline is chosen.

Run two clients against a real server with simulated 50/100/200 ms RTT and jitter. Measure input response, authoritative corrections, disconnect behavior, and command backlog. Run the same scenario headlessly. Document one representative AI-assisted rule change including compiler/test feedback. Record limitations honestly rather than implying all browser combinations passed.

## Sources

- [Wenilla source investigation](../research/wenilla.md)
- [Bevy setup and build tradeoffs](https://bevy.org/learn/quick-start/getting-started/setup/)
- [Babylon.js documentation](https://doc.babylonjs.com/)
- [Godot browser export documentation](https://docs.godotengine.org/en/stable/tutorials/export/exporting_for_web.html)
- [WebSocket browser API](https://developer.mozilla.org/en-US/docs/Web/API/WebSocket)
