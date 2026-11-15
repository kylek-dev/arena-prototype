# Free texture sources

Checked 2026-11-14 (license page re-checked the same day when adding the venue sets). Licenses and download hosting change; re-check the linked license page before adding a new texture, and record it in [assets/textures/README.md](../../assets/textures/README.md).

Anything committed here is redistributed in a public MIT repository, so only sources whose terms allow redistribution of the raw files are usable. "Free to download" is not enough.

## Sources

| Source | Style | License (as checked) | Fit for this game | Downloads |
|---|---|---|---|---|
| [3DTextures.me — stylized category](https://3dtextures.me/category/stylized-textures/) | Hand-painted, chunky shapes | CC0: "All textures on this site are licensed as CC0"; credit optional; redistribution allowed; you may not claim authorship ([about](https://3dtextures.me/about/)) | **Best fit.** Matches [art direction](../ART-DIRECTION.md) rules 1–2 | Per-texture Google Drive folders linked from each post. 1K PNG basecolor/normal/roughness/AO/height; 4K is a Patreon perk |
| [ambientCG](https://ambientcg.com/) | Photoreal PBR (photogrammetry and procedural) | CC0 1.0; "you can include the raw files in your project, for example a video game"; credit optional ([license](https://docs.ambientcg.com/license/)) | Poor as-is (photoreal). Usable only heavily stylized, or for neutral masks/noise | Scriptable [API](https://ambientcg.com/api/v2/full_json?id=Ground037) and ZIPs at 1K–8K |
| [Poly Haven — textures](https://polyhaven.com/textures) | Photoreal PBR | CC0 | Same as ambientCG | Scriptable [API](https://api.polyhaven.com/assets?t=textures) |
| [stylizedtextures.com](https://stylizedtextures.com/) | Hand-painted (3DTextures.me's new home for recent sets) | **Unclear.** Links the CC0 deed, but the footer says "All rights reserved" | Do not use until the terms page explicitly states CC0 for the files | Site download button; bulk ZIP is a Patreon perk |
| [FreeStylized](https://freestylized.com/) | Hand-painted | **No license found** on the site (only disclaimer/privacy pages) | Do not use | — |

Photoreal sets fight rule 2 ("hand-painted textures") and the "avoid photorealism" list. If one is ever needed, desaturate and blur it heavily, downscale to 512 px and tint it in-engine, as the tool below does. Prefer an existing hand-painted CC0 set first.

## In use

| Runtime file | Source | Used on |
|---|---|---|
| `assets/textures/stone-floor.png` | [Stylized Stone Floor 010](https://3dtextures.me/2024/10/28/stylized-stone-floor-010/) basecolor + AO | Arena floor, 4 m repeat |
| `assets/textures/stone-bricks.png` | [Stylized Bricks 005](https://3dtextures.me/2024/08/21/stylized-bricks-005/) basecolor | Pillars, pit wall, parapet, pilasters, cornice and door frames, 2 m repeat |
| `assets/textures/stone-wall.png` | [Stylized Stone Wall 002](https://3dtextures.me/2024/06/18/stylized-stone-wall-002/) basecolor + AO | Outer colosseum wall (carved chevron frieze), 6 m repeat |
| `assets/textures/wood-planks.png` | [Stylized Wood Planks 002](https://3dtextures.me/2024/07/19/stylized-wood-planks-002/) basecolor + AO | Stand benches and concourse doors, 1.6 m repeat |
| `assets/textures/metal-grate.png` | [Stylized Metal Plates 001](https://3dtextures.me/2025/02/18/stylized-metal-plates-001/) basecolor + AO | Team portcullis gates, 2 m repeat |
| `assets/textures/metal-panels.png` | [Stylized Metal Pattern 001](https://3dtextures.me/2024/10/09/stylized-metal-pattern-001/) basecolor + AO | Gold trim, coping, lintels and keystones (tinted gold), 1 m repeat |

The stands reuse the floor flagstones with a warm sandstone tint. Banners, pennants and the sky are original art painted in code (`heraldry.rs` at startup; `arena-textures` offline for the sky) rather than downloaded: tiled fabric reads as noise on cloth, and photographic skies conflict with the art direction.

Character surfaces do not use downloaded textures. Tiled photo or fabric textures stretch on scaled ellipsoids and read as noise at combat distance. Instead, `crates/arena-client/src/textures.rs` paints original light gradients (warm tops to cool undersides), cloth hems, leather scuffs and hair strands in code.

## Workflow

1. Pick a set that passes the [art-direction quick checks](../ART-DIRECTION.md#quick-checks). Prefer large, calm shapes for ground; busy ground hurts readability.
2. Re-check the license page. Add each file's name, Google Drive file id and SHA-256 to `SOURCES` in `scripts/fetch_textures.py`, and a recipe to `TEXTURES` in `crates/arena-textures/src/main.rs`.
3. Run `python3 scripts/fetch_textures.py`. It downloads into the gitignored `assets/textures/source/`, verifies the hashes and runs `cargo run -p arena-textures`. That keeps a little of the original colour, bakes in ambient occlusion, adjusts contrast and brightness, and halves to 512 px with a seam-safe 2×2 box filter. The result is a near-neutral texture that the game tints.
4. Record provenance in `assets/textures/README.md`. The web build copies `assets/textures/*.png`; ordinary builds never touch the network.
5. In the client, give boxes world-anchored UVs with `textures::tiled_cube` and register the material with `textures::ArenaTextures`. That attaches the texture after load, adds mipmaps and leaves the flat tint in place if loading fails.

Bevy 0.20 does not build mipmaps for PNGs, so the client generates a gamma-correct chain on load. Normal, roughness and height maps are not shipped: the painted colour already carries the bevels, which keeps download size down and avoids the normal-map noise the art direction warns about.
