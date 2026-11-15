# Art direction

**Core style:** stylized, hand-painted high fantasy with exaggerated proportions, vibrant colors and strong gameplay readability. Cartoon-inspired, never photorealistic.

**North star:** environments should feel magical, handcrafted, colorful and instantly readable from a distant third-person camera. When style and readability conflict, readability wins.

This guide applies to environments, props, characters, weapons and effects, whether code-generated, built in Blender or made with AI assistance. All art must be original; see [asset provenance](../assets/README.md) and [AGENTS.md](../AGENTS.md).

## Art rules

1. **Stylized geometry.** Chunky, exaggerated shapes with beveled edges and oversized architecture. Keep geometric complexity low: express form through big masses and clear profiles, not fine modeled detail.
2. **Hand-painted textures.** Painterly surfaces with visible color variation, soft gradients and illustrated material details. Paint the light, wear and edge highlights into the surface instead of relying on high-frequency texture or normal-map noise.
3. **Vibrant fantasy palette.** Saturated greens, warm earth tones, rich blues, purples and golds, all taken from the [closed palette](#palette). Avoid muddy, desaturated realism. Reserve the brightest palette steps for gameplay-relevant elements such as weapons and team markers; only spell effects may go beyond the palette.
4. **Readable silhouettes.** Pillars, platforms, walls and landmarks should be large and recognizable at a glance. A player should be able to orient themselves and identify cover from silhouette alone.
5. **Simplified materials.** Stone, wood, metal and grass look illustrated rather than physically accurate. Keep roughness high and reflections minimal; avoid mirror-like metals, specular sparkle and surface noise.
6. **Soft lighting.** Broad ambient illumination, gentle shadows and warm highlights. Avoid harsh contrast, deep black shadows and lighting that hides characters or the ground.
7. **Gameplay-first environments.** Clean ground surfaces, obvious collision boundaries, limited clutter and unobstructed visibility. Anything that looks solid must block; anything that does not block must not look solid.

## Palette

**Only colors in [palette.json](palette.json) may be used.** The palette is closed: environment tints, materials, characters, weapons, props, the sky, light colors, the HUD, menus and the map editor all draw from the same swatches, so nothing looks like it came from a different game. **Spell effects are the one exemption.** They may use any color or intensity because they are meant to stand out; nothing else may out-glow or out-saturate them.

- Swatches are grouped into ramps ordered dark (1) to light: `night` (UI neutrals and deep shadow), `stone`, `sand`, `wood`, `gold`, `ember`, `leaf`, `teal`, `sky`, `arcane`, plus on-screen `skin` and `hair` tones.
- Reference swatches by id, never by copying a hex value: Rust `palette::color(palette::STONE_3)`, TypeScript `palette["stone-3"]`, CSS `var(--pal-stone-3)`.
- Opacity, blends between swatches, lighting math (exposure, emission strength, linear lifts) and white as an untinted identity multiplier are not new colors and are allowed.
- Textures ship near-neutral and are tinted with swatches. Hand-authored textures and imported models must be painted from, or recolored to, the palette.
- To add or change a swatch, edit `docs/palette.json`, run `node scripts/palette.mjs` to regenerate the CSS, TypeScript and Rust bindings, and justify the change in the pull request. Prefer reusing a swatch.
- Tests reject color literals outside the palette in the client's Rust presentation code and the web UI. See [ADR 0042](decisions/0042-closed-color-palette.md).

## Textures

Use only hand-painted sources whose license allows redistribution; see [free texture sources](research/free-texture-sources.md). Ship near-neutral textures and tint them in code with palette swatches so one palette governs the scene. Paint light into character surfaces rather than tiling photo or fabric detail onto them.

## Quick checks

Before accepting an asset or scene change, view it from the default third-person combat distance and ask:

- Can I tell walkable ground, cover and walls apart instantly?
- Do characters, nameplates and spell effects stand out against the environment?
- Does it read as illustrated and handcrafted, not realistic or noisy?
- Is the palette vibrant without competing with gameplay colors?
- Does every color come from [palette.json](palette.json), with only spell effects outside it?
- Would it still read on the Low graphics setting and with MSAA off?

## Avoid

Photorealism, gritty or muddy palettes, thin or spindly architecture, busy ground textures, high-gloss reflections, decorative clutter in combat lanes, and any likeness to existing games' assets, UI or characters.
