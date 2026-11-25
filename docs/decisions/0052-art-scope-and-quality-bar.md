# ADR 0052: Art scope and quality bar

Date: 2026-11-23. **Status: scope and quality bar accepted** at the user's
request. The user then chose Meshy as the generator and set the first
production decisions; see the update at the end.

## Intent

In the user's words: make a great game that people love to look at and that is
perfect in every way. There is no team, no studio and no large budget, so the
plan is to invest small amounts of money in tools that let one developer
produce AAA-quality pieces. It does not need to be an AAA game in scope; every
piece that goes into the game should be of the highest quality.

## Decision: quality bar

- **Every shipped piece is finished.** Maps, props, character parts,
  equipment, spell icons and spell animations must hold up both at the default
  combat distance and in the character customizer close-up. Silhouettes are
  clean, textures are crisp and hand-painted, topology is finished and
  deformation is clean in every animation.
- **Same style, higher craft.** The stylized hand-painted direction in
  [ART-DIRECTION.md](../ART-DIRECTION.md) stays. Quality comes from craft, not
  from photorealism or noise.
- **Procedural is allowed when it meets the bar.** The procedural lightning
  spell effect is the reference example.
- **Placeholders are temporary.** Downloaded packs (Quaternius kits),
  procedural humanoids and AI output that has not been finished are labelled
  placeholders and must be replaced before release.

## Decision: art scope

| Area | Target |
| --- | --- |
| Arena maps | A pool of at least 5 maps |
| Props | Each map dressed with a large number of props, with combat lanes kept readable |
| Character customization | 2 genders, at least 3 races, face, hair and colour options |
| Classes | At least 5, each with equipment that clearly represents the class |
| Store equipment | Per class, sold visuals that look more epic than the base sets |
| Progression equipment | Per class, earned through progression |
| Spells | At least 8 per class (40+ in total), each with 1 icon and 1 animation; procedural animations are fine when they meet the bar |

Minimum totals: 5 maps, 5 classes, 40 spells, 40 spell icons and 40 spell
animations, plus 2 × 3 body and race combinations. The number of equipment sets
per class (base, progression tiers, store) is still open.

## Consequences

- [ART-DIRECTION.md](../ART-DIRECTION.md) no longer asks for low geometric
  complexity. Big readable masses come first, then finished secondary detail.
  Maps are richly dressed with props around the venue while lanes and sightlines
  stay clear.
- [GAME.md](../GAME.md) no longer defers production art. AI-assisted production
  art is allowed under human art direction, with recorded provenance and
  licensing that permits commercial use.
- [ADR 0035](0035-character-startup-budget.md)'s texture caps (1024 px colour,
  512 px data) were chosen for the placeholder Quaternius kit's startup time.
  They are not the budget for finished assets. Budgets per quality setting,
  with on-demand loading, need a later decision backed by measurements.
  [ADR 0030](0030-stylized-surface-textures.md)'s "no normal maps" applies to
  its procedural textures only; finished assets may use normal maps for form,
  never for noise.
- The kit system of [ADR 0040](0040-character-customization-kits.md) (shared
  skeleton, swappable parts, covered body regions) remains the integration
  target for bodies, races and equipment.

## Open questions (not decided here)

- Whether progression equipment changes stats or is cosmetic only.
  [GAME.md](../GAME.md) currently keeps "no gear advantage" for the first
  playtests, which matters for competitive fairness.
- The store's model, pricing, payments and legal requirements. Building the
  store waits until the core loop is proven.
- Class, race and spell names and identities.
- Triangle and texture budgets for the browser build and the planned native
  client.

## Production tooling (proposal)

Evidence from 2026-11-23: generating a character locally on an RTX 3070 Ti
(8 GB) with open models (TripoSG, Hunyuan3D-2mv) plus a custom Blender pipeline
took hours and still fell short of the bar. Meshy's free tier produced a better
result within minutes, in the user's judgement.

Recommended small investment:

- **Generation:** Meshy Pro, about $20 month to month or $12 a month billed
  yearly. Its 1,000 credits cover up to about 120 assets a month. Paid plans
  advertise a commercial-ready private licence; free-tier output is CC BY 4.0,
  which requires attribution. Verify the terms before shipping.
- **Finishing:** Blender (free), plus scripted steps for anything the generator
  does not do: fitting to the kit skeleton so the shared clips play, palette
  grading, installing into the kit, and posed QA renders. Those scripts
  currently live outside this repository (`~/dev/warrior`) and should move
  under `scripts/` once the approach is adopted.
- **Do not buy a GPU for asset generation now.** In November 2026 an RTX 5090
  costs about $6,200–6,900 new and a used RTX 4090 about $2,100–2,500, and open
  models still trail the commercial generators. Renting a 48 GB cloud GPU (about
  $0.30–0.60 an hour) is the fallback only if licensing requires self-hosted
  open models.
- **Contract a human artist for hero assets** where generated output still falls
  short (armour separated into rigid pieces, retopology, hand-paint cleanup,
  finger and face rigs).
- **Provenance:** record the tool, plan, date and licence for every asset in
  [assets/README.md](../../assets/README.md), as [AGENTS.md](../../AGENTS.md)
  requires.

## Update — 2026-11-23: production decisions

The user tested Meshy and Tripo, preferred Meshy, and is buying Meshy Pro.
Decided:

- Races: Human, Elf and Orc, with original designs.
- A modular kit: base bodies plus separately fitted gear pieces.
- Gladiator is the first class.
- Month 1 is a Gladiator vertical slice: human male and female bodies, the
  base gear set, a sword and shield, 8 spell icons, all in game. (Changed on
  2026-11-25 at the user's request: one two-handed sword; the game has no
  shields.)

The remaining tooling recommendations above stand: no GPU purchase, Blender for
finishing, and a contract artist for hero gaps when needed. The production
spec is [ART-PRODUCTION.md](../ART-PRODUCTION.md).
