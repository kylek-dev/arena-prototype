# React combat UI implementation plan

## Handoff and authorization

The user approved React and requested implementation of the complete feature set below, with substantial organization, reusable abstractions, and professional architecture. Do not omit features to reduce file count or hide unfinished functionality behind decorative UI. Implement in focused, reviewable increments without repeated confirmations for ordinary reversible work.

This plan began as the fresh-context handoff. Read it, root AGENTS.md, README.md, docs/GAME.md, docs/ARCHITECTURE.md, docs/BROWSER.md, and relevant ADRs before implementation. Inspect Git status and preserve existing user changes. The only pre-existing working-tree change at implementation kickoff was this untracked plan, which has been preserved and updated. Implementation and automated checks are now complete; see [verification and handoff](REACT-UI-VERIFICATION.md). Human rendered review remains pending.

## Required outcome

**2026-11-07 follow-up:** the user explicitly revised the UX after hands-on feedback. [ADR 0008](../docs/decisions/0008-gameplay-first-ui.md) supersedes conflicting items below: gameplay-first focus, a bindable Escape menu instead of the top toolbar, direct whole-frame layout dragging with a movable editor, always-sample layout frames, hidden absent target/focus, icon-only drag-only spellbook, canvas discard, unlocked defaults without a banner, and Radix Themes styling. Earlier non-drag assignment requirements are intentionally removed, not unfinished work. See the current verification record and docs/FRONTEND.md.

- Original MMO-inspired HTML/CSS HUD over the existing Bevy canvas, organized with React and TypeScript.
- Configurable player, target, and focus frames sharing one unit-frame component.
- Layout editing: move, scale, show/hide, lock, and restore frames and action bars.
- Configurable action bars with assignable slots, visible keybindings, tooltips, and real ability availability/cooldown feedback.
- Spellbook with draggable spells that can be placed on action slots.
- Settings UI for remappable movement, tab targeting, and action-slot bindings.
- Quick binding mode: hover an action slot and press a key or supported modifier chord.
- Saved configuration that survives reload, safe defaults, and validated recovery from invalid stored data.
- Preserve gameplay, mouse controls, loading/error handling, diagnostics, and static deployment.

Use original visual design, icons, labels, and assets, not copied proprietary game UI or data. Do not claim multiplayer, additional combat abilities, or browser validation that does not exist.

## Scope and defaults

- Start with one 12-slot action bar; allow adding/removing bars with bounded, documented limits and changing slot count, rows, orientation, scale, and visibility. Use stable bar/slot IDs, never binding identities based only on array indices.
- Provide primary and secondary binding entries for bindable actions, including movement, next target, clear target, set/clear focus, target focus, action slots, and UI toggles. Retain practice reset with a configurable binding.
- Default movement remains character-relative WASD. Preserve existing left-look, right-steer, both-button walking, zoom, and ordinary click targeting.
- Only Pulse is currently a real combat spell. Show every supported real ability in the spellbook; do not pad it with fake usable abilities. Permit assigning Pulse to multiple slots, all displaying the same authoritative cooldown.
- Focus is independent selection state keyed by stable entity identity, not a stale copy of target health. Clear it when its entity disappears or the session resets. With one opponent, target and focus may refer to the same entity.
- Generalize next-target selection deterministically where needed without adding new opponents or changing combat balance. Keep current single-opponent behavior correct; do not advertise cycling multiple opponents as demonstrated.
- No account backend, mod system, macros, networking, new ability kit, or arbitrary user scripting is included.

## Architecture

### Ownership and dependencies

1. Rust simulation owns combat rules and validation; UI never authors health, damage, cooldown completion, or coordinates.
2. Bevy owns the canvas, scene, camera, and render interpolation. React mounts into a sibling overlay root and never remounts or reconciles the canvas.
3. A narrow browser runtime owns engine startup, input sampling, pointer lock, lifecycle, and snapshot delivery.
4. Framework-independent domain modules own bindings, assignments, layouts, focus selection, preferences, and deterministic transitions.
5. React components subscribe to read-only snapshots and configuration slices and dispatch explicit UI/domain commands.

