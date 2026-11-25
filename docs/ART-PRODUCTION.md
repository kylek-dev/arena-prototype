# Art production

How finished art gets made, from concept to the game. The quality bar and scope
are in [ADR 0052](decisions/0052-art-scope-and-quality-bar.md); the look is in
[ART-DIRECTION.md](ART-DIRECTION.md).

## Decisions (2026-11-23, by the user)

- **Generator:** Meshy (Pro plan), chosen over Tripo after testing both.
- **Races:** Human, Elf, Orc, with original designs. Each has a male and a
  female body type.
- **Build:** a **modular kit**. Base bodies wear simple underclothes; gear is
  generated as separate pieces and fitted to every body, extending the kit
  system of [ADR 0040](decisions/0040-character-customization-kits.md). How a
  generated body becomes a finished kit is in
  [ADR 0053](decisions/0053-finished-character-kits.md).
- **First class:** Gladiator. Other classes come after the pipeline is proven.
- **Month 1:** one Gladiator vertical slice (below).

## Month 1: Gladiator vertical slice

| Deliverable | Count | Notes |
| --- | --- | --- |
| Human base bodies | 2 (male, female) | Underclothes, bald head, A-pose, split into the kit's body regions |
| Hairstyles | 2 (one per body type) | Sculpted hand-painted locks as separate meshes, weighted to `Head` |
| Gladiator base gear | 6 pieces | One per slot: head (helmet), shoulders (shoulder guard), torso (harness and chest), arms (arm guard and bracers), legs (greaves and skirt), feet (sandal boots) |
| Gladiator weapons | 2 | Short sword (main hand), round shield (off hand) |
| Gladiator spell icons | 8 | 2D, see the icon list below |
| In game | 1 | Both bodies wearing the set, playing all kit clips, chosen in the customizer |

Generation budget: about 12 assets × about 4 attempts ≈ 50 generations, well
inside Pro's monthly allowance of about 120 assets.

### Icon list (working names)

Only **Decapitate** exists in [the ability examples](../crates/arena-sim/Abilities_example.toml).
The other seven are icon briefs, not game design; the Gladiator kit's real
abilities are still to be designed.

Decapitate, Charge, Shield Bash, Whirlwind, Rallying Roar, Net Throw, Sunder
(a Negative Effect) and Last Stand (a Positive Effect).

## Technical spec

**Skeleton.** Every character piece uses the kit skeleton: 65 joints with
Unreal (UE4 Mannequin) names (`pelvis`, `spine_01`–`spine_03`, `neck_01`,
`Head`, `clavicle_l`, `upperarm_l`, …). The kit clips key rotations only, plus
pelvis translation, so bodies may differ in proportion. In Meshy, choose the
**Unreal / UE4** rig preset. Its weights are kept when the bone names map;
otherwise Blender bone-heat weights are used.

**Scale.** Match the kit heights within 2 cm: male about 1.81 m and female
about 1.77 m to the scalp, with hair on top. Soles sit at y = 0, facing +Z,
character's left on +X.

**Triangle budgets** (a fully dressed character stays at or under 70k):

| Asset | Max triangles |
| --- | --- |
| Base body (with head, without hair) | 25k |
| Hairstyle | 8k |
| Gear piece | 8k (helmet 6k) |
| Weapon / shield | 5k / 4k |

**Textures** (PNG only; the client decodes nothing else):

| Asset | Base color | Normal |
| --- | --- | --- |
| Body | 2048² | 1024² |
| Head (own material) | 1024² | 1024² |
| Each gear piece, weapon, hair | 1024² | 1024² |

- Light is painted into the base color. Materials are rough (0.9) and not
  metallic, following the art direction.
- Normal maps carry form only. Generated roughness and metallic maps are
  dropped.
- Every base color is graded to [palette.json](palette.json).

**Body regions.** Finished bodies are cut along edge loops into `Head`,
`Neck`, `Chest`, `Abdomen`, `Shoulders`, `UpperArms`, `Forearms`, `Hands`,
`Knees`, `Calves`, `Shins` and `Feet`, plus the `Trunks` piece over the hips
and thighs. Each gear piece lists the regions it covers, so hidden skin never
pokes through. The slot table and the attachment sockets are in
[ADR 0053](decisions/0053-finished-character-kits.md).

**Rest pose.** The body's own A-pose, bound to the kit skeleton. Generate gear
on or beside the body in that pose.

## Concept inputs

Each asset needs its own concept images. They can come from Meshy's image
generator (Pro includes it), ChatGPT or Gemini. Every image must have:

