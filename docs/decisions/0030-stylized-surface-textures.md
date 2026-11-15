# ADR 0030: Stylized textures on procedural geometry

Date: 2026-11-14. **Status: accepted for implementation**, at the user's request to give procedural models stylized textures from free sources without authoring GLBs in Blender. Human rendered review is pending.

## Decision

Keep all geometry procedural and add textures in two ways.

**Arena: prepared CC0 hand-painted textures.** Two 3DTextures.me stylized sets (CC0, redistribution allowed; see [free texture sources](../research/free-texture-sources.md)) cover the floor (Stylized Stone Floor 010) and the pillars, pillar bases and boundary walls (Stylized Bricks 005). A new offline workspace tool, `crates/arena-textures`, uses the already-locked `image` 0.25.10 with PNG only. It turns each source into a near-neutral 512² value map: it keeps a little of the original colour, bakes in AO, adjusts contrast and brightness, and halves the resolution with a seam-safe 2×2 box filter. `scripts/fetch_textures.py` pins each Drive file id and SHA-256 and regenerates the outputs. The prepared PNGs (~1 MB total) are tracked under `assets/textures/` with provenance; the web build copies them and ordinary builds never touch the network. The client is not a dependent of the tool.

Each material keeps its flat colour as a tint. The texture is attached only after loading, and the client then adds a gamma-correct mip chain, because Bevy 0.20 does not generate mips for PNGs and unmipped painted detail shimmers. Samplers repeat with trilinear filtering and request 8× anisotropy, which wgpu clamps where unsupported. A failed load leaves the original flat colours, never untextured invisible materials. Boxes get world-anchored planar UVs (`textures::tiled_cube`): texel density is constant, brick courses stay level, and neighbouring boxes continue one pattern. Transforms keep their scale because camera collision reads it. The flagstones replace the old floor grid seams as scale cues. Caps, team markers and the dark outer slab stay flat colours, keeping saturated colour for gameplay.

**Characters: original painted textures generated in code.** Downloaded fabric or skin textures stretch on scaled ellipsoids and read as noise, so `textures.rs` generates small RGBA images at startup with mips. Each paints light into the surface: warm tops fall to cool, hue-shifted undersides with soft wrap-around brush variation, plus a dashed-stitch hem for cloth, scuffs for leather, and strands with a sheen band radiating from the crown for hair. Sphere UVs (u around, v from the top) apply this per part. Base colours are lifted slightly in linear space so lit tops stay near the previous flat colours. The near-black legs/boots split into slate trousers and warm brown leather boots and belt, so the texture reads and silhouettes avoid deep black (art direction rule 6). Eyes and mouth stay untextured. The accent material shared with arena markers is cloned into a textured cloth material rather than texturing the markers.

No normal, roughness or height maps are shipped: the painted colour carries the bevels, download stays small, and it avoids normal-map noise. No simulation, collision, picking, input, weapon or lighting-preset changes; no runtime network or AI dependency.

## Verification

Tool tests cover neutralization and mean normalization, kept chroma, AO, saturated lift, contrast and seam-safe halving. Client tests cover world-anchored UV spans, level courses and continuity between adjacent boxes; mip count, size and gamma-correct averaging; and painted gradients (lighter, cooler-shifted undersides, seamless wrap, darker hem, mipped images). Existing humanoid tests still pass with the new materials.

Local verification on 2026-11-14: `./scripts/check.sh` passed (76 client, 13 simulation, 3 texture-tool, 12 Python, 28 Node input and 166 Vitest tests, plus workspace Clippy/format and frontend checks). WASM Clippy with warnings denied, the complete web build with its static HTTP check (10 resources, including both textures) and the scripted headless scenario passed. No browser was launched.

Human review must check, at default combat distance and close up on Low and High with MSAA on and off: floor readability and shimmer, pillar and wall brick scale and level courses, seams between boxes, how the flat-to-textured switch looks on load, contrast of characters, nameplates and spell effects against the new ground, the painted character shading, hems, hair strands and boots in motion, and console errors.
