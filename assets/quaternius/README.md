# Quaternius asset library

Reviewed 2026-11-14. Creator: **Quaternius**. Downloads supplied by the user;
Animation Library 1 was added during integration. Preserve each pack's included
license. These assets are **not covered by the repository's MIT license**.

| Folder | Source | Included license | Use |
| --- | --- | --- | --- |
| `base` | [Universal Base Characters](https://quaternius.com/packs/universalbasecharacters.html) | CC0 1.0 | Female/male superhero-proportion human bases, brows and hair |
| `animation v1` | [Universal Animation Library](https://quaternius.com/packs/universalanimationlibrary.html) | CC0 1.0 | Core animation source used by the player |
| `animation v2` | [Universal Animation Library 2](https://quaternius.com/packs/universalanimationlibrary2.html) | CC0 1.0 | Compatible supplemental clips; retained for future use |
| `outfits` | [Modular Character Outfits — Fantasy](https://quaternius.com/packs/modularcharacteroutfitsfantasy.html) | CC0 1.0 | Ranger armor used by the player; peasant and modular parts retained |
| `props` | [Fantasy Props MegaKit](https://quaternius.com/packs/fantasypropsmegakit.html) | CC0 1.0 | 94 glTF props retained; not yet integrated |
| `races` | [Characters — Fantasy Races](https://quaternius.com/packs/charactersfantasyraces.html) | **QAL v1.0**, not CC0 | Four elf/orc bases and texture variants, retained locally |

CC0 requires no attribution; we retain provenance voluntarily. QAL also requires
no attribution and permits personal/commercial game use and modification, but
prohibits **free or paid standalone asset redistribution**. It does not prohibit
shipping a completed game incorporating the assets. Raw `races/` is Git-ignored
so it is not republished as an asset download in this public source repository.
Keep the original local `races/License_Standard.txt`; [full QAL terms](https://quaternius.com/license.html).
Do not relabel those assets CC0 or MIT. Assess race packaging before publishing
a future game build that actually uses them.

## What Git tracks

The table describes the local library. Git tracks only the CC0 source files
`scripts/quaternius_character.py` reads to build the default player, plus this
README and the `base`, `animation v1` and `outfits` license files (26 files,
about 71 MB). These are the female base glTF/BIN with its seven textures,
`UAL1_Standard.glb`, `Hair_Buns.gltf`/BIN with its two textures, and
`Female_Ranger.gltf`/BIN with its six textures. Every other file, including
`animation v2`, `props`, peasant/male/modular outfits and other hairstyles, is
Git-ignored and remains local; add an explicit `.gitignore` exception when the
game starts using one. `races/` stays ignored for the QAL reason above.
Tools that scan the whole library, such as
`scripts/check_animation_compatibility.py` and `scripts/prune_quaternius.py`,
see only the tracked subset in a fresh clone.

## What was kept and removed

Keep `.gltf` **together with its `.bin` and referenced PNGs**, or `.glb` with any
external dependencies. GLB is not the only usable format. Unique alternate PNG
textures, previews, readmes and license files are retained. Referenced textures
stay beside their exports; duplicate unreferenced copies were removed by hash,
within each pack/license boundary. Some empty export directories may remain.

Removed redundant FBX/OBJ/MTL exports, the duplicate mannequin Blend source,
root-motion `_RM.glb` variants, unused Unreal-specific normal maps and
other-engine setup screenshots. These formats are not inherently unusable; they
are unnecessary duplicates for this glTF/Bevy pipeline. The simulation owns
locomotion and jump height, so the non-root-motion animation library is used.
Two base-character glTF files had three broken `*_png.png` image references;
they now point to the supplied equivalent `.png` files.

Cleanup removed **426 files / 421.3 MiB**. A recoverable archive of every removed
or repaired original is local at
`/tmp/opencode/quaternius-pruned-2026-11-14.tar.gz` (temporary storage, not a
permanent backup or repository artifact). The operation is reproducible with
`python3 scripts/prune_quaternius.py` (dry-run); applying requires `--apply` and
a fresh `--archive PATH`. A second dry-run reports no remaining changes.

## Customization kits

Since [ADR 0040](../../docs/decisions/0040-character-customization-kits.md)
the player is assembled from per-body kits built by
`scripts/character_kits.py`; Git additionally tracks the male base, all rigged
hairstyles/brows/beard, the Peasant and male outfits and the Ranger 3 /
Peasant 2 / light-underwear textures. The single dressed-female build below is
superseded; its default look is the kit's default appearance.

## Default female player

`scripts/quaternius_character.py` combines
`base/Base Characters/Godot - UE/Superhero_Female_FullBody.gltf` and
`animation v1/Unreal-Godot/UAL1_Standard.glb` offline, adding `Female_Ranger.gltf`
from the outfits pack, rigged `Hair_Buns.gltf` from base hairstyles and the
[separate authored universal strafe library](../animations/README.md). It embeds
the needed PNG images (shared atlas bytes deduplicated), keeps her skin/rest skeleton, verifies matching joint names and
hierarchy, and transfers local rotation deltas while preserving female bone
lengths. Pelvis pose offsets are scaled to her proportions; root motion,
animated scales/bone lengths and positive extra jump elevation are excluded.
The supplied jump clips' downward crouch/recovery offsets are retained.
No Blender, online retargeting service, or runtime AI is involved. Normal
preparation invokes the repository's pinned native `arena-textures` image tool:
color PNGs are bounded to 1024px and normal/data maps to 512px, never upscaled.
Colors are filtered in linear light with premultiplied alpha; normal vectors
are renormalized. Original downloads remain unchanged. Unused meshes,
materials, accessors and binary views are pruned; duplicate image, sampler and
texture records are merged so shared atlases are not repeatedly decoded.

The generated `web/public/assets/characters/quaternius-female.glb` contains:

| Runtime state | UAL1 Standard source |
| --- | --- |
| Idle | `Idle_Loop` |
| Forward | `Jog_Fwd_Loop` |
| Backward | `Jog_Fwd_Loop`, keys reversed |
| Left/right | Authored `Strafe_Left` / `Strafe_Right` from `assets/animations/universal-strafes.glb` |
| Jump | `Jump_Loop`, pose sought against authoritative flight |
| Jump start | `Jump_Start`, authored 0–0.35 s segment during the first 0.30 s of flight |
| Jump land | `Jump_Land`, full 0–38/30 s clip over 0.80 s after visual touchdown |
| Attack (reserved) | `Punch_Jab` |
| Successful Pulse | `Spell_Simple_Shoot` |

**Limitation:** the supplied Standard editions do not include authored backward
or lateral jogs. Left/right now use the separately authored lateral clips;
backward still reverses the forward cycle. Proper world-space foot locking
and production-quality retargeting are not claimed.
The female now has bun hair, source eyes and brows, and the Ranger torso/belts,
arms/bracers, shoulder armor, leggings and boots. The hood is excluded so hair
and face remain visible. Head/neck connected components are extracted from the
base mesh; covered body/limb triangles are removed from its draw indices so they
cannot clip through the outfit. Original unused vertex data remains in the
buffer; this is not mesh decimation. Armor and hair use the same skeleton/skin
after validating joint order, hierarchy, rest transforms and inverse bind
matrices. They animate with the body, not independent animation players.

The dressed model is normalized from boot-to-hair rest bounds
Y −0.0039672535…1.7791516 m to 1.9 m,
with glTF +Z facing game −Z. The existing procedural player remains the
loading/error/missing-source fallback; other practice units remain procedural.
Only the chosen player model ships, not the whole asset collection.

After the user reported a large entry hitch, character preparation reduced
the runtime GLB from **57,690,364 to 9,193,776 bytes (84.1%)**. Nominal unique
RGBA8 texture storage, before mips, falls from 336,068,608 to 24,641,536 bytes;
this is a dimension-based estimate, **not observed browser/GPU memory**.
The unused hood mesh is no longer loaded. These changes address a likely
startup bottleneck; no browser performance pass is claimed.

The player is **temporarily unequipped** (`weapons::EQUIPPED = false`).
No equipment carrier is spawned, so saved weapon selections cannot instantiate
or request either weapon. Weapon source files/settings/support code remain for
later use. Static build outputs still contain the prepared weapon files, but
the game does not request them.

All dark foot discs and the world target ring are removed. Selection continues
to appear in HUD frames/nameplates. These are cosmetic changes only.

Source SHA-256 after the image-URI repairs:

- Female glTF: `5ab0d4ae5ce17f4f4c4319655d82a4c882b559d6bb8c592b4dbb76d4f2ff1906`
- Female BIN: `3a8220a485b33d05d879115a50697728b45a151781106033afb8b8c243fca208`
- UAL1 non-RM GLB: `69591853d817488edaa8fd9bf8fc1d821eaeaf789f8627b3cd23b41c4ed67997`
- UAL2 non-RM GLB: `8cee20ab1bc55130092447e810e26df22dd2803eccc54f52137a7d54d7ab88a8`

Human browser review is pending. Check facing, feet, deformation, all movement
directions, jump/landing, Pulse, both back weapons, reset, loading/failure fallback
and console errors. Also check bun hair/face visibility, skin seams at neck/hands,
armor clipping and takeoff/landing recovery during movement and repeated jumps.
Build and headless loading checks do not establish appearance.