No circular feature dependencies. Features share domain types and UI primitives rather than importing each other's component internals. Prefer composition over inheritance. Keep modules cohesive; do not create an enormous shell, store, or hooks file, and do not split trivial code merely to increase file count.

### Proposed source organization

Adapt filenames to repository conventions while preserving these boundaries:

```text
web/
  index.html                       # stable canvas + React overlay mount
  src/
    app/                           # bootstrap, providers, App, app error boundary
    bridge/                        # engine ABI, snapshot validation/adaptation, loader
    runtime/                       # DOM lifecycle, pointer capture, input routing, diagnostics
    domain/
      abilities/                   # real ability catalog and availability selectors
      bindings/                    # actions, chords, defaults, conflicts, resolution
      action-bars/                 # bar/slot model and assignment transactions
      layout/                      # anchors, bounds, scaling, layout transitions
      units/                       # entity identity, frame selectors, focus state
      preferences/                 # schema, migrations, validation, persistence adapter
    state/                         # typed stores, selectors, React subscription hooks
    features/
      unit-frames/                 # shared UnitFrame + role-specific composition
      action-bars/                 # ActionBar, ActionSlot, configuration controls
      spellbook/                   # catalog, details, spell placement
      keybindings/                 # editor, capture, conflict dialog, quick-bind overlay
      layout-editor/               # edit handles, selection, position/scale controls
      settings/                    # settings navigation and preference panels
      practice/                    # round state, feedback, reset
      diagnostics/                 # opt-in metrics panel
      loading/                     # startup, fallback and actionable errors
    ui/                            # accessible button, dialog, tooltip, tabs, field primitives
    styles/                        # tokens, reset, global overlay/layout styles
  input.js                         # preserve deterministic public input contract or compatibility export
tests/
  unit/                            # existing regressions + domain/runtime unit tests
  ui/                              # focused DOM component tests, not rendered browser E2E
```

Keep styles near their feature where useful, with shared design tokens centrally defined. Document public boundaries rather than relying on barrel exports that obscure dependency direction.

### Technology and build

- React, React DOM, TypeScript, Vite; exact versions and committed lockfile. Consult current version-specific documentation before using APIs.
- Client-only application: no Next.js, SSR, server components, router, or server backend required for this screen.
- Ordinary CSS with shared tokens and component/feature scoping; no unrelated dashboard theme.
- Use React external-store subscriptions with stable immutable snapshots and narrow selectors; select a small store library only if it clearly reduces complexity. Avoid one rapidly changing global context.
- Select accessible dialog/tooltip and drag-and-drop helpers based on actual requirements, compatible versions, licenses, and maintenance. Do not build a generic UI framework or install an entire component suite without need.
- Preserve deterministic input tests as a fast independent loop. Add TypeScript/domain and focused DOM tests with explicit tooling; update scripts and CI to run them, without adding browser E2E gates.
- Generated WASM and optional assets must work in Vite development and production static output, including subpath hosting. Define one documented asset-copy/serve strategy. Avoid deleting source files or treating Vite output as committed source.
- Separate frontend build from Rust rebuild for fast UI iterations; provide explicit complete-build and serve commands and update current scripts/instructions together.

## Behavior contracts

### Input arbitration and keybindings

