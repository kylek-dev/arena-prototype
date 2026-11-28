# Asset provenance

## Window frame (2026-11-28)

`assets/ui/window-frame.webp`: the stone frame windows and dialogs wear, a
CSS 9-slice border (`web/src/styles/window-art.css` gives its slices).

- **Generated** by the user with Google Gemini on 2026-11-28 (model and plan
  not recorded) from the brief: a game UI window frame, polished black
  obsidian stone border with subtle antique gold inlay at the corners,
  empty centre, even front lighting, no text. Gemini output carries
  Google's invisible SynthID watermark. AI assistance does not establish
  exclusive rights. (A seamless obsidian surface texture from the same day
  was tried behind the windows and dropped at the user's request.)
- **Master** (1024² on opaque black, in the gitignored `assets/ui/source/`),
  SHA-256: `window-frame.png`
  `ea8f54551af6e9b6dc43dc476095b7e8bd7131141312247726e5319e330d5ea8`.
- **Prepared** by `scripts/prepare_ui_art.py`: the black ground and black
  centre made transparent, cropped, its corner slices measured to keep the
  gold corner ornaments whole, Lanczos downscale to 360 px, WebP quality 90
  with lossless alpha. Colours as generated (near-black stone, antique
  gold), not remapped to the palette.
- **Status:** in use for review; not yet accepted in the browser.

## Cast bar art (2026-11-28)

`assets/ui/cast-bar.webp`: the frame every cast bar wears (unit frames, the
separate player cast bar and nameplates), drawn as a CSS 9-slice border so
it stretches to any bar length; `web/src/features/unit-frames/cast-bar-art.css`
gives its slices.

- **Generated** by the user with Google Gemini on 2026-11-28 (model and plan
  not recorded), to match the unit frame art: an empty horizontal bar trough
  in black stone with an antique gold rim. Gemini output carries Google's
  invisible SynthID watermark. AI assistance does not establish exclusive
  rights.
- **Master** (1600 × 500 PNG on opaque black, in the gitignored
  `assets/ui/source/`), SHA-256:
  `cast-bar.png` `8af1a4f541ff6555293189857b4ab523833e91915191ad23882e5b787a752400`.
- **Prepared** by `scripts/prepare_ui_art.py`: the black ground made
  transparent (edges un-blended from black), cropped to the bar, the trough
  measured for the slices, Lanczos downscale to 800 px wide, WebP quality 90
  with lossless alpha. Colours as generated, not remapped to the palette.
- **Status:** in use for review; not yet accepted in the browser.

## Unit frame art (2026-11-28)

`assets/ui/unit-frame.webp` and `assets/ui/unit-frame-bare.webp`: the frame
every unit frame wears, with and without its round portrait socket (a
per-frame layout setting; player and party frames as painted, target, focus
and opponent frames mirrored in CSS), with
`web/src/features/unit-frames/unit-frame-art.css` giving where each one's
plaque, socket and bar troughs sit.

- **Generated** by the user with Google Gemini on 2026-11-28 (model and plan
  not recorded), over several rounds the same day; the ones in use are the
  last portrait frame and a matching frame without the portrait. The brief
  asked for an original horizontal frame: black textured stone with antique
  gold trim and small faceted blue gems, an empty name plaque, two empty bar
  troughs, no dragons or brown, even lighting so it can be mirrored. Earlier
  rounds (dragons around a portrait ring, painted bar fills, opaque
  background; a longer frame with a thin resource trough; an oval ring) were
  replaced. Gemini output carries Google's invisible SynthID watermark. AI
  assistance does not establish exclusive rights.
- **Masters** (1024 × 426 PNGs on opaque black, the same frame with and
  without the portrait, kept out of Git in the gitignored
  `assets/ui/source/`), SHA-256:
  `unit-frame.png` `4ca9611284b906f8227a5ca8147f24f9c7d8926952e7d14c5c86d70758c2add0`,
  `unit-frame-bare.png` `25fc6a407ad6b0eaf5754503015ad9c2c47ae6053c7a0a39542ab1a01b80cf8b`.
- **Prepared** by `scripts/prepare_ui_art.py`: the black ground made
  transparent (edges un-blended from black), each frame cropped to itself
  and its troughs, plaque and socket measured (the live bars are drawn on
  top, sized to the troughs), Lanczos downscale to 840 px wide with the
  portrait (the bare frame at the same density, 621 px), WebP quality 90 with
  lossless alpha. Colours are as generated (antique gold, near-black stone,
  blue gems), not remapped to the palette's swatches; the palette rule for
  painted textures applies before this ships.
- **Status:** in use for review; the user has not yet accepted it in the
  browser ([ADR 0052](../docs/decisions/0052-art-scope-and-quality-bar.md)).

## Spell icons: Pulse, Rift Bolt, Quell and Mend (2026-11-26)

`assets/icons/spells/{pulse,rift-bolt,quell,mend}-{128,256}.webp`: the action
icons of the four current spells, shown in action slots, the spellbook and the
drag preview.

- **Generated** by the user with Google Gemini, model Neon 4 on a Gemini Pro
  plan, on 2026-11-26, one icon per image, from the briefs: Pulse, an electric-blue sphere bursting outward with
  jagged lightning spikes; Rift Bolt, a violet arcane bolt tearing out of a
  rift in space; Quell, a glowing pale-cyan spell sigil shattered by a steel
  blade; Mend, cupped hands holding a swirling orb of green-gold healing
  light. The user states that the Pro plan licenses this output for
  commercial use (recorded 2026-11-27). Gemini output carries Google's
  invisible SynthID watermark. AI assistance does not establish exclusive
  rights.
- **Masters** (2048² JPEG, kept out of Git in the gitignored
  `assets/icons/source/`), SHA-256:
  `spell-pulse.jpg` `393355f52f45d406878155788fd8e8938cd214f91b06fa77474ef53e04e5d56e`,
  `spell-riftbolt.jpg` `34c90035104874293d947b261ce05c8c7ed08c7af965ec631af84a23713fd937`,
  `spell-quell.jpg` `344182bf1f160f2411a1c256d28d5ed1643651114c5b40ba32f16473b2add5d2`,
  `spell-mend.jpg` `c451f73ddd53007f4a2aaa973498de5aef392a77e80165248ef0462c1a9196e2`.
- **Prepared** by `scripts/prepare_icons.py`: square crop around the bright
  core (Mend: a hand-picked 1500 px crop centred on the orb), black ground
  lifted to palette `night-1`, Lanczos downscale in linear light to 128 px and
  256 px, WebP quality 90.
- **Status:** placeholders until the user accepts them in the browser
  ([ADR 0052](../docs/decisions/0052-art-scope-and-quality-bar.md)).
- Also in `assets/icons/source/` and unused: `icons1.jpg` and `icons2.jpg`,
  two 10 × 10 sheets of generic fantasy icons from the same model and plan
  (2026-11-26).

## Human male kit rebuilt from a Meshy model (2026-11-24)

`assets/characters/kits/meshy-human-male/` (`meshy-human-male.glb`, `kit.json`):
the finished human male of [ADR 0053](../docs/decisions/0053-finished-character-kits.md),
built at the user's request for a base body with separate customizable pieces.
The 7.4 MB file is tracked in Git at the user's request (its opaque textures
are embedded as JPEG since 2026-11-25; it was 13.9 MB as PNG). It is built by
[scripts/finished_kit](../scripts/finished_kit/README.md).

