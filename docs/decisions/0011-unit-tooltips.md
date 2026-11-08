# ADR 0011: Anchored unit tooltips

Date: 2026-11-07. **Status: accepted**, following the user's request for unit/world-frame hover and a layout-editable, bottom-right tooltip.

## Decision

One shared unit tooltip resolves frame tags or the engine's hovered world-unit tag against the latest roster. It shows name, authoritative health and explicitly labelled placeholder resources; no new combat mechanics or unit metadata are invented. Hover never submits selection, focus or attack intent. Reuse `TooltipCard` visuals, but not cursor-relative Radix placement: `Placed` owns a fixed viewport anchor with saved visibility, scale and position lock. Layout mode always shows an editable sample.

Add `frames.unitTooltip` to schema-1 validation with bottom-right defaults, preserving existing placements. Existing layout editor controls, profile import/export and resets apply unchanged. Live tooltips pass pointer events through to the game.

The optional shell `readHover` callback supplies unpressed canvas-local CSS coordinates separately from the positional command ABI. Rust publishes an optional `hoveredUnit` tag (`Player`, `Opponent1` or null). Picking uses presentation proxies and rendered transforms, chooses the nearest unit and respects conservative pillar blockers. Proxies deliberately do not depend on optional character mesh geometry. This is visual hover, not authoritative visibility, targeting or multiplayer identity. Older engine snapshots remain usable for frame hover.

Hide live tooltips during spell dragging, quick binding, capture, menus/settings and startup failure. Camera gestures, canvas exit, window blur, visibility changes and Escape clear pointer hover; frame exit/unmount clears its tag. Keyboard frame focus also exposes details. Layout mode substitutes its preview rather than a live tooltip.

## Verification

Rust picking tests cover nearest identity, misses, behind-camera units and pillar occlusion. DOM/domain/runtime tests cover shared frame/world details, live health, suppression, layout selection/visibility, profile migration and pointer cancellation. Local checks, WASM Clippy and the complete web build passed on 2026-11-07. Human browser review of world picking, static placement and layout dragging remains required and pending; no automatic rendered E2E gate is added.

## Human review update — 2026-11-08

The user verified and approved all implemented UI changes, including unit tooltips. Historical pending human-review statements above are superseded. See [review record](../UI-REVIEW.md).
