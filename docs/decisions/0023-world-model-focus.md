# ADR 0023: Right-click world-model focus

Date: 2026-11-08. **Status: accepted**, following the user's request to right-click character models to focus them, like left-click targeting.

## Decision

An ordinary right-button canvas click focuses the picked character without changing selection or attacking. Both the player and current opponent can be focused. Misses leave focus unchanged; clear-focus bindings and reset retain their existing behavior. Four pixels of cumulative motion or either two-button chord suppress the click, preserving right-drag steering and two-button walking. Ordinary clicks do not request pointer lock; cancellation clears pending focus clicks alongside other input.

Append one-shot canvas-local CSS coordinates at input indices 15–16; indices 0–14 remain unchanged. Missing/negative/nonfinite coordinates mean no focus click, so older shells cannot accidentally focus at the top-left corner. After frame-selection intents, both left/right model clicks use the existing world-hover picker against rendered fighter transforms, including player/jump positions, nearest-unit ordering and pillar occlusion. This replaces left-click's older opponent-only sphere approximation, aligning both buttons with model hover. A left miss still clears target; a right miss does not clear focus. Nameplates deliberately remain clickable through pillars; attacks still use simulation-owned line of sight.

Rust returns picked local identities through the optional `onFocusUnit` shell callback, mapping only actor 0/1 to Player/Opponent1 tags. The browser reuses guarded `interactUnit(tag, "focus")`; focus remains browser-owned identity state, independent of selection. Do not publish client-authored combat results or add simulation focus mechanics. Reset suppresses a focus callback from the same input sample. Older engines ignore the appended fields; older shells can omit the callback. Rebuild frontend and WASM together to enable the feature.

## Verification

Node input regressions cover one-shot focus coordinates, unchanged prior ABI fields, draining/cancellation, drag threshold/reversal and both chord orders. DOM pointer tests cover coordinate routing, no pointer lock on ordinary clicks, no focus coordinates after drags and editing suppression. Runtime shell tests cover player/opponent identity routing, no selection/attack intent and gameplay/loading guards. Rust tests cover independent focus results, misses/reset, left target/miss behavior and coordinate validation; existing picker tests cover nearest units and pillar occlusion. Human rendered review of this new change is pending, separately from the user's earlier UI approval. No automatic browser E2E gate is added.

Local verification on 2026-11-08: `./scripts/check.sh` passed, including 62 client tests, 12 simulation tests, nine asset-adapter tests, 28 Node input tests and 141 Vitest tests. WASM Clippy with warnings denied and the complete web build/static resource checks also passed. The build used the procedural character fallback (BaseMale absent). No browser was launched.

## Human review update — 2026-11-08

The user explicitly approved the completed model right-click focus change. This closes the pending human UI review above, based on user confirmation rather than an agent-run browser session. Browser/device and individual console results were not supplied. See [review record](../UI-REVIEW.md#subsequent-changes).