- **Shape and paint:** generated by the user with Meshy (Pro plan) on
  2026-11-23 from their own front, side and back concept images, exported as a
  detailed model and a retopologized, rigged model. Skin under the painted
  clothing is the model's own painted shading re-coloured with its skin tones;
  no other image generation was used. Meshy states paid-plan output is under a
  private commercial licence; confirm the terms before shipping.
- **Topology, UV layout, skin weights and face shapes:** the MakeHuman base
  mesh, `game_engine` rig weights and face targets as distributed with the MPFB
  Blender extension (2.0.18), which the MakeHuman and MPFB projects publish as
  CC0. Confirm those terms for the rig and weight files before release. No
  MakeHuman skin, hair or clothing is used, and none of MPFB's code ships.
- **Clips:** the ten clips of the previous Meshy kit, which were Quaternius
  UAL1 (CC0) and the repository's strafes (MIT) plus Meshy's "Running", hand
  edited by the user in Blender. Only the pelvis height was rescaled.
- **Swept-back hairstyle (`Hair_SweptBack`):** generated with Meshy (Pro
  plan) on 2026-11-24. The concept is a `nano-banana-2` image edit in Meshy
  (task `01a21076-4c0e-70e4-9cfd-b76e2b93895c`) of front, side and back renders
  of this kit's own bald head, made by `hair_refs.py`; the 3D bust is Meshy 7.1
  multi-image to 3D with the meshy-7 texture at 4K (task
  `01a21079-4936-771e-84fe-0854449eeb7b`), 36 credits in all. The offline
  pipeline (`hair_fit.py`, `hair_cut.py`, `hair_low.py`, `hair_tex.py`) cut
  the hair out of the bust, reduced it to 7,595 triangles with new UVs, moved
  the paint across and made it grey for tinting. Same licence terms as the
  body.
