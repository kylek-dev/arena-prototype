# ADR 0042: Closed color palette

Date: 2026-11-15. **Status: accepted for local integration**, at the user's
request for a palette file beside the art direction that only allows its
colors, applied to the existing game. Human browser review of the recolored
scene, sky and HUD is pending.

## Decision

**One closed palette.** [`docs/palette.json`](../palette.json) lists every
color the game may show: 90 swatches in dark-to-light ramps (`night`, `stone`,
`sand`, `wood`, `gold`, `ember`, `leaf`, `teal`, `sky`, `arcane`) plus on-screen
`skin` and `hair` tones. Its `rules` say how swatches may be used. Spell
effects (`crates/arena-client/src/lightning.rs`) are the only exemption, so
spells are the one thing that stands out.

**Generated bindings, one source.** `node scripts/palette.mjs` writes
`web/src/styles/palette.css` (`--pal-*` variables plus Radix Themes scale
overrides), `web/src/domain/palette.ts` and
`crates/arena-palette/src/swatches.rs`. `arena-palette` is a dependency-free
crate shared by `arena-client` (through `palette::color` / `palette::vec3`)
and the offline `arena-textures` sky painter.

**Radix follows the palette.** The HUD keeps `accentColor="indigo"` and
`grayColor="slate"`, but the slate, indigo, red, green and amber scales (and
the black/white overlay alphas) are redefined from swatches, so library
components, shadows and focus rings use palette colors.

**Characters.** Skin and hair swatches are the on-screen colors. Each texture
tint is derived as swatch ÷ approximate atlas color, so the existing tints are
reproduced to two decimals and the customizer shows the swatch itself.

**Applied to the existing game.** Existing colors were consolidated into the
ramps and every literal replaced: venue surface tints, team markers, backdrop
and light colors, procedural humanoids, weapon variants and metals, banner
heraldry, the sky's painted anchors (sky.png regenerated), and all HUD,
customizer and map-editor CSS. Most swatches keep the previous color or move
slightly; the visible changes are weapon glows (previously near-pure RGB, now
the brightest palette step of each hue), the HUD neutrals and accent
(Radix slate/indigo replaced by `night`/`sky`), friendly unit-frame health
(now the same green as nameplates) and the map editor's pillars and snow.

**Seamless stone.** A surface shows its texture's average colour times its
tint, so mismatched textures made matching tints disagree. The arena's stone
floor and brick textures are now greyscale with one shared linear average
(`STONE_MEAN`/`BRICKS_MEAN` in `arena-textures`), so a tint looks identical
on either. Floors and starting-room floors (a separate `RoomFloor` surface,
so the map's ground choice still affects only the arena floor) use
`stone-3`; pit walls and trim use `stone-4`, one gentle step bluer.

**Pillars read as cover.** In browser review a pillar vanished against the
pit wall behind it, first with a slightly lighter tint and then with a much
lighter one: the same fine brick pattern on both made them one surface at
distance. Pillars now use their own original texture, `stone-granite.png`:
soft, clustered granite grains (≈1.7 cm, 384 texels per metre) painted by
`arena-textures --painted` and mean-matched to the other stone. Shafts are
light `stone-6`, bands and crown the lightest `stone-7`, and a `stone-5`
plinth (retuned to `#7484a0`) steps between floor and shaft.

Two earlier tries were rejected in review: large painted ashlar blocks
(magnified, jagged and misaligned at the corners, then still "very blocky")
and corner posts that showed offset strips. Granite has no direction, so it
tiles in world space like the other surfaces.

## Enforcement

- `node scripts/palette.mjs --check` (in `scripts/check.sh`) and
  `tests/unit/palette.test.mjs` fail when bindings drift from the JSON.
- `tests/unit/palette.test.mjs` rejects hex, functional and named color
  literals in `web/src` and `web/index.html`, Radix scales or color props the
  palette does not drive, and default light colors that are not swatches.
- `arena-client`'s `palette` test rejects literal `Color`, `Srgba` and
  `LinearRgba` constructors and non-identity named colors outside test code
  and the exempt spell module. `arena-palette` checks its constants against
  the JSON.

## Not covered

Colors baked into imported assets (Quaternius character, outfit and prop
textures; the glaive GLBs' metal, leather and jade materials) and the
near-neutral shading multipliers inside procedural texture painters are not
rewritten. The rules require new and updated assets to follow the palette;
recoloring the existing imports is future work. Procedural `Vec3` colors are
not detected automatically, so reviews still check painters such as heraldry.
