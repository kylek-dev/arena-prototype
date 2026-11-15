# ADR 0043: Hexagonal pillars

Date: 2026-11-15. **Status: accepted for local integration**, at the user's
request to make pillars hexagons "so they are more rounded and easier to turn
corners on". Changes combat rules (movement and line of sight). Human browser
review is pending.

## Decision

**Shape.** A pillar is the hexagon inscribed in its existing width × depth
box: corners at ±half-width on its X axis, flat faces at ±half-depth. The map
format and editor fields are unchanged, so every existing map stays valid and
its pillars become hexagons in place (the colosseum's 9 × 9 m pillars give a
slightly tall hexagon; a regular one is 9 m wide and ≈7.8 m deep). Validation
still uses the box (on the floor, at least 1 m each way).

**Rules (`arena_sim::map::Pillar`, formerly `Block`).** Integer millimetres,
no floating point:

- `contains(point, padding)` is exact: inside the hexagon, or within
  `padding` of an edge (squared distances compared in `i128`, no division).
  Bodies keep `BODY_RADIUS`, so the blocked outline is the hexagon grown by a
  rounded margin.
- Line of sight is blocked by any of the six edges (touching counts), or by
  either end inside the hexagon.
- Movement: the step is clamped to the walkable regions as before; then any
  pillar it overlaps pushes the body straight out to just beyond its
  surface (along a face's normal, or radially around a corner). The body
  keeps the along-face part of its step, so it slides along slanted faces
  and rounds corners instead of stopping. Because pushing out onto a convex
  shape never lengthens a step, the result is trimmed to the requested step
  length to absorb millimetre rounding; the speed bound stays exact. If the
  body would still be wedged (a pillar against a wall or another pillar),
  the previous axis-by-axis veto applies.

Square pillars stopped a body dead at a corner it clipped and, with the
axis-by-axis veto, would also have snagged on any slanted face.

**Presentation.** A shared convex `Prism` (up to eight planes) replaces the
camera's axis-aligned obstacle boxes and the hover/nameplate picking boxes,
so the camera and pointer see the same hexagon as the rules. Pillar shafts,
plinths, bands and crowns are unit hexagonal prisms (`tiled_hexagon`) with
world-aligned UVs per face. The map editor draws pillars as hexagons.

## Verification

Rules tests cover corner rounding (clipping the corner keeps running past the
pillar), sliding along a slanted face (keeps over two thirds of the speed),
the exact speed bound, replay determinism, hexagon line of sight (a sight
line through the old square corner is now clear), and validation. Client
tests cover the prism against the rules' corners, ray entry, and outward-
facing hexagon meshes; a domain test covers the editor outline.