- One subject, filling about 85–90% of the frame, at 1024 px or more.
- A plain flat grey background, with no text, labels, floor or ground shadow.
- A straight-on, orthographic-style camera at chest height, with soft even
  lighting.
- **Front, side and back views** of the same design, for Meshy's multi-view
  input.
- For bodies: A-pose, symmetrical, bald, in fitted underclothes.
- For gear: the piece alone on an invisible mannequin, or worn on the base
  body in the same A-pose.
- The hand-painted stylized fantasy look and proportions of
  [ART-DIRECTION.md](ART-DIRECTION.md), and no likeness to existing games.

## Meshy settings

- Image to 3D with multiple images (front, side, back).
- Symmetry on for bodies and symmetric gear.
- Quad topology, with the target face count from the budget table.
- Texture from the image, kept stylized and hand-painted rather than
  photoreal.
- Rig: Unreal / UE4 preset, T-pose or A-pose.
- Export GLB (FBX only if GLB is missing), with the rig and skin, without
  animations.
- **Also export the dense High Detail model.** Meshy packs its texture
  islands almost edge to edge, so distant views (mipmaps) blend shirt and
  skin into specks along seams. The pipeline repacks the UVs with wide gaps,
  moves the colour across, and re-bakes the normal map from the dense model.
- Never post game assets to the Meshy Community page: posting makes them
  public under CC0 or CC BY-NC.
- Opt out of Meshy training on your inputs and outputs by emailing
  support@meshy.ai (Terms §2.9).

## Customization

A finished kit ([ADR 0053](decisions/0053-finished-character-kits.md)) has one
piece and one material slot per customizable feature. Nothing is detected in a
texture.

| Feature | How it is built | In game |
| --- | --- | --- |
| Skin tone | The body's paint is blank skin, graded onto the palette's `skin-natural`: "Natural" shows it as is and every other tone lands on its swatch | Tint of `Skin` and `Skin_Head` |
| Eye colour | Real eyelid openings with an eye white and a grey iris behind them | Tint of `Eye_Iris` |
| Eyebrows | A thin layer of the forehead's own quads with a grey, alpha-blended brow texture | Shape choice; hair colour tint |
| Hair | Separate meshes weighted to `Head`, grey textures | Style choice; hair colour tint |
| Facial hair | Separate meshes and a stubble layer, grey textures | Style choice; its own colour tint |
| Face | Morph targets on the head, followed by brows and beards | Preset choice |

**Concept tips that keep the rebuild clean:** clothes in a flat colour distinct
from skin; even skin without tattoos or markings; eyes open, looking straight
ahead, with clearly visible whites and irises; 4K texture. Meshy's image editor
refuses "remove the clothing" edits, so ask for the underclothes you want from
the start.

## Files

- **Raw generator output and concept images:** outside Git, in
  `~/pitborn-art/<asset>/` with one dated file per attempt. Back these up;
  Meshy may delete output from inactive accounts (Terms §2.5).
- **Finished, compact source assets:** in the repo under `assets/`, with
  provenance, the same way the Quaternius kit sources are tracked. (Proposal:
  confirm file sizes before the first commit.)
- **Built kit GLBs:** generated into `web/public/` by the build, never edited
  by hand.

## Pipeline

1. **Concept:** front, side and back images that meet the input rules.
2. **Generate in Meshy:** best of several attempts, retopologized, textured
   and rigged.
3. **Finish:**
   - fit to the kit skeleton
   - fit gear to every body and transfer the body's weights
   - set covered regions
   - grade to the palette
   - fix seams, hidden areas and anything else that falls short of the bar
4. **Build into the kit.** The offline tooling fits the scaffold mesh to the
   detailed model, transfers the paint, builds the eyes, brows, trunks, regions,
   sockets and face presets, and writes `<id>.glb` and `kit.json`. Copy both to
   `assets/characters/kits/<id>/`; `scripts/character_kits.py` ships every kit
   found there. The tooling is in
   [scripts/finished_kit](../scripts/finished_kit/README.md) (`run_all.sh`).
5. **QA:** posed renders at combat distance and close up, all clips, Low and
   High settings, then human review in the browser.
6. **Provenance:** an entry in [assets/README.md](../assets/README.md) with
   the tool, plan, date, concept source and licence.

## Definition of done (per asset)

- It meets the quality bar at combat distance and in the customizer close-up.
- It is within budget, uses the kit skeleton, and holds up in every clip
  (no stretching, poke-through or seams).
- Its colors are graded to the palette.
- Its provenance is recorded, it was made on a paid Meshy plan, and it was
  never posted to the Community page.
- A human has reviewed it in the browser; until then it is recorded as
  pending.
