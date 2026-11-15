# ADR 0035: Bounded character startup assets and unequipped player

Date: 2026-11-14. **Status: accepted for local integration** following the user's
entry-lag report, request for more visible jump start/land clips and temporary
weapon removal. The customization UI was discussed only; it is not authorized
or implemented.

## Decision

Reduce the new dressed character's source-resolution texture burden offline,
without changing source downloads or requiring new Python/image dependencies.
The existing pinned `image = 0.25.10` dependency in `arena-textures` supplies a
small `character-textures` stdin/stdout PNG adapter. Keep that crate outside the
client/simulation. Preserve its existing CLI's default with `default-run`.

Cap albedo/emissive images to a 1024px longest edge, normal/roughness/ORM data to
512px, preserving aspect ratio and never upscaling. Filter colors in linear
light with premultiplied alpha, data without gamma conversion, and renormalize
downsampled normal vectors. Cache duplicate input processing by bytes and
color-space role within a build.

Compact the GLB to mesh/material/texture/sampler/accessor/binary-view records
actually referenced by its character and selected animations. Share identical
image, texture and sampler records. Prune the excluded hood asset rather than
merely leaving it without a scene node. This retains the head's original vertex
attributes (including unused vertices); it is not character mesh decimation.

Extend the source-based takeoff phase from 0.12 to **0.30 s**, and complete
landing/recovery from 0.28 to **0.80 s**. Keep using UAL1 `Jump_Start`/`Jump_Land`,
not procedural replacements. There is still no pre-jump gameplay delay; world
flight/height and legal movement/casts/jumps remain simulation-owned.

Set `weapons::EQUIPPED = false` and omit the equipment carrier. The existing
selection system then cannot instantiate geometry, effects or weapon asset
requests, even from saved settings. Keep source weapons, saved preferences,
support code/tests and prepared static files for future re-enabling. Do not
implement a customization UI in this change.

## Evidence and verification

The user reported a large hitch on entry; no browser profile was captured.
The preceding dressed character measured **57,690,364 bytes**. The revised
prepared/production GLB measures **9,193,776 bytes (84.1% less)**. Its thirteen
unique textures have nominal RGBA8 texel storage of **24,641,536 bytes** before
mips, versus **336,068,608 bytes** for the unique source-resolution images.
These are artifact/dimension measurements, not GPU-memory or frame-time tests.

Synthetic tests cover reference compaction, removed data, duplicate
image/texture/sampler records, processing-cache roles and unchanged retargeting
contracts. Rust tests cover image dimensions/no-upscale, linear-light/alpha
filtering, normal normalization and unmodified data channels. Animation tests
require at least eight takeoff and 23 landing samples at 30 Hz. A no-carrier
regression checks all saved weapon selections allocate no geometry/materials
or cached groups. The ignored real-Bevy-loader test enforces a 12 MB payload
budget, bounded image dimensions and the existing skeleton/mesh/clip contracts.

Local `./scripts/check.sh`, WASM Clippy with warnings denied, the full web build
and its 16 static HTTP/MIME checks, explicitly invoked real-Bevy-loader test
and headless scenario passed. Prepared and production character GLBs match.
No browser was launched. **Whether the entry hitch is resolved remains pending
human browser review**, along with reduced-texture appearance and longer jump
phases. If a hitch remains, capture engine/browser frame timings and asset or
shader/GPU activity rather than assuming payload size was the sole cause.
