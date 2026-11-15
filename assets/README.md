# Asset provenance

## Shared universal strafe clips (2026-11-14)

`assets/animations/universal-strafes.glb`: two new in-place lateral cycles,
authored in Blender 5.3.0 Release Candidate via Blender MCP with OpenAI GPT-6.1
Sol assistance. The user requested missing left/right strafes that are
universal/shareable across bases, outfits and races, using neutral names.
Source rig and single baseline idle pose: Quaternius Universal Animation
Library 1 Standard, **CC0 1.0**. New gait curves and tiny skin carrier are
**MIT** with a notice alongside the library and in the production build.
No existing asset's license is changed.

The library contains only the shared 65-joint reference skeleton, two cycles
and a three-vertex carrier—no human, outfit, race or texture data, and no
proprietary game animations or motion capture. The authoring script scales
analytic leg poses to reference limb lengths and bakes ordinary glTF keys.
Source mannequins/downloads are not modified. No QAL race data is redistributed.
See [reuse instructions, hash, provenance and validation](animations/README.md).
AI assistance does not establish exclusive rights. Human browser review is
pending despite numerical checks and Blender pose inspection.

## Quaternius character customization kits (2026-11-14)

At the user's request for a customization screen with "all the options
available" in the packs, `scripts/character_kits.py` builds one kit GLB per
body from the CC0 base characters, rigged hairstyles/brows/beard, Ranger and
Peasant outfits (both bodies) and their alternate colour textures, plus UAL1
clips; see [ADR 0040](../docs/decisions/0040-character-customization-kits.md).
Git now tracks the 39 additional CC0 source files these read (listed in
`.gitignore`). Eye colours other than brown are recoloured offline from the CC0
brown eye by the repository's `arena-textures iris` tool (our derivative of a
CC0 texture; no new art source). Elf/orc kits and the elf/orc eyes are built
only from a locally installed QAL races pack, are never committed and are
labelled QAL in the GLB and manifest. Anthropic Claude Opus 5.5 (Claude Code)
wrote the adapter, runtime and UI; geometry, textures and animation remain
Quaternius's work under their original licences. Human rendered review is
pending.

## Quaternius library and default female (2026-11-14)

The user supplied Quaternius base characters, animation libraries 1/2, fantasy
outfits, props and races and requested curation plus an animated female default.
Base/animation/outfit/prop notices are CC0 1.0. Races are **QAL v1.0**, retained
locally for permitted game use but Git-ignored to avoid standalone source-asset
redistribution. They are not MIT or CC0. No attribution is required by either
license; preserve creator/license provenance.

See the [curated inventory](quaternius/README.md) for source URLs, exact license
scope, cleanup backup, SHA-256 hashes, runtime modifications and animation
limitations. The Python adapter combines the CC0 female full-body base and
selected UAL1 clips offline without modifying source geometry. OpenAI GPT-6.1
Sol assisted the integration code, tests and documentation; the mesh, textures
and animation sources are by Quaternius, not AI-generated. Only the selected
female and embedded textures ship. Human rendered review is pending.

Follow-up at the user's request adds the included CC0 `Female_Ranger` outfit
(without its hood), rigged `Hair_Buns` and source UAL1 `Jump_Start`/`Jump_Land`
clips. Runtime indices exclude covered base-body triangles; head/neck, eyes
and brows remain. Outfit and hair are merged onto the same verified skeleton,
not re-generated art. Source files remain unchanged. The same OpenAI coding
assistance produced these offline/runtime adapters and tests; authorship and
CC0 terms of Quaternius geometry/textures/animation are unchanged.

Entry-hitch follow-up: with the same OpenAI assistance, the offline adapter
now downsamples runtime color PNGs to at most 1024px and normal/data PNGs to
512px using the existing pinned Rust image library, preserving original
downloads. It prunes unused glTF records/binary views and shares duplicate
texture/image/sampler records. The prepared model drops from 57.7 MB to 9.2 MB.
This is optimization of the same CC0 assets, not new generated art or a license
change. No browser performance improvement is claimed without review.

## Weapon publication authorization (2026-11-08)

The user confirmed that `assets/Aetherthorn.glb` and `assets/Verdant Eclipse.glb` are their original assets and explicitly authorized public redistribution under the repository's MIT license. This supersedes the historical pending publication/license notes for these two weapon files below, including their generated runtime derivatives. Preserve the recorded provenance; AI assistance does not establish exclusive rights. This authorization does not extend to the absent `assets/BaseMale.glb`, whose binary publication/license status remains unconfirmed.

