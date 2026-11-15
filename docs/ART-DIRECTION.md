# Art direction

**Core style:** stylized, hand-painted high fantasy with exaggerated proportions, vibrant colors and strong gameplay readability. Cartoon-inspired, never photorealistic.

**North star:** environments should feel magical, handcrafted, colorful and instantly readable from a distant third-person camera. When style and readability conflict, readability wins.

This guide applies to environments, props, characters, weapons and effects, whether code-generated, built in Blender or made with AI assistance. All art must be original; see [asset provenance](../assets/README.md) and [AGENTS.md](../AGENTS.md).

## Art rules

1. **Stylized geometry.** Chunky, exaggerated shapes with beveled edges and oversized architecture. Keep geometric complexity low: express form through big masses and clear profiles, not fine modeled detail.
2. **Hand-painted textures.** Painterly surfaces with visible color variation, soft gradients and illustrated material details. Paint the light, wear and edge highlights into the surface instead of relying on high-frequency texture or normal-map noise.
3. **Vibrant fantasy palette.** Saturated greens, warm earth tones, rich blues, purples and golds. Avoid muddy, desaturated realism. Reserve the most saturated and emissive colors for gameplay-relevant elements such as spells, weapons and team markers.
4. **Readable silhouettes.** Pillars, platforms, walls and landmarks should be large and recognizable at a glance. A player should be able to orient themselves and identify cover from silhouette alone.
5. **Simplified materials.** Stone, wood, metal and grass look illustrated rather than physically accurate. Keep roughness high and reflections minimal; avoid mirror-like metals, specular sparkle and surface noise.
6. **Soft lighting.** Broad ambient illumination, gentle shadows and warm highlights. Avoid harsh contrast, deep black shadows and lighting that hides characters or the ground.
7. **Gameplay-first environments.** Clean ground surfaces, obvious collision boundaries, limited clutter and unobstructed visibility. Anything that looks solid must block; anything that does not block must not look solid.

## Textures

Use only hand-painted sources whose license allows redistribution; see [free texture sources](research/free-texture-sources.md). Ship near-neutral textures and tint them in code so one palette governs the scene. Paint light into character surfaces rather than tiling photo or fabric detail onto them.

## Quick checks

Before accepting an asset or scene change, view it from the default third-person combat distance and ask:

- Can I tell walkable ground, cover and walls apart instantly?
- Do characters, nameplates and spell effects stand out against the environment?
- Does it read as illustrated and handcrafted, not realistic or noisy?
- Is the palette vibrant without competing with gameplay colors?
- Would it still read on the Low graphics setting and with MSAA off?

## Avoid

Photorealism, gritty or muddy palettes, thin or spindly architecture, busy ground textures, high-gloss reflections, decorative clutter in combat lanes, and any likeness to existing games' assets, UI or characters.
