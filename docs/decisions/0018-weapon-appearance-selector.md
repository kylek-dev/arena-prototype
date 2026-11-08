# ADR 0018: Device-local weapon appearance selector

Date: 2026-11-07. **Status: accepted for local prototype integration**, at the user's request. Human rendered review and binary publication/license confirmation remain pending.

## Decision

Add Settings → Weapons with Verdant Eclipse paired glaives and Aetherthorn crystal staff. Both support purple, yellow, red, blue, green and cyan energy palettes. Replace random startup colors with a validated, versioned device-local preference, independent of HUD profiles and graphics settings. Default to purple glaives when storage is absent or invalid; do not overwrite invalid/future storage simply by loading. Storage failures keep the session selection active with a warning.

A separate optional `readWeapons` callback supplies `[kind, color]` to presentation code, with strict numeric matching and safe defaults. Do not change the positional combat ABI, rules, collision, abilities or authority. Both weapons remain cosmetic back mounts under the player's existing motion carrier; hand gripping/attack animation is not implemented.

Lazily instantiate each requested weapon/color combination once per session (at most twelve groups), retaining cached groups behind parent visibility. Repeated switching allocates no additional models/materials after the combinations are cached. Each model has its own fallback and readiness observer; a late-ready model cannot override a hidden ancestor. Clone only energy/jade materials while retaining unrelated finishes and source assets. Inactive spill lights inherit hidden visibility. No bloom dependency is added.

Extend the build-time GLB adapter to retain only Aetherthorn's named Crystal Staff hierarchy, remapping node children and excluding the unrelated cube. Reject studio geometry/lights/cameras or missing essential staff parts within that hierarchy. Keep binary buffers/materials and source GLBs unchanged. Normalize the Y-up staff to about 2.45 m and mount it diagonally behind the player, above the floor. The bottom display plate is not part of the game asset.

## Verification

Follow-up correction: user screenshots exposed both weapon types visible together. The original group-only test did not verify descendant propagation through mounts missing visibility components. ADR 0019 fixes those mounts, adds actual inherited-visibility coverage and supersedes the initial no-bloom presentation decision below.

DOM tests exercise both weapons, all six colors, callback ordering, persistence, invalid/future storage and save failures. Rust tests cover numeric validation, tint isolation, fallback readiness and mount bounds; Python tests cover actual staff hierarchy extraction, source preservation and unexpected hierarchy rejection. Static HTTP checks include both generated GLBs.

Human review must check every weapon/color, rapid switches during loading, reload persistence, back clearance from front/side/rear, hidden old weapons/lights, movement/jump/reset attachment, optional character/fallback, and console/load errors. Automated checks do not establish rendered appearance or performance.

Local verification on 2026-11-07: `./scripts/check.sh` passed (12 simulation tests, 40 client tests, four Python asset tests, 26 Node input tests and 125 Vitest tests). WASM Clippy with warnings denied, the complete `./scripts/build-web.sh` including eight static HTTP resources, and `cargo run -p arena-headless` passed. The build used procedural characters (optional character asset absent). No browser was launched; human rendered review remains pending.