`assets/lighting.json` is the user's exported tuning preset from the temporary lighting panel, included as editable reference data under the repository MIT terms. It is not automatically loaded as the default lighting configuration.

## BaseMale rigged character (2026-11-07)

`assets/BaseMale.glb`: original procedural Blender geometry, flat-color shader materials, 45-bone skeleton and eight authored keyframe clips, created via Blender MCP with OpenAI GPT-6.1 Sol assistance in Blender 5.3.0 Release Candidate. Prompt: “create a character model of a human male with animation rigging for left, right strafe, forwrad, back, jump, attack, cast” with a base outfit and armor later. No third-party model, reference imagery, texture, motion capture or paid generation service was used. The user copied the resulting GLB into this repository and requested local game integration; other subsequent human edits are not known. No source URL or third-party attribution applies. AI assistance does not establish exclusive rights. Binary asset publication/licensing remains unconfirmed; existing code licensing is unchanged.

`scripts/prepare_character.py` creates `web/public/assets/characters/base-male.glb` without overwriting the source. It isolates the character scene, removes authoring extras/local paths, orders all eight clips explicitly and excludes Jump's root translation to avoid double-applying simulation height. Binary mesh, joint/skin and remaining animation data are retained. The player uses the imported rig when ready and the procedural fallback otherwise. Idle and locomotion loop; Jump follows rule-owned flight; successful Pulse uses a cosmetic Cast release. Attack is retained for future presentation, not exposed as a new ability. The outfit is modular, but there is no complete hidden unclothed body, IK control rig, facial rig or armor/customization UI. No game-rule or runtime AI dependency was added. Human rendered/browser review is pending; see [ADR 0021](../docs/decisions/0021-base-male-skeletal-character.md).

## BaseFemale rigged character (2026-11-14)

`assets/BaseFemale.glb`: original procedural Blender geometry, matte vertex-colour shader materials, 45-bone skeleton and eight authored keyframe clips, created via Blender MCP with Anthropic Claude Opus 5.5 (Claude Code) assistance in Blender 5.3.0 Release Candidate. Prompt: the user's written specification for "an original stylized high-fantasy human female character for a third-person arena game" following [the art direction](../docs/ART-DIRECTION.md): about 7.5 heads tall, chunky sculpted high-ponytail hair with swept bangs, base tunic/leggings/boots/bracers as separate meshes, a rig mirroring the BaseMale hierarchy, and clips `Idle`, `Move_Forward`, `Move_Backward`, `Strafe_Left`, `Strafe_Right`, `Jump`, `Attack` and `Cast`. Body, head, hands and hair were built from code-generated sweeps and ellipsoids, voxel-remeshed and decimated; colours are painted per vertex. No third-party model, reference imagery, texture, motion capture or generation service was used, and no existing game's characters, armour or insignia were referenced. No source URL or third-party attribution applies. AI assistance does not establish exclusive rights. Binary asset publication/licensing remains unconfirmed; existing code licensing is unchanged.

Skeleton: `BaseFemale_Rig` with one non-deforming `root` at the floor and 44 deform bones (`pelvis`, `spine_01`–`spine_03`, `neck`, `head`, and per side `clavicle`, `upper_arm`, `forearm`, `hand`, `thumb_01`–`03`, `index`/`middle`/`ring`/`pinky` `_01`–`_02`, `thigh`, `shin`, `foot`, `toe`). The `assets/BaseMale.glb` source was not available locally, so these names follow the documented BaseMale bone set and count rather than a verified name-for-name copy. Body weights are Blender automatic weights with smoothing at shoulders, elbows, hips and knees; garments copy the body weights; face details and hair are rigid to `head`. Body faces fully hidden by the outfit were removed (no complete unclothed body, as with BaseMale). Locomotion clips keep `root` in place; `Jump` carries 0.45 m of root translation for the game to strip. `Idle` is a 2 s loop; the locomotion loops are 1 s, `Jump` 1.1 s, `Attack` 0.9 s and `Cast` 1.2 s. Source SHA-256: `ad7d4bf7f08a4c66ebcc1afacf77d2f13cb5e708871ebf0bfc6fac379f675d7b`. `scripts/blender/base_female/build.py` regenerates the file from code (Blender 5.3: `blender --background --factory-startup --python scripts/blender/base_female/build.py -- --clean`); on 2026-11-14 both a headless run and a run in the authoring session reproduced the hash above byte for byte. The builder scripts were written with the same Claude assistance and are covered by the repository MIT license. Game integration (2026-11-14, same Claude assistance): the player now prefers this model over BaseMale. `scripts/prepare_character.py` writes `web/public/assets/characters/base-female.glb` without overwriting the source, applying the same scene isolation, canonical clip order, extras removal and Jump root-translation stripping as for BaseMale. The client scales it to the shared 1.9 m height and uses its own Jump/Cast timing. At the user's request the same day, rigged players are disabled by default: the build ignores this source unless `ARENA_RIGGED_CHARACTER=1` is set, and the game uses the procedural character. Human rendered/browser review is pending; see [ADR 0029](../docs/decisions/0029-base-female-player-character.md).

