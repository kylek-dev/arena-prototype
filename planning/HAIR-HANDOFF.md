# Handoff: hair and facial hair for the finished human male

Written 2026-11-24 for the agent that continues this work. It assumes no
memory of the earlier sessions. Read
[ADR 0053](../docs/decisions/0053-finished-character-kits.md),
[ART-PRODUCTION.md](../docs/ART-PRODUCTION.md) and
[scripts/finished_kit/README.md](../scripts/finished_kit/README.md) first.

## Where things stand

- The finished human male kit (`meshy-human-male`) is built, wired into the
  game, and accepted by the user in the browser. It is bald and has no beard.
  In the customizer it is ancestry "Human (new)", body Male.
- The work is on the local branch `feat/finished-human-male-kit`. It is not
  pushed. Push or open a pull request only when the user asks
  ([AGENTS.md](../AGENTS.md)).
- Nothing has been generated for hair yet, and no Meshy credits are spent.

## The task

First pass, agreed with the user: **3 hairstyles, 2 beards and a stubble
layer** for this kit, chosen in the customizer and tinted by hair colour and
facial hair colour.

**Start with one pilot hairstyle**, show it to the user, and build the rest
only after they approve it. The user has not picked the styles. The last
proposal was a short swept-back cut for the pilot; ask once and carry on with
the preparation that does not depend on the answer.

## What the user expects

These come from the user's own feedback. Each was learned the hard way.

- **The quality bar is finished art**
  ([ADR 0052](../docs/decisions/0052-art-scope-and-quality-bar.md)). Hair is
  one of the things that sank an earlier attempt: "the hair, the faces, the
  proportions all were way off".
- **Hair comes from the generator, not from a script.** Script-built hair
  shells and clay-like caps were rejected twice. The user accepted sculpted,
  hand-painted locks "given it looks good". Meshy made the body the user
  likes, so it makes the hair too.
- **Real pieces and real material slots.** The user called detecting features
  in a baked texture "janky/hacky". Every customizable feature is its own mesh
  and material.
- **Show a small visual sample before building around it.** Concept image
  first, then the 3D piece on the head, then the game.
- **Money.** Confirm the model and the cost with the user before every paid
  Meshy call. The cap is 400 credits in total; the balance was 1,365 on
  2026-11-24. One attempt is about 36 credits: 6 for a `nano-banana-2` image
  edit and 30 for a Meshy 7.1 image-to-3D model with texture.
- **Meshy rules.** If a create call fails or times out, check
  `meshy_list_tasks` before calling it again: the task may exist and be
  charged. If the image editor refuses a prompt on content policy, do not
  reword around it. Never post assets to the Meshy Community page. Keep the
  user's name, email and project details out of prompts.

## What the kit expects of a hair piece

| | Hair | Beard | Stubble |
| --- | --- | --- | --- |
| Node | `Hair_<Style>` | `Beard_<Style>` | `Beard_Stubble` |
| Material | `Hair`, tint `hair` | `FacialHair`, tint `facialHair` | `FacialHair`, alpha-blended |
| Manifest group | `hair` | `beard` | `beard` |
| Triangles | 8k at most | 8k at most | a few hundred |
| Textures | 1024² base colour and 1024² normal, PNG | same | 1024² base colour with alpha |

- **Group ids must be `hair` and `beard`.** They are the feature groups the
  client knows (`featureGroups` in `web/src/domain/appearance.ts`). Each
  option is `{id, label, nodes}`. Add an option with id `none`; a look that
  names no option falls back to it.
- **Grey textures.** The game multiplies the texture by
  `swatch / hairAtlas`, with `hairAtlas = [0.78, 0.78, 0.76]`. Keep the
  painted light and dark of the locks, remove the hue, and bring the average
  to about that grey so each hair colour lands on its swatch. The brow
  textures average 0.70. Compute averages with 64-bit sums.
- **Skinning.** Weight every vertex to `Head`. There is no jaw joint.
- **Face presets.** The three face morph targets (Broad, Lean, Rugged) move
  the jaw and chin by up to 13.6 mm, the cheeks by 8.3 mm and the head above
  the brows by 4.5 mm. Beards, stubble and hair therefore carry the same three
  targets, copied from the nearest head vertex, and their nodes are added to
  `morphNodes` in the manifest. `build_kit.py` does this for the brows
  (`FACE_D`).
- **Helmets.** A chosen `head` option hides every node of the `hair` group.
  Nothing to build.