- Explicit modes: gameplay, ordinary UI/typing, modal binding capture, quick binding, and layout editing. Define precedence in code and tests; capture modes consume events before gameplay.
- Bind normalized physical `KeyboardEvent.code` chords with modifier flags; render readable labels and explain keyboard-layout limitations. Preserve browser/OS shortcuts that cannot safely be captured; show a clear unsupported/reserved message rather than promising universal binding.
- Do not cast, target, reset, or move while entering bindings or typing. Entering an exclusive mode, losing focus, hiding the document, losing pointer lock, or changing bindings clears affected held actions so movement never sticks.
- One-shot actions ignore repeats; held movement survives unrelated key release. Track active actions correctly through modifier release and multiple physical keys bound to the same action.
- Deterministic binding conflict detection across movement, targeting, UI toggles, and action bars. Offer explicit replace or cancel; no silent reassignment. Remove stale bindings on bar removal with predictable feedback.
- Keybindings editor supports search/categories, two bindings, clear, restore defaults, capture cancellation, and conflict resolution.
- Quick binding has visible instructions and highlighted hovered/focused slot. Key capture is active only for an eligible slot; Escape exits without binding/casting; provide explicit unbind and keyboard-accessible slot selection. Persist accepted changes and show updated labels immediately.
- Preserve an accessible route out of the game canvas and dialogs; explicitly reconcile Tab targeting with normal UI Tab navigation and the existing Shift+Tab escape route.

### Spellbook and action bars

- Spellbook shows supported spell names, descriptions, range/damage/cooldown metadata consistent with real rules, and availability. Avoid independently maintained contradictory numbers; define ownership of presentation metadata at the bridge boundary.
- Spellbook-to-slot placement copies an ability assignment. Slot-to-slot moves/swaps atomically; define explicit copy and remove operations. Invalid drops and cancellation are no-ops. Dragging never casts.
- Lock/unlock assignments separately from layout editing; make both states obvious. Provide keyboard/non-drag placement and removal so dragging is not the sole interaction.
- Tooltips explain bindings, targeting, range, cooldown, and why an action is unavailable. Click and key activation resolve the same slot-to-ability mapping and submit the same validated intent.
- Use authoritative cooldown values; visual animation may interpolate presentation without becoming combat authority. Empty slots, invalid saved ability IDs, no target, out of range, occlusion, won rounds, and loading all have explicit behavior.
- Do not assume one boolean attack flag scales to multiple abilities. Keep today's single-ability adapter honest and document extension points. Any bridge change must update Rust/JS contract tests together.

### Frames and layout

- Shared UnitFrame supports name, health/current maximum, selection/focus labels, empty/dead states, and role-specific settings. Do not display fake resources, portraits, buffs, or casts as real data.
- Target frame reflects actual selection rather than presenting the opponent as selected at all times. Focus can be set from the selected target, cleared, and selected back as target through UI and bindings.
- Layout mode provides preview placeholders for absent target/focus, drag handles, visible bounds, scale/visibility controls, reset selected/all, and lock/done controls.
- Positions use documented viewport anchoring/normalized offsets and clamp to reachable bounds. Handle viewport resize, UI scale, minimum supported viewport, and corrupted/offscreen saved layouts.
- Settings windows remain reachable and usable independently of HUD configuration. Noninteractive overlay areas pass pointer events through; interactive UI never leaks pointer events into camera/targeting gestures.

### Persistence and accessibility

- Versioned, validated preference schema covers bars, slots, bindings, frame settings, and layout. Whitelist IDs, bound counts/scales/coordinates, migrate known older schemas, and handle unknown future versions without destructive overwrite.
- Recover safely from invalid JSON, unavailable storage, quota errors, and stale abilities/actions. Provide visible save-failure feedback; gameplay remains usable. Coalesce writes; do not persist every pointer movement or snapshot.
- Support named local profiles with create, select, rename, duplicate, delete, restore defaults, and validated JSON import/export. Profiles are local, not account synchronization. Avoid overwriting another profile silently.
- Accessible names, keyboard operation, focus trapping/return for dialogs, visible focus, Escape precedence, reduced motion, sufficient contrast, readable scaling, and restrained live announcements. Rapid health/cooldown updates should not flood screen readers.

## Implementation sequence and acceptance gates

Track completion here as work proceeds; each phase delivers working behavior, not placeholder panels.

