# ADR 0017: Imported Verdant Eclipse cosmetic glaives

Date: 2026-11-07. **Status: accepted for local prototype integration**, at the user's request. Binary asset publication/license confirmation and human browser review remain pending.

## Decision

Use the user's `assets/Verdant Eclipse.glb` for both existing crossed back mounts. Preserve procedural glaives while loading or on failure. Each imported world instance reveals itself and hides its own fallback only after spawning mesh descendants. Keep mounts under the existing presentation motion carrier, independent of character model/fallback visibility, collision, simulation and combat rules.

The export includes multiple scenes: scene zero is the original cube, and the named sword scene includes its studio backdrop. `scripts/prepare_weapon.py` creates a generated, single-scene GLB with only the flat weapon mesh nodes reachable. It excludes the backdrop/cameras/lights, preserves buffer/accessor/material indices and embedded binary data, and rejects unexpected hierarchy changes rather than guessing. The saved source remains untouched; unused mesh data remains in the generated file but is not spawned. Build failure is explicit if the required source or named scene is missing.

Rotate the exported XZ blade plane into the existing XY mount plane, align the source X grip axis with local Y, and scale the measured 9.3-unit blade span to 2.16 m before the existing 0.9 mount scale. Preserve the embedded green emissive materials and existing shadowless green spill light. Blender compositor glow is not part of glTF; no bloom or postprocessing dependency is added. Procedural electrical ribbons remain fallback-only rather than overlaying a different silhouette.

## Verification

Color-variant follow-up (2026-11-07), at the user's request: randomly choose a matching purple, yellow, red or blue pair once during presentation startup. Browser `Math.random()` is sampled outside the simulation; native presentation uses a system-time sample, while tests supply explicit samples. Equal-width sample buckets select the four variants. Reload rerolls; practice reset does not. Clone energy/jade materials per instance at readiness, reusing each clone across that instance's mesh descendants and preserving original shared GLB assets, energy strength and unrelated finishes. Fallback energy/halos/arcs and the shared spill light use the same palette. No per-frame recoloring, new dependency or source-GLB modification is needed. Human browser review remains pending.

Renderer-free Python tests exercise the actual export, scene filtering, preserved binary/material data, source bounds and malformed/unrelated-scene rejection. Rust tests cover normalization, mount clearance and per-instance fallback replacement only with mesh descendants. Static-build validation requests the generated GLB under a non-root hosting prefix.

Human browser review must verify both imported glaives, grip alignment, thin edge-on profile, green emission, rear/front/side silhouette, character clearance, movement/jump/reset attachment, both graphics detail settings, optional character behavior and browser console/load errors. Automated checks do not establish rendered appearance or browser compatibility.

Local verification on 2026-11-07: `./scripts/check.sh` passed (12 simulation tests, 35 client tests, two Python asset tests, 26 Node input tests and 122 Vitest tests), as did WASM Clippy with warnings denied, the complete `./scripts/build-web.sh`, generated-GLB HTTP/static checks and the scripted headless scenario. The build used procedural characters (optional character model absent). No browser was launched; human rendered review remains pending.

Palette follow-up verification on 2026-11-07: the same full checks passed with 38 client tests, including four-bucket selection, preserved energy strength/finishes, per-instance clone reuse and source-material isolation, matching pair variants and light color. WASM Clippy, the complete web build/static checks and the headless scenario passed. The source GLB was not edited. No browser was launched; all four palettes still require human rendered review.

## Human review update — 2026-11-08

The user verified and approved all implemented UI/rendered changes, including current weapon presentation. Historical pending human-review statements above are superseded. Binary publication/license confirmation remains separate and unresolved by this UI approval. See [review record](../UI-REVIEW.md).