- **Materials.** Roughness 0.9, not metallic, like the rest of the kit.
- **No protocol change is expected.** Protocol 8 already carries `hairColor`,
  `facialHairColor` and up to 24 part groups. The customizer already shows
  facial hair colour when a `facialHair` material exists.

Everything goes through `scripts/finished_kit/build_kit.py`: add the
primitives, the materials and the manifest entries there, in the same way as
the brows and the trunks.

## A route that fits the existing tooling

1. **Reference renders** of the bald head from the built GLB: front, side and
   back on flat grey, following "Concept inputs" in ART-PRODUCTION.md.
   `check_glb.py` and `ref_renders.py` have the cameras.
2. **Concept.** Add the hairstyle to those renders with Meshy's image editor,
   or use images the user supplies. Show the user before generating 3D.
3. **Generate** with multi-image to 3D. Keep the raw download outside Git in
   `~/pitborn-art/human-male-hair/<style>/`, one dated file per attempt.
4. **Fit to the head.** Align the generated head to the kit head with a
   similarity fit on the face and skull (`fit_pose.py` does this for the
   body). Keep the geometry that lies outside the kit's scalp; the kit head is
   the reference surface. Close the underside onto the scalp with a small
   inward overlap, so no gap shows at the hairline or behind the ears.
5. **Make it a game piece.** Reduce to the budget, give it new UVs with wide
   gutters, move the colour across by closest surface point (`bake.py`), bake
   the normal map from the dense piece (`bake_normal.py`), and make the grey
   texture.
6. **Build, install, review** as in the tooling README. Add a `--hair` tint
   to `check_glb.py` beside `--skin`, and look at dark and light hair colours.
7. **Beards** the same way. **Stubble** is a painted layer made like the
   brows (`brows.py`): the jaw and cheek quads 0.7 mm above the skin with a
   grey alpha texture. It needs no generation.

This is a suggestion. If Meshy offers a cleaner way to get hair alone, such as
generating the piece on an invisible head, use it.

## Pitfalls already paid for

- Meshy packs texture islands edge to edge, so mipmaps bleed neighbours into
  specks. Repack with 12 px gutters.
- Inpainting or filtering near a UV island edge pulls in the empty texels
  outside it. Fill outside the islands from the nearest good texel first.
- Meshy exports flat per-face normals. Recompute smooth normals.
- libigl's `ray_mesh_intersect` silently drops hits for axis-aligned rays on
  dense meshes. Project onto triangles, or use closest-point queries.
- A per-channel RGB gain to reach a target average shifts the hue. Grade in
  Lab, or work on luminance.
- numpy `float32` means over millions of texels drift by several percent.
- Blender's glTF importer culls back faces. Thin, single-sided pieces must
  face outwards.
- The preview renders in Blender show shadow dithering that is not in the
  asset. Use `--no-shadow` in `check_glb.py` before chasing a blemish.
- The two raw Meshy body files carry different paint. The full-detail file
  has the cleaner skin.
- The Meshy MCP's retexture tool takes a model by URL or data URI only.

## Environment

- Python with the scientific packages: `~/dev/warrior/.venv/bin/python`.
- Headless Blender 5.3: `~/dev/blender-5.3/blender`.
- Art workspace: `~/pitborn-art/human-male-kit/` (`work`, `tex`, `out`,
  `qa`). The `.blend` files there predate the final kit.
- A live Blender on the Windows side is reachable through the Blender MCP
  and has the MPFB add-on. The build does not need it.
- The game: `./scripts/build-web.sh`, then `npm run serve` on port 8000.

## Done means

- The pilot, then every piece, is approved by the user in the browser. Record
  the actual status; do not claim a browser check that was not observed.
- `./scripts/check.sh` passes, and so do the WASM Clippy check
  (`cargo clippy -p arena-client --target wasm32-unknown-unknown --locked -- -D warnings`)
  and `./scripts/build-web.sh` if client code changed.
- Tests cover the new groups: `tests/unit/test_character_kits.py`,
  `tests/domain/kit-fixture.ts` (`finishedKit()`) and
  `tests/domain/appearance.test.ts`.
- [assets/README.md](../assets/README.md) has a provenance entry for each
  generated piece: tool, plan, date, concept source and licence.
- ADR 0053 no longer says hair and beards are missing, and the triangle
  counts in it are current.

## Not part of this task

Attaching weapons to the hand sockets, the female body, gear pieces, and
replacing the Quaternius human male with this kit. The user has not asked for
the last one.
