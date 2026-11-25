# ADR 0040: Character customization from Quaternius kits

Date: 2026-11-14. **Status: accepted for local integration; not shipped since
2026-11-25**, when [ADR 0053](0053-finished-character-kits.md) made its finished
kits the only ones the game offers (the build code here stays). Built at the user's
request for "a character customization screen where I can view my character up
close and I can select different customization options available in the packs
… all the options available, gender, hair, eyes, etc". Human browser review is
pending.

## Problem

The player was one pre-baked GLB (ADR 0033/0034): the female base wearing the
Ranger outfit with bun hair. The installed packs contain much more: a male
base, eight rigged hairstyles/brows/beard, Ranger and Peasant outfits for both
bodies (with a hood), alternate outfit colours, light/dark underwear textures
and, locally, four QAL elf/orc bodies with their own hair, horns, eyes, skin
variations and a "Size" morph. Baking one GLB per combination is impossible.

## Decision

**Kits, built offline.** `scripts/character_kits.py` (standard library; reuses
the ADR 0033 retargeting/compaction) writes one GLB per body to
`web/public/assets/characters/kits/`: human female/male always, elf/orc
female/male only when the Git-ignored races pack is installed. Each kit holds
the skeleton, the same ten retargeted UAL1/strafe clips and *every* compatible
part as a separately named node (`Hair_*`, `Eyebrows_*`, `Hair_Beard`,
`Ranger_*`, `Peasant_*`, `Race_Hair`, `Race_Horns`). Materials are renamed to
slots (`Skin`, `Skin_Hands`, `Hair_1`, `Hair_2`, `Hair_Fantasy`, `Eyes`,
`Ranger`, `Peasant`). `character.json` gains a `kits` manifest of groups,
options, covered regions and texture variants; the model id is
`quaternius-kits`.

- **Body regions.** The human base body is split (index-only, shared vertex
  data) by each triangle's dominant bone into `Body_Head/Torso/Hips/Arms/
  Thighs/Calves/Shins/Feet`; calves split at 0.45 m between the lowest trouser
  hem (0.40 m) and highest boot top (0.56 m). Each outfit piece lists the
  regions it hides, so any mix of pieces shows skin exactly where nothing is
  worn. The hood also hides hairstyles.
- **Bind poses.** Parts must share joint names, order and hierarchy, but may
  differ slightly in bind pose: a part whose inverse bind matrices differ gets
  its own skin over the same joints. The male outfits/beard/brows share a bind
  pose that differs from the male base by up to 2.4 cm in the limbs, so the
  male kit adopts theirs for the skeleton and the body keeps its own skin.
  Hairstyles and brows are weighted only to `Head`, so every hairstyle works on
  every body (offset kept relative to the head bone).
- **Colours.** Human hair atlases are near-white and tinted at runtime (16
  presets). Skin is tinted (9 presets over the source tone). Eye colours are
  new textures: `arena-textures iris RRGGBB` recolours only the iris of the CC0
  brown eye (pupil, sclera, highlight preserved); elf/orc eyes are added when
  the races pack is installed. Outfit colour variants (Ranger 3, Peasant 2),
  light underwear and race skin/hair variations are separate bounded PNGs
  loaded only when chosen.
- **One scale.** All kits use the previous dressed-female scale, so bodies keep
  their authored relative heights (male taller); sole is the bare body's.

**Runtime.** The browser resolves the saved look against the manifest
(`web/src/domain/appearance.ts`) into a render spec: kit path, visible part
nodes, per-slot tint/texture, morph weight. `arena-client/src/appearance.rs`
only applies it: named node visibility, per-mesh material copies (the shared
glTF asset is never modified), `MorphWeights`. Changing body/ancestry loads the
other kit; the current one stays visible until the new one is ready and
styled, and a kit replaced before it finished loading is discarded. Spec paths
must stay under `characters/`; malformed specs are ignored whole.

**Start screen.** When kits are installed, the arena opens (after loading) on
a start screen that frames the saved character with two choices: **Enter the
arena** or **Customize character**. Customizing is optional. In the
customizer, **Save** returns to the start screen and **Enter the arena** starts
play directly; entering always begins a fresh round. Gameplay input waits
while the start screen is up.

**Customizer.** Also reachable during play from Menu → Customize character
(closed with Done or Escape). A right-hand panel (Body / Head / Outfit,
Randomize, Reset) hides the combat HUD; the camera frames the player from the
front, character left of the panel. Drag turns it; the wheel or the floating
Full body / Face buttons zoom. Walls still shorten the boom but never below
0.6 m. Movement, attacks and jumps are ignored while open. Choices save on
the device immediately (`arena.appearance.v1`). The UI shows only
player-facing names (no asset ids, file or licence details).

## Consequences

- Presentation only: no simulation, collision or rule changes; other units
  stay procedural.
- Human kits are about 13 MB each (previous single model 9.2 MB): they carry
  both outfits and every hairstyle. Only the chosen kit is downloaded; variant
  textures load on demand. Hidden parts are not pre-warmed, so the first
  showing of a piece may compile a pipeline.
- Git now tracks 39 more CC0 Quaternius source files (~76 MB) the kits read.
- Elf/orc kits come from the QAL races pack. They are built only from a local
  install and never committed; the GLB and manifest say QAL. Assess packaging
  before publishing a build that includes them. Outfits are fitted to human
  proportions, so race kits have no outfit slots.
- Supersedes the single `quaternius-female.glb` output of ADR 0033/0034, whose
  default look the default appearance reproduces (hair is now tinted auburn
  instead of the untinted grey atlas).