1. [x] **Decision and build foundation:** add numbered accepted ADR for the user-approved React direction, establish pinned tools/typechecking/linting/test commands, static deployment/asset handling, and minimal React overlay. Preserve current runnable gameplay and diagnostics.
2. [x] **Runtime and state boundaries:** extract startup, bridge adapter, pointer lifecycle, stores, metadata and typed snapshots from shell.js; retain deterministic input behavior with regression tests. Define stable entity identity and any required Rust snapshot changes.
3. [x] **Domain configuration:** implement preferences schema/migrations, profiles, bindings/actions/conflicts, bars/slots, layout and focus transitions with pure unit tests, then persistence adapters and failure feedback.
4. [x] **Design system and unit frames:** implement original visual tokens/primitives, shared player/target/focus frames, real focus controls, practice feedback, loading/fallback/error and diagnostics views. No parallel legacy HUD owners.
5. [x] **Action bars and spellbook:** implement real slots, shared cooldowns, activation, configuration, drag/copy/move/swap/remove, tooltips, and non-drag placement. Test invalid/cancelled transactions and activation mapping.
6. [x] **Bindings end to end:** connect configurable movement/targeting/action/UI commands to input resolver; complete editor, conflicts, alternate bindings, reserved-key handling, and hover/focus quick binding. Test capture isolation and stuck-input regressions.
7. [x] **Layout and settings:** complete draggable/scalable/visible frames and bars, edit previews, bounds/resize recovery, profiles and import/export UI, reset flows, accessibility, and responsive styling. Test coordinate and schema boundaries.
8. [x] **Integration cleanup and documentation:** remove superseded code only after replacements are wired; update architecture, setup, browser instructions, CI triggers/scripts, optional diagnostic selectors and human acceptance checklist. Keep generated artifacts ignored and dependency licenses/provenance clear.
9. [x] **Verification and handoff:** run all relevant checks below; fix failures. Record actual results and pending human review. Do not claim completion while required behaviors are stubbed, skipped, or disconnected. Automated results are recorded in REACT-UI-VERIFICATION.md; human browser review is still a required pre-merge/release step, not an observed pass.

## Verification

- Preserve and extend `npm test` and its watch mode for deterministic input/domain tests. Include modifier chords, alternate binding release, conflict replacement, repeated keys, quick-bind isolation, focus transitions, slot transactions, bounds, profile switching/import and malformed persistence.
- Add focused DOM component tests for dialogs/focus, binding capture/hover, spell placement alternatives, slot rendering, errors, and settings controls. These are not a substitute for human pointer-lock/rendered testing.
- Run frontend typecheck, lint, tests, and production build. Check complete-build scripts and relative/static asset wiring.
- For Rust changes run `./scripts/check.sh`; add behavioral and bridge contract tests where appropriate. Run `cargo run -p arena-headless` for the scripted scenario.
- For client changes run `cargo clippy -p arena-client --target wasm32-unknown-unknown --locked -- -D warnings` and `./scripts/build-web.sh` (updated coherently for the frontend pipeline).
- Do not automatically invoke Playwright or browser E2E. Human review remains required before merge/release.
- Human checklist: startup/fallback/errors; four-hit practice loop; unchanged camera/movement; focus independent of target; bindings and conflicts; hover binding never casts; dragging never casts/turns camera; layout/scale/resize; storage reload/profiles; keyboard navigation; supported desktop viewports; console errors. Record browser/device and actual pass/failure status.

## Fresh-context kickoff prompt

> Implement planning/REACT-UI-IMPLEMENTATION.md in full. React is approved. Read repository instructions and the linked architecture/decision documents, inspect current Git changes, then proceed through implementation and verification. Maintain the plan checkboxes and actual verification results. Use focused modules and reusable composition, implement all listed UI features rather than placeholders, preserve original IP and Rust authority, and do not automatically run Playwright. Human rendered review must be reported as pending unless actually performed.