## Aetherthorn crystal staff and selectable palettes (2026-11-07)

`assets/Aetherthorn.glb`: original procedural Blender geometry and shader materials authored via Blender MCP with OpenAI GPT-6.1 Sol assistance, exported by the user. Prompt: “in blender mcp build a glowing crystal staff”; follow-up: “remove the plate at the bottom so it can be used on a character model.” The design was named Aetherthorn in this conversation. Blender 5.3.0 Release Candidate. No external assets, reference imagery, textures or paid generation services were used. The user-exported file contains the staff hierarchy and an unrelated cube; the build adapter extracts only the staff hierarchy without modifying the source. The plate and rim were removed in Blender. Other human edits beyond export are not known. No third-party source URL or attribution applies; AI assistance does not establish exclusive rights. Binary asset publication/licensing is unconfirmed; this is authorized local prototype integration only, not authorization to redistribute or change license terms.

The weapon selector and six runtime palettes (purple, yellow, red, blue, green and cyan) for both staff and glaives were authored with the same coding assistance at the user's request. Energy materials are cloned/tinted; original GLBs and metal/grip finishes are preserved. No new external asset or runtime AI dependency. Original code remains under repository MIT terms. Human rendered review of the selector and staff is pending.

Screenshot-driven presentation follow-up: with the same coding assistance, added HDR bloom/tone mapping and warm/cool arena lights, reduced the cloned staff crystal body's emission to retain lit facets, and fixed weapon mount visibility inheritance. No additional assets, external references beyond the user's comparison screenshots, mesh decimation, source edits or license changes. Human rendered review is pending.

Further user review found that first HDR revision too dark and without a body halo. The correction restores brighter lighting with explicit exposure/Reinhard mapping, raises cloned crystal emission above the bloom threshold and adds original procedural vertex-alpha halo discs and mathematical orbiting sparks in `weapons.rs`. Same OpenAI coding assistance and original-code MIT terms; no external textures, assets, shader services, motion captures or source-GLB edits. Human rendered review of the correction is pending.

## Verdant Eclipse Blender model (2026-11-07)

Runtime palette follow-up: purple, yellow, red and blue variants are authored in `crates/arena-client/src/weapons.rs` with the same OpenAI coding assistance, without new external imagery or generated assets. A random matching pair is chosen per page load; cloned game materials tint energy and jade inlays and match the light/fallback effects. The Blender GLB remains unchanged, with the same provenance and pending binary publication/license confirmation below. Human browser review of the four palettes is pending.

`assets/Verdant Eclipse.glb`: original procedural Blender geometry and shader materials, authored through OpenAI GPT-6.1 Sol assistance and exported to GLB by the user. Prompt: “a glowing green sword that looks legendary and like a warglaive, wielded from the middle with a blade on each end.” No external reference image, downloaded model, texture, proprietary game asset, or paid generation service was used for this design. Blender 5.3.0 Release Candidate was used. Human edits beyond exporting/saving the GLB are not known. No source URL or third-party attribution requirements apply to the authored geometry; AI assistance does not establish exclusive rights. A separate license for this binary asset has not been confirmed; local prototype integration is requested, and publication/licensing must be confirmed before redistribution. Existing code licensing is unchanged.

The source GLB includes the original cube scene and the presentation backdrop. `scripts/prepare_weapon.py` generates `web/public/assets/weapons/verdant-eclipse.glb` with only weapon nodes reachable from its sole scene, retaining binary mesh data and the original emissive materials unchanged. The source is never overwritten. The client rotates the exported XZ blade plane to its back-mount plane and scales the 9.3-unit span to 2.16 m before the existing 0.9 mount scale. Two copies replace the procedural back glaives when ready; loading failures retain procedural visuals. Blender's compositor glow does not export in GLB; the game uses its emissive materials and existing green light spill, without adding bloom. Human browser review is pending.

## Arena textures (2026-11-14)

