# ADR 0041: Map props, ground surfaces and new maps

Date: 2026-11-14. **Status: accepted for local integration**, at the user's
request to place the Quaternius props on maps, create new maps in the editor
and offer sand, snow and grass ground textures (CC0 from online sources).
Human browser review is pending.

## Decision

**Map file additions (still format 1).** Two optional top-level fields, so
every existing map stays valid:

- `"ground": "stone" | "sand" | "snow" | "grass"` (default `stone`) chooses
  the arena floor's texture. Presentation only; every ground plays the same.
- `"props": [{ "model", "x", "z", "y", "rotation", "scale" }]` (default `[]`)
  places models from `assets/props/catalog.json`. `rotation` is degrees,
  counter-clockwise seen from above; `y` and `scale` default to 0 and 1.

`arena-map` embeds the catalog and rejects unknown models, props outside the
building, heights outside −2–60 m, scales outside 0.1–10 and more than 1000
props. The editor writes both fields in a fixed key order.

**Props are decoration.** The rules never read them: units walk, see and cast
through props, and the camera does not collide with them. Making some props
solid would add rules geometry (movement, line of sight, server validation)
and is left for a later decision.

**Prop assets.** All 94 glTF props of the CC0 Fantasy Props MegaKit are
tracked as sources (about 43 MB). `scripts/prepare_props.py` (run by
`scripts/build-web.sh`) copies each `.gltf`/`.bin`, capping every material's
`metallicFactor` at 0.3, and shrinks the
13 shared trim-sheet textures with the pinned `character-textures` tool
(colour 1024 px, normal/ORM 512 px; about 11 MB prepared). Props keep their
relative texture URIs, so the browser decodes each trim sheet once however
many props a map uses. The cap exists because the metal trim sheet is almost
entirely fully metallic: without an environment map (the arena has none) the
first build rendered most props flat grey/white, as the user reported. At 0.3
metal keeps a sheen and shows its painted and vertex colours. Plain
`MI_Trim_Metal` (untinted iron) also gets a `baseColorFactor` of linear
(0.4, 0.42, 0.36): its sheet is a light lilac grey that the pack's renders
darken only through reflections, so the anvil otherwise read pale
purple-grey (reported by the user).

It also writes the catalog: id, label, category and rest bounds computed from accessor bounds through the node hierarchy. A
Python test checks the tracked catalog against the sources.

The client loads each model once (`props::PropModels`) and spawns props with
the venue, so editing a map rebuilds props with everything else.

**Ground textures.** Stylized Sand 002, Snow 005 and Stylized Grass 003 from
3DTextures.me (CC0, the same source as the venue textures), pinned by hash in
`scripts/fetch_textures.py` and prepared by `arena-textures` (which now reads
JPEG too). Unlike the venue's near-neutral maps they keep most of their
colour and are shown almost untinted.

**Editor.**

- *New map…* asks for a name, file id (derived from the name) and ground and
  starts an open 72 × 56 m floor with one room per team, the colosseum's
  venue, no pillars or props. It becomes live at once, starts a new undo
  history and is unsaved until *Save as <id>.json*.
- *Open map* lists `assets/maps` (new dev-server endpoint
  `GET /__map-editor/maps`; static builds read a generated
  `assets/maps/index.json`). Opening with unsaved changes asks first.
- The map opened or saved last on this device is the one the arena starts
  with next time (`localStorage`, falling back to the colosseum).
- Props: pick a model (grouped by category) and *Add prop*; drag in the plan,
  R / Shift+R turn by 15°, *Turn 90°*, Ctrl+D duplicates, Delete removes, and
  the inspector edits model, position, height, turn and scale. The plan draws
  each prop's scaled, turned footprint with a bar on its +Z side, plus a dot
  and a 9 px click target at its origin, because small props are only a pixel
  or two wide when the whole building is in view.
- The map settings gain a Ground choice; the plan's floor colour follows it.
- Inspector number fields apply on Enter, blur, spinner clicks and arrow-key
  steps. Clicking empty plan focuses the plan before clearing the selection,
  so a typed value is applied instead of lost (a reported bug in the first
  build).

## Consequences

- Maps can be dressed and new arenas made without code changes. A new map is
  playable immediately and after a reload once saved.
- Props cannot block or hide anything yet; players may expect a crate to be
  cover. This is stated in the editor.
- The WASM still embeds only the colosseum; other maps load from their files.

## Verification

Observed on 2026-11-14:

- `./scripts/check.sh` passes: `arena-map` 6 tests (grounds/props round trip
  and validation, catalog), `arena-client` 109 (1 ignored; including props
  spawning on a live edit with shared model handles, prop turn direction and
  ground floors), 38 Python tests (including the prop catalog), 199 Vitest
  tests (props, ground, new map, open and save-as in the editor UI; domain
  helpers) and the Node server tests (map listing).
- WASM Clippy with `-D warnings` and `scripts/build-web.sh` pass; the static
  check serves the ground textures, map index, prop catalog and a prop.
- A temporary copy of the new-map template with a prop passed the Rust map
  validator.

Human browser review is pending: prop appearance, scale and facing in the
arena, the ground textures' look and tiling, and the editor flows.
