# ADR 0008: Gameplay-first focus and direct HUD editing

Date: 2026-11-07. **Status: accepted**, following the user's hands-on feedback and explicit request to revise the React UI. Supersedes conflicting UX details in ADR 0007 and the original implementation plan; Rust combat authority, static deployment and manual rendered-review policy are unchanged.

## Focus and navigation

Ordinary HUD interaction must not strand movement keys on a button. The runtime reclaims canvas focus after ordinary UI focus/clicks and routes gameplay keys even when a non-keyboard-owning HUD element briefly has focus. Plain HUD focus transitions preserve held keyboard movement. Text fields, selects, editable content, modal dialogs, binding capture, quick bind, layout editing and active spell drags explicitly own/suspend keyboard input. Entering those owners clears holds. Window blur, hiding and unexpected pointer-lock loss still cancel input; the runtime never forces focus onto an unfocused browser window.

Shift+Tab explicitly opts into normal keyboard UI navigation through a focus-only menu-access button. That navigation opt-out remains until clicking in the page or explicitly refocusing the canvas; focus recovery must not become a keyboard trap. The nonmodal icon-only spellbook does not suspend gameplay or dismiss itself when focus returns to the canvas.

Replace the visible top-right toolbar with a bindable `menu` action, default Escape. The menu exposes Settings, Spellbook, Layout, Quick binding, practice reset and opt-in diagnostics. Default clear target moves to Shift+Escape. Existing schema-1 profiles receive an additive binding migration: move the old default clear-target Escape, but preserve other custom Escape assignments rather than steal them. The menu remains reachable through Shift+Tab if unbound. Escape may be assigned while capturing the menu action; use Cancel to leave that capture. Other captures still cancel on Escape.

## Direct manipulation

Target/focus frames hide when their selection is absent. Layout editing always shows clearly labelled sample unit frames, independent of live target/focus and saved visibility. Left-click selects a frame/bar; its whole body is draggable, with arrow-key movement retained. No frame dropdown, X/Y sliders or frame drag bars. Position locks remain selectable and separately configurable. The movable editor window is dragged by its title and uses the same bounded viewport-placement hook as HUD frames; its position is session-only, while completed frame drags persist.

Spellbook is a set of action-slot-sized spell icons, with descriptions and real metadata in tooltips. Assignment is drag-and-drop only, as expressly requested: catalog copy, slot move/swap and Shift-copy. Dropping a valid slotted spell onto the canvas clears its source assignment, including a Shift-drag; it does not delete the slot or keybindings. Catalog-to-canvas, external, stale, invalid and cancelled drops are no-ops. Locked assignments cannot be discarded. No dropdown placement/removal alternatives remain. This deliberately reduces keyboard-only spell-assignment accessibility compared with the original plan; the interface must not claim otherwise.

New/default profiles start with assignments unlocked and no unlocked warning banner. Existing saved lock choices are preserved. Locking remains available under Settings → Action bars.

## Styling

Use **Radix Themes 3.3.0 (MIT)** with the existing React/Radix primitives, pinned in npm and its lockfile. The user requested a styling/component library; this explicitly replaces the earlier decision not to add a styled suite. Use the library's dark slate/indigo tokens, typography, buttons and separators, with neutral panels and restrained health colors. Custom canvas HUD controls use the same tokens. Dialog/tooltip portals mount within the themed overlay, never the Bevy canvas. The library's published stylesheet is included whole; build artifacts record its actual size rather than claiming a performance improvement. Ignore only dependency `use client` directive warnings in this deliberately client-only bundle; retain all other build warnings.

Sources consulted: [Radix Themes setup](https://www.radix-ui.com/themes/docs/overview/getting-started), installed 3.3.0 package exports/source and MIT license, and existing Radix Dialog/Tooltip contracts. No proprietary game assets or UI were imported.

## Validation

Add DOM/runtime regressions for focus recovery versus typing/navigation, Escape menu and rebinding, direct frame selection/drag, movable editor, sample/hidden frames, icon-only placement and canvas discard boundaries. No browser E2E is run automatically. User-reported issues motivated this revision; human review of the revised implementation remains pending.

## Human review update — 2026-11-08

The user verified and approved all implemented UI changes, including this decision's current revisions. Historical pending human-review statements above are superseded. See [review record](../UI-REVIEW.md).