`assets/textures/stone-floor.png` and `stone-bricks.png` are prepared from CC0 3DTextures.me stylized sets; sources, hashes, license and modifications are recorded in [assets/textures/README.md](textures/README.md). The character textures generated in `crates/arena-client/src/textures.rs` are original code (Anthropic Claude Opus 5.5 assistance, Claude Code, 2026-11-14) under the repository MIT license. See [ADR 0030](../docs/decisions/0030-stylized-surface-textures.md).

The public repository includes original procedural visuals and support for an optional third-party stock character. The browser spike generates its meshes and flat-color materials in crates/arena-client/src/lib.rs; the HUD lightning mark is an original inline SVG path in web/index.html. These code-authored prototype visuals are covered by the repository MIT license. No external reference imagery or asset generation service was used to create those procedural visuals. Use original characters, silhouettes, iconography, sounds, and worldbuilding.

For every added asset record its path, creator/source URL, license, modifications, and attribution requirements. For AI-generated assets also record tool/model, date, prompt/reference provenance, and human edits when available. Do not claim exclusive rights from generation alone. Check provider terms before publishing generated assets. Never use protected game assets as source material.

Code and docs default to MIT; asset licensing must be explicit per contribution. Prefer CC0 or CC-BY-4.0 for external assets and keep notices alongside them. Decide a uniform license for original production assets before the art milestone.

## Crossed back blades (2026-11-07)

`crates/arena-client/src/weapons.rs`: original code-generated weapon geometry and materials, created with OpenAI GPT-6.1 Sol coding assistance. The first iteration was a pair of dark-metal, purple-edged swept swords; the user gave positive visual feedback. The current iteration is a pair of center-gripped, double-ended storm glaives, with opposing hooked blades, green emissive edges, additive halo ribbons, cached animated electrical arcs, and green light spill.

Prompt/reference provenance: the user requested an X on the player's back, then supplied two game screenshots to communicate center grips, blades at both ends, a crossed mounting arrangement and electric green glow. The subsequent refinement narrows and sharpens the blades and places both glaives flush in one intersecting back plane, per user feedback. Only those broad features informed the new original procedural silhouette; no screenshot pixels, textures, meshes, ornaments, insignia or other game asset data are included. No asset-generation service was used. Geometry, materials and effects are authored in Rust. Licensed under the repository MIT license; no additional attribution requirements for this original code. Human rendered review of the thinner, flush-mounted refinement is pending.

Latest glaive refinement (2026-11-07), at the user's request: widen the broad blade faces without thickening the edge-on profile or widening the green cutting-edge bands. The authored maximum face width increases from 0.21 m to 0.35 m; the thin depth, length, sharp tips and intersecting back mounts are preserved. Halo and electrical paths follow the new outline. Same original Rust geometry, coding-assistance provenance and MIT terms as above; no new external references or assets. Human rendered review is pending.

Crescent/angular follow-up (2026-11-07): the user requested a crescent cutout and additional sharp edges/turns rather than one continuous curve. Original authored profile corners now form an open inner recess and three angular outer teeth per blade end. Mesh faces and decorative traces follow the concave outline. Same coding assistance and MIT terms as above; no additional external imagery, geometry or generation service. Human rendered review is pending.

## Procedural humanoids (2026-11-07)

`crates/arena-client/src/humanoid.rs` and the rounded mesh cache in `graphics.rs`: original code-authored humanoid geometry, facial features, hairstyles and flat-color appearance presets, created with OpenAI GPT-6.1 Sol coding assistance. Prompt provenance: the user requested more human-like models with arms, legs, face, hair and eyes, leaving room for later configuration. No external reference imagery, downloaded character data, or asset-generation service was used for these humanoids. Repository MIT license; no additional attribution requirements. Human rendered review is pending.

Follow-up procedural movement animation in `crates/arena-client/src/animation.rs` and the humanoid joint hierarchy was authored with the same coding assistance at the user's request for walking, idle and jump motion. Original mathematical poses only; no external motion capture, animation clips, imagery or asset-generation service. Repository MIT license; human rendered review remains pending.

Rounded-model follow-up (2026-11-08): the user supplied a screenshot of this prototype and requested less boxy geometry, smooth joints/arms/legs/shoulders/hands, more expressive animation and no leg crossing during strafing. With OpenAI GPT-6.1 Sol coding assistance, the Rust builder now uses overlapping ellipsoids and thumbs, softer material finishes, chest/head articulation and an outward-only strafe gait. No external character data, motion capture, new binary assets or generation service; repository MIT terms are unchanged. Human rendered review of this refinement is pending.
