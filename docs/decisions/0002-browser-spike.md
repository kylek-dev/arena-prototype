# ADR 0002: First browser practice client

Date: 2026-10-31. **Status: accepted for the spike only.** ADR 0001's full production stack remains proposed.

## Scope and choices

Use Bevy **0.19.0** with its exact top-level dependency and committed Cargo.lock (which resolves compatible engine components to 0.19.1). This is a deliberate experiment on a documented release, not a claim that 0.19 is the newest engine. Pin wasm-bindgen crate and CLI to **0.2.122**, js-sys to **0.3.99**, Playwright to **1.61.1**, and Rust to **1.99.1**. Review upgrades deliberately rather than mixing tutorials across engine versions.

Use **WebGL2** for this simple arena. Unlike Wenilla's world renderer, these primitives do not require WebGPU storage-buffer features. This makes broad browser support a reasonable goal, but each browser/GPU still needs observed validation. No custom shaders, physics plugins, Lua, external assets, or engine editor are needed.

Keep the HUD in plain HTML/CSS. The browser handles input focus, readable text, accessible buttons, error presentation, and diagnostics. Bevy handles the canvas, camera, meshes, materials, and interpolation. A narrow JS/WASM adapter transfers input and structured state; gameplay rules do not depend on the browser. This avoids a UI-framework dependency for one practice screen.

## Rules and controls

The new `arena_sim::arena` module is separate from the original tiny 1D duel to preserve that runnable example. The browser uses these shared rules at 30 Hz: integer millimetres, a normalized 4.8 m/s movement cap, one movement command per actor per tick, bounded arena, two solid pillars, a 6 m targeted attack, 25 damage, a one-second cooldown, line-of-sight checks, and terminal round completion. Collision slides along axis-aligned geometry. Actor bodies do not yet collide with each other. No facing check, cast time, jumping, opposing AI, team rules, or competitive scheduling is implied.

WASD is camera-relative; right-drag orbits, scroll zooms, Tab selects the only opponent, left-click performs ray-based selection, Escape clears, 1 attacks, and R resets. Movement is interpolated for presentation. The opponent is stationary and does not fight back. Simulation runs locally; no multiplayer authority or networking is implemented.

## AI development feedback

A representative change is adding pillar collision and line-of-sight rejection in the pure rules module: `cargo test -p arena-sim` verifies collision, attack rejection behind a pillar, movement caps, cooldowns, and replay state. Client changes are checked by the compiler/Clippy and then through real Playwright keyboard and pointer events. `?debug=1` exposes read-only snapshots and timing diagnostics; tests do not inject health or teleport characters. This is a concrete code/test/browser feedback path, not a guarantee of universal AI capability.

## Evaluation

See [browser development](../BROWSER.md) and [spike results](../research/browser-spike.md) for commands, measurements and remaining gates. Retain the full stack's proposed status until real hardware/browser and later network evidence justify accepting it.

## Version-specific references

- [Bevy 0.19 manifest and features](https://github.com/bevyengine/bevy/blob/v0.19.0/Cargo.toml)
- [Bevy 0.19 orbit example](https://github.com/bevyengine/bevy/blob/v0.19.0/examples/camera/camera_orbit.rs)
- [Bevy 0.19 primitive scene](https://github.com/bevyengine/bevy/blob/v0.19.0/examples/3d/3d_scene.rs)
