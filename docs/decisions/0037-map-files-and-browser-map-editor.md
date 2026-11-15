# ADR 0037: Map files and the in-browser map editor

Date: 2026-11-14. **Status: accepted for local integration**, at the user's
request to edit maps in a GUI instead of asking for code changes, and to stay
with Bevy rather than move to Unreal, Unity or Godot. Human browser review is
pending.

## Context

Room sizes, pillars and spawns were Rust constants (`arena.rs`), and the venue
was generated from those constants (`venue.rs`). Every layout change meant
editing Rust and rebuilding. The user also set new starting-room rules: no
hiding corners, no wider than the door, and a ceiling at least three
character heights tall.

Alternatives considered: Blender blockouts with an export script, TrenchBroom,
Tiled, and Bevy–Blender bridges. All of them edit outside the game, and the
bridges may lag Bevy 0.20. Switching engines would mean rewriting the client.
The layout is a flat plan of rectangles, so a top-down editor inside the game
covers it with the least new tooling.

## Decision

**One text file per map.** `assets/maps/colosseum.json` (format 1, metres)
holds the floor, pillars, starting rooms with their spawns and ceilings, and
the venue settings. It is the single source for both the rules and the
scene.

- `arena_sim::map::Layout` is the rules' integer-millimetre plan. Its
  `validate` reports gameplay problems in plain language: rooms too small,
  overlapping, or too close to a corner; missing or overlapping spawns; a
  pillar blocking an opening; sealed-off spawns; any point in a room hidden
  from its opening.
- The `arena-map` crate parses and serializes the file and adds presentation
  checks: room ceilings ≥ 5.7 m and at least 0.8 m below the pit wall, rooms
  not breaking through the outer wall, and pillars and outer wall taller than
  the camera can see. The WASM build embeds `colosseum.json`, so the game
  always has a valid map, and a test checks that every file in `assets/maps`
  is valid and round-trips.
- The client builds the venue from the active map and rebuilds it whenever
  the map changes.

**Live edits.** The browser shell exposes `readMap() → [revision, json]`. Rust
polls it every frame and answers each new revision through `onMapStatus`
with the full list of problems. A valid map becomes active at once. An
invalid one is reported, and the last valid map stays in play.
`Arena::relayout` swaps the layout mid-round when every fighter is still
walkable, keeping health, cooldowns, the tick and the camera; otherwise the
round restarts at the new spawns. When it starts, the shell fetches the map
file and sends it as revision 1, so a map saved since the last WASM build
applies without a rebuild.

**Editor (`web/src/features/map-editor`).** Open it from Escape → **Map
editor**. It is a movable, non-blocking window like the lighting panel, with
these parts:

- A top-down SVG plan (north up) showing the building, stands, pit wall,
  floor grid, rooms, pillars and spawns. Drag to move. Corner and back
  handles resize pillars and set room depth. Moves snap to 0.5 m, or 0.1 m
  with Alt.
- An inspector with numeric fields for the selection, or for the map name,
  floor and venue when nothing is selected. Edits commit on Enter or blur.
- Undo and redo (100 steps), add pillar, duplicate, add friendly or opposing
  room, add spawn, delete, and keyboard nudges.
- A status line with Rust's verdict and problem list, and unsaved-change
  tracking with a warning before the page unloads.

Drags preview only in the plan and commit once on release, so each gesture
rebuilds the arena once. Pure edit and history functions live in
`web/src/domain/map.ts`. Rust stays the only authority on whether a map is
valid.

**Saving.** `scripts/map-editor-server.mjs` is a Vite plugin for `npm run dev`
and `npm run serve` only. It serves `assets/maps/*.json` fresh from the
repository and accepts `PUT /__map-editor/maps/<id>.json`, writing
`assets/maps/<id>.json` atomically. It refuses requests unless all of these
hold:

- the Host is loopback (guards against DNS rebinding);
- the Origin, if sent, is the same origin;
- the content type is `application/json`, so another site would need a
  preflight that is never approved;
- the id uses only lowercase letters, digits and dashes;
- the body is at most 256 KiB and is a format-1 JSON object.

The editor enables Save only after Rust has accepted the current revision.
Saved files use the same layout as the tracked file (flat objects on one
line), so diffs stay small. Static production builds have no endpoint; they
offer **Download** and **Import…** instead. Once saved, the map is an ordinary
tracked file: commit it, and the next WASM build embeds it.

## Consequences

- Layout changes no longer need Rust edits. A saved map takes effect after a
  reload, and in the embedded copy after the next build.
- Bad maps cannot ship. The editor cannot save them, and Rust tests reject
  any invalid file in `assets/maps`.
- The editor handles one map id (`colosseum`). Choosing between maps, venue
  art placement (statues, arches) and height editing beyond the venue
  settings are not built.
- The server keeps the authoritative map once networking exists. Clients
  would then receive the map rather than send it, and the editor stays a
  local authoring tool.

## Verification

Observed on 2026-11-14:

- `arena-sim` passes 18 tests, including `relayout`, and `arena-map` passes 4,
  including the check of every tracked file. `arena-client` passes 93 of 94
  library tests; the map-edit test now covers both keeping and restarting the
  round. The one failure, a lighting colour-grading default, comes from
  separate in-progress lighting and startup changes that this work did not
  touch.
- Node: 31 tests pass, including the save endpoint (serving, saving, and
  refusing cross-origin, rebinding, wrong type, oversized, malformed and
  bad-id requests).
- Vitest: 179 tests pass, including map domain round trip, geometry, edits
  and history, and editor UI tests for loading, verdicts, drag
  preview/commit, undo/redo, inspector, keyboard, save gating, download
  fallback and import.
- Typecheck, lint and format pass. `scripts/build-web.sh` passes, and the
  static check serves `assets/maps/colosseum.json`.
- A smoke test against `vite preview` returned the status, served a fresh
  file, accepted a same-origin save of identical bytes, and refused a
  cross-origin save.
- Clippy with `-D warnings` (native and WASM) currently fails only on unused
  `ready()` methods in `sky.rs` and `textures.rs`, which belong to the
  unrelated, unwired `startup.rs` work.

Follow-up on 2026-11-14, before publication: the lighting test now checks the
camera against the shipped preset's grade rather than an ungraded default, and
the two `ready()` methods carry `#[expect(dead_code)]` until the startup gate
is wired in (`startup.rs` itself remains local and unpublished). With those
changes `./scripts/check.sh` passes (`arena-client` 94 of 94 run, 1 ignored),
as do WASM Clippy and `scripts/build-web.sh`.

Human browser review of the editor (dragging, live rebuilds, the round
continuing, saving and reloading) is pending.
