# ADR 0012: Device-local graphics quality

Date: 2026-11-07. **Status: accepted for the prototype**, at the user's request for smoother arena and character edges. Human rendered review remains pending.

## Decision

Use the pinned Bevy 0.20.1 camera `Msaa` component, defaulting to `Sample4`, with an explicit Off option. Do not advertise unverified 2×/8× sample modes. High procedural detail uses 64×32 capsule subdivisions and a 48×32 head sphere; Low retains the original 32×16 capsule and 16×12 sphere. The meshes retain their dimensions and smooth primitive normals. Both levels are cached once, and switching replaces only procedural visual mesh handles, including the optional model's fallback. Imported glTF geometry, collision, simulation, and square pillar geometry are unchanged.

Settings → Graphics applies both options immediately. Versioned, validated device-local storage (`arena.graphics.v1`) is separate from exportable HUD profiles, since GPU preferences belong to the device. Missing, invalid, future or inaccessible storage falls back to 4× MSAA and High without writing on load. An explicit settings change saves the current supported format; storage failure leaves the setting applied with a visible warning.

A separate optional `arenaShell.readGraphics()` callback returns `[samples, highDetail]`; the positional combat input ABI is unchanged. Missing callbacks and malformed values use quality defaults. Rust samples settings before rendering, but updates camera/mesh components only when preferences change. No runtime dependency or custom shader is added.

Keep native resolution. Render scale/supersampling and imported-mesh reprocessing are deferred rather than introducing a new render-target/picking path in this edge-smoothing change. Anti-aliasing softens pixel stair-steps, not intentional geometric corners or low-poly imported surfaces.

## Verification

Domain/DOM tests cover settings reachability, bridge values, save/reload/reset, validation and storage failures. Rust tests check default/invalid bridge values and that High increases vertex counts without changing bounds. Run the standard native and WASM checks/build; human review must compare Off/4×, Low/High, camera movement, picking, reload, optional models and browser console on the intended GPU/browser. No rendered quality, compatibility or performance pass is inferred from compilation.

Local verification on 2026-11-07: `./scripts/check.sh` passed (including 25 Node tests, 120 Vitest tests and 10 client Rust tests); WASM Clippy with warnings denied and `./scripts/build-web.sh` passed. The build used the procedural fallback because the optional model was not installed. No browser was launched; rendered review remains pending.

Version-specific implementation references: the pinned Bevy 0.20.1 sources for `bevy_render::view::Msaa` and `bevy_mesh::primitives::{Capsule3dMeshBuilder, SphereMeshBuilder}` in the Cargo registry.