- **Close crop, tied back, full beard and handlebar beard (`Hair_CloseCrop`,
  `Hair_TiedBack`, `Beard_Full`, `Beard_Handlebar`):** generated the same way
  on 2026-11-24, from the same three renders: `nano-banana-2` concept edits
  (tasks `01a2109c-de1e-744f-a3ec-73d5a8e6cbe8`,
  `01a2109c-ea33-7586-b4c9-2f0cfa9218f1`,
  `01a2109c-f6c3-74e8-9704-3abf411d2c3a`,
  `01a2109d-0203-7223-8cc3-c93c70ae6aa1`) and Meshy 7.1 multi-image to 3D
  with the meshy-7 texture at 4K (tasks
  `01a210a5-4a72-74dc-9bd5-222e5d5e722d`,
  `01a210a5-5103-7559-9aa4-4a81959721f6`,
  `01a210a5-5b0b-72be-b9ce-34754a3440df`,
  `01a210a5-63e8-714a-b235-b34822e8b0fa`), 144 credits in all. The
  handlebar beard is the "goatee and moustache" concept as it came out, kept
  at the user's request. These were cut out by where Meshy painted hair
  (approved by the user). Same licence terms as the body.
- **Stubble (`Beard_Stubble`):** made by `stubble.py` from the full beard's
  footprint on the skin; no generation.
- **Iris and brow textures, eyes, trunks, sockets and regions:** made by the
  offline pipeline. The eye white and the brow shapes come from the Meshy paint.
- **Tools:** Blender 5.3 and Python (NumPy, SciPy, libigl, OpenCV). Anthropic
  Claude Fable 5.2 (Claude Code) wrote the pipeline, the build and client
  changes and this entry.

The user reviewed the bald kit in the browser and accepted it on 2026-11-24.
The user reviewed the swept-back hairstyle in the browser and approved it on
2026-11-24. The user accepted the other hairstyles, the beards and the
stubble on 2026-11-25 after two rounds of fixes; the last fix (the ponytail
clearing the back) awaits their browser check. AI assistance does not
establish exclusive rights.

## Gladiator greatsword (2026-11-25)

`assets/weapons/gladiator-greatsword/gladiator-greatsword.glb`: the two-handed
sword of the Gladiator slice, held in the finished kit's right-hand socket.

- **Generated** with Meshy (Pro plan) on 2026-11-25 from a concept by the user's
  brief: a `nano-banana-2` text-to-image concept (task
  `01a2114c-5466-7686-b57c-83a640130a48`), then Meshy 7.1 image to 3D with the
  meshy-7 texture at 4K (task `01a2114f-39fe-758f-b5c6-4a0f35adc52b`), 36
  credits. Same licence terms as the kit (Meshy paid-plan output; confirm the
  terms before shipping).
- **Finished** offline by `scripts/finished_kit/weapon.py` (scaled to 1.35 m,
  blade and grip thinned to game proportions, placed in the grip socket's
  frame), `hair_low.py --by-normal` (5,000 triangles, new UVs, normal map baked
  from the dense model), `hair_tex.py --colour` and `weapon_build.py` (JPEG
  textures, 0.67 MB).
- **Tools:** Blender 5.3 and Python. Anthropic Claude (Claude Code) wrote the
  pipeline and this entry. AI assistance does not establish exclusive rights.

The user reviewed it in the browser, held and sheathed, and accepted it on
2026-11-25.

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

`assets/lighting.json` is the user's exported tuning preset from the former temporary lighting panel, included under the repository MIT terms. It is the default lighting the arena loads (`web/src/domain/lighting.ts`); with the panel removed (2026-11-25), edit this file to change lighting.

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
