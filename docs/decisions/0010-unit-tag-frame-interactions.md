# ADR 0010: Unit-tag frame interactions

Date: 2026-11-07. **Status: accepted**, following the user's request for left-click targeting and right-click focusing on every unit frame, using unit tags.

## Decision

Resolve `Player`, `Target`, `Focus`, `Opponent1/2/3` and future `Team1/2/3` through a pure roster resolver. Tags are presentation references, not entity IDs or authority. Target/focus resolve the currently represented identity; focus can point to the player independently of target selection. Missing slots resolve to nothing. Today only Player and Opponent1 exist; this does not implement teams or additional opponents.

All live frames share one click adapter: left-click submits target intent, right-click sets focus without targeting or opening the context menu. Layout samples remain noninteractive so whole-frame selection/dragging takes precedence. Existing gameplay suspension guards apply to both actions.

The local practice bridge appends a one-shot self-selection flag at input index 13, preserving indices 0–12. Rust tracks selection as an optional local actor index and publishes `selectedPlayer` alongside the existing `selected` flag. The snapshot's `target` entity remains the practice opponent, not the selected identity. Older snapshots without `selectedPlayer` mean opponent selection; rebuild WASM and frontend together to enable self-targeting. Frame intents resolve opponent, player, clear, then canvas picking; reset runs first. Self-selection does not authorize Pulse against oneself: the shared simulation rejects that target, and the UI reports Invalid target.

## Verification

Domain/DOM tests cover tag resolution, unpopulated and synthetic team slots, all live frame interactions, self-target/focus display and editing guards. Input tests cover the appended field's draining/cancellation; Rust tests cover selection precedence and simulation rejection of self-damage. Human browser review remains required and pending; no automated rendered E2E gate is added.

## Human review update — 2026-11-08

The user verified and approved all implemented UI changes, including unit-frame interactions. Historical pending human-review statements above are superseded. See [review record](../UI-REVIEW.md).
