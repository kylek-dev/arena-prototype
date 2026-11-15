# ADR 0019: Weapon visibility inheritance and HDR presentation

Date: 2026-11-07. **Status: accepted for local prototype implementation**, at the user's request after comparing Blender and in-game screenshots. Human rendered review of this revision is pending.

## Evidence and correction

The user observed both weapon types visible together and a flat, saturated crystal compared with Blender's glowing studio render. The selector's earlier automated checks asserted group visibility, not inherited visibility of imported descendants. Both weapon mount types lacked a `Visibility` component, breaking Bevy's propagation chain: an inheriting child whose immediate parent lacks visibility defaults to visible. Add inherited visibility to every mount and test the real visibility plugin across all twelve cached combinations and readiness updates. No simulation changes are involved.

Blender's compositor glow does not export to glTF. Source meshes are not decimated by the game. Preserve geometry/source GLBs and do not smooth away deliberately faceted crystal normals.

## Decision

Enable the pinned Bevy 0.20.1 `bevy_post_process` feature and thresholded additive bloom on the HDR camera. After screenshot-driven correction below, use a restrained 512px maximum bloom mip dimension, intensity 0.22, threshold 0.8 and soft knee 0.35. The pinned implementation contains a WebGL fallback using separate textures per mip. No custom shader, external texture, new version or runtime AI dependency is added. Actual GPU/browser compatibility and performance still require human review.

Use analytic Reinhard tone mapping (no LUT asset required), retaining HDR even with bloom off so highlights roll off rather than clipping. Explicit EV100 8.0 gathers about 3.25 times as much light as Bevy's implicit EV100 9.7 default. After further user feedback on contrast, use ambient brightness 500, a 6500-lux near-neutral warm key and a 3200-lux slightly cool fill, with no additional shadow maps. Lift the clear background to muted slate (sRGB 0.18, 0.21, 0.25) rather than near-black. This changes arena presentation but not picking, collision, simulation, camera controls or DOM HUD rendering.

At imported material readiness, identify the original staff crystal via its exported `GltfMaterialName`, clone its material, and retain a lit darker surface with emission strength 3.2, metallic 0.15 and roughness 0.24. Emission ignores exposure; every palette's peak exceeds the bloom threshold with headroom. Preserve the crystal's geometry/normals, source materials and selected palette. Thin energy inlays/filaments retain their source strength. Metal and grip materials remain unchanged. This is an artistic runtime adaptation, not an exact recreation of Blender's Cycles lighting or diffraction streaks.

Add two original code-generated, soft vertex-alpha additive discs at the crystal and shaft channels, plus twelve orbiting/pulsing sparks. Halos face the camera through the rotated staff mount and gently breathe; assets are allocated only on first selection of a combination. These local depth-tested, unlit effects work independently of bloom and lighting. They are siblings of the imported model/fallback, surviving model readiness but inheriting the selected group's visibility. No downloaded textures, custom shader, simulation time or combat randomness is involved. The bloom toggle controls post-processing only; local weapon effects remain on.

Expose **Weapon glow (bloom)** in Settings → Graphics, default on, saved independently of HUD profiles. Append a third graphics callback field without changing existing fields or combat input. Existing version-1 graphics preferences missing the optional bloom field acquire the on default in memory without a load-time write. Explicit invalid bloom fields are rejected. Off removes bloom but leaves HDR tone mapping and lighting active.

## Verification

### Screenshot-driven correction

The user reviewed the first HDR revision and reported a nearly black arena and no staff halo. That rendered review failed: compilation/tests did not establish visual quality. The original combination of reduced illumination and ACES tone mapping was not acceptable. The crystal's 0.55 emission also fell below the 1.0 bloom threshold, preventing its body from contributing; thin filaments alone were insufficient. The current decisions above supersede those initial values: restored lighting, explicit exposure, gentler Reinhard mapping, above-threshold crystal emission and independent local effects. The revised appearance still requires human review; no GPU/rendered pass is claimed.

Rust regressions exercise inherited visibility of simulated ready imported mesh descendants, bounded caches, crystal material lookup/source isolation, HDR requirements and bloom parameters. DOM tests cover glow toggling, persistence/reset, old preference compatibility and invalid input. Run full repository checks, WASM Clippy, the complete browser build/static checks, and the headless scenario.

Manual review: switch all weapons/colors rapidly during loading and confirm old meshes/lights disappear; compare cyan staff to the supplied Blender reference; inspect crystal facets, gold highlights, halo strength and motion/occlusion from front/side/rear. Check glow Off/On, MSAA Off/4×, reload/reset, optional character/fallback, arena/target readability and browser console on intended WebGL GPUs. No automatic browser or rendered pass is implied by unit tests or compilation.

Version-specific references: pinned Cargo registry sources `bevy_camera-0.20.1/src/visibility/mod.rs` (propagation and missing-parent fallback), `bevy_gltf-0.20.1/src/loader/mod.rs` (`GltfMaterialName`), `bevy_post_process-0.20.1/src/bloom/{mod,settings}.rs` (WebGL mip fallback, required HDR and threshold settings), and `bevy_core_pipeline-0.20.1/src/tonemapping/mod.rs` (analytic ACES fitted).

Local verification on 2026-11-07: `./scripts/check.sh` passed (12 simulation tests, 43 client tests, four Python asset tests, 26 Node input tests and 126 Vitest tests). WASM Clippy with warnings denied, the complete `./scripts/build-web.sh` including eight static HTTP resources, and `cargo run -p arena-headless` passed. The build used procedural characters (optional character asset absent). No browser was launched; rendered appearance, GPU compatibility and performance remain pending human review.

Correction verification on 2026-11-07: the same complete checks passed with 46 client tests, including exposure gain, every palette clearing the bloom threshold, soft halo geometry, camera-facing orientation, bounded spark motion and inherited visibility of halos/sparks/spill lights across all cached selections. The complete browser build/static checks and headless scenario passed. No browser was launched; the earlier failed user review remains recorded, and human review of this correction is still pending.

Further human feedback: the user confirmed the glow is visible in their screenshot but found the scene too contrasty. The current lighting values above reduce the key/fill ratio from 5:1 to roughly 2:1, increase ambient illumination and lift the background without changing exposure, tone mapping, weapon materials, bloom, halos or sparks. This records positive feedback on glow visibility only, not a full browser acceptance pass. Human rendered review of the softer lighting remains pending.

Softer-lighting verification: full repository checks (including 46 client tests and 126 Vitest tests), WASM Clippy, the complete browser build/static HTTP checks and the headless scenario passed locally. No browser was launched for this adjustment; rendered review remains pending.

## Human review update — 2026-11-08

The user verified and approved all implemented UI/rendered changes, including the current glow, visibility and softer-lighting corrections. Historical pending human-review statements above are superseded; earlier failed reviews remain historical facts. No measured performance or exhaustive GPU compatibility claim is added. See [review record](../UI-REVIEW.md).
