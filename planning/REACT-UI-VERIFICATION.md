# React combat UI verification and handoff

Date: 2026-11-07. Implementation is wired, with no decorative placeholder feature panels. **Human rendered review and hosted CI execution remain pending.** No Playwright or browser E2E was invoked.

The user reported focus, layout and presentation issues in the first implementation. The follow-up scope is implemented under [ADR 0008](../docs/decisions/0008-gameplay-first-ui.md); the results below cover its automated regressions, not a claim that revised rendering has been reviewed by a person.

Cooldown follow-up: reversed the old counterclockwise shrink into a clockwise reveal and replaced snapshot-stepped styling with a shared requestAnimationFrame presentation timeline. Cooldown starts/ends bypass the normal HUD throttle. Animation never changes combat readiness; reduced motion keeps static shading. Automated tests cover interpolation, direction angles, duplicate assignments, stale samples/reset, lifecycle cleanup and no per-frame slot rerenders. Revised rendered review remains pending.

Spell-drag follow-up ([ADR 0009](../docs/decisions/0009-pointer-spell-placement.md)): the user reported that no spells could be dragged/dropped. Replaced native HTML dragging with captured pointer presses/movement/release, preventing canvas focus reclamation during the armed phase. Added ghost/target feedback, explicit unlock for saved locked profiles, and shared 64×64 px base icons/bar tracks. Existing configured scale/fitting still apply. Tests now exercise production pointer sequences rather than synthetic dragstart/drop; only jsdom hit testing and pointer capture are mocked. No browser reproduction/acceptance pass is claimed.

## Delivered

- React/TypeScript sibling overlay; stable Bevy-owned canvas; separate bridge, runtime, domain, stores, features and reusable UI primitives. Legacy shell/HUD removed after replacement.
- Shared player/actual-target/identity-based-focus frames; absent target/focus hide. Layout mode always shows labelled samples.
- Bounded stable-ID bars, Pulse-only icon spellbook, shared authoritative cooldowns, metadata, mapping, tooltips, catalog copy, slot move/swap/Shift-copy and canvas-drop removal with slot/bindings retained. Dropdown/non-drag assignment forms were removed at the user's explicit request; assignment is drag-only.
- Physical-code primary/secondary binding editor, search/categories, clear/defaults, explicit conflicts, reserved-key feedback, quick hover/focus binding, and exclusive input arbitration. Mouse controls retain the original deterministic state machine.
- Gameplay-first canvas focus recovery with typing/modal/navigation exceptions; bindable Escape menu replaces top-right controls. Old default Escape-clear migrates to Shift+Escape-clear without stealing custom Escape uses.
- Viewport-normalized whole-frame dragging/arrow movement with direct click selection, no frame dropdown/X/Y sliders/drag bars, and a movable editor window. Sample previews, independent locks, visibility, scale, resets and resize fitting.
- Radix Themes 3.3.0 neutral dark styling and components. New/default profiles start with assignments unlocked and no warning banner; existing saved lock choices remain intact.
- Validated versioned preferences, schema-0 import support, eight named local profiles, create/select/rename/duplicate/delete/restore, additive import/export, debounced persistence, preserved invalid/future storage and visible save failure/retry/recovery.
- Loading, fallback/error, practice and opt-in diagnostics; accessible modal primitives, focus return, visible focus and reduced motion. Readable multi-line source plus formatter commands.
- Updated setup/build/static hosting/CI/docs and human checklist. No networking, new combat abilities, opponents, account sync, macros, scripting or runtime model dependency added.

## Observed checks

Environment: Linux; Node 24.12.0 / npm 11.6.2; repository-pinned Rust/WASM tooling. Optional local character was absent; complete build generated the procedural fallback manifest.

| Check | Actual result |
| --- | --- |
| `./scripts/check.sh` | Passed: workspace formatting and native Clippy, 7 rules tests, 5 client presentation/movement tests, frontend typecheck/lint/format and all frontend tests |
| `npm test` | Passed: 24 original input tests + 86 Vitest domain/DOM/runtime tests (110 total), including captured-pointer copy/move/discard, short press versus drag, fast release, focus recovery, cancellation, saved locks/unlock, stale/profile boundaries and unavailable-spell placement |
| `npm run typecheck`, `npm run lint`, `npm run format:check` | Passed |
| `cargo clippy -p arena-client --target wasm32-unknown-unknown --locked -- -D warnings` | Passed |
| `./scripts/build-web.sh` | Passed: Rust web build, bindings, fallback asset manifest, frontend production build, static resource check |
| `npm run build` | Passed independently, without invoking Rust |
| `python3 scripts/check-static-assets.py` | Passed: six resources reachable with a real `/dist/` HTTP prefix, relative bundle URLs and `application/wasm` MIME. No browser used |
| `cargo run -p arena-headless` | Passed: four scripted hits, victory at tick 90, player health 100 / opponent health 0 |
| `npm audit` | Zero reported vulnerabilities |
| `python3 scripts/test-client-build-needed.py` | Five tests passed; existing selective-build behavior retained |
| Watch smoke test (initial implementation) | Both watchers ran initial suites and waited for changes; deliberately terminated after five seconds (exit 124), not a failed suite. Watch scripts are unchanged in this revision |
| `git diff --check` | Passed |

Build warning policy: ignore only dependency `use client` module-directive warnings in this deliberately client-only application; other warnings remain visible. This is not SSR/RSC support. Radix Themes ships its full published CSS; no performance improvement is claimed. Observed WASM output: 31,076,218 bytes, gzip 6,724,117 bytes; warmed build times are not cold-start or rendering benchmarks.

Observed pointer-drag revision frontend output: approximately 703 KB CSS (84 KB gzip) and 337 KB JavaScript (108 KB gzip). These are build sizes, not measured browser startup/rendering performance. Full native checks, WASM Clippy, the complete build/static-resource check and npm audit passed again for this revision; no rendered/browser pass is claimed.

## Required human gate — NOT performed

Use `npm run serve` and review [docs/BROWSER.md](../docs/BROWSER.md#manual-e2e-acceptance). Record browser/version, OS/device/GPU, viewport, console errors and actual results before merge/release:

- [ ] Startup, procedural fallback, optional-model failure and actionable errors.
- [ ] Four-hit practice loop; unchanged character-relative movement and camera gestures.
- [ ] Ordinary click targeting; independent focus; latest health/reset; absent target/focus frames hide.
- [ ] Gameplay focus recovery after HUD clicks; no stolen typing/modal/Shift+Tab focus; movement with spellbook open.
- [ ] Escape menu and rebinding, Shift+Escape clear; primary/secondary entries, conflicts, capture cancellation and browser shortcuts.
- [ ] Quick binding never moves/targets/casts/resets; keyboard slot selection and unbinding.
- [ ] Icon-only drag/copy/move/swap and canvas removal (bindings retained); cancellation/lock protection; no casting/camera motion during drags. No assignment forms should remain.
  Check 64×64 base sizing, pointer ghost/drop outline, drag before native focus changes, fast releases, click-versus-drag, uncastable spell placement and the saved-profile Unlock spell dragging control.
- [ ] Cooldown reveals clockwise smoothly between snapshots, all Pulse copies stay synchronized, starts/ends feel immediate, tab suspension recovers correctly, reduced motion uses static shading and animation never enables an early cast.
- [ ] Unlocked defaults without banner, saved locks respected; always-sample layout frames, direct selection, whole-frame dragging, movable editor window, scale/locks/visibility/resets and resize/reachability.
- [ ] Reload, profiles, additive JSON import/export and storage-failure recovery.
- [ ] Dialog focus/navigation, readable contrast/scale and reduced motion.
- [ ] Desktop viewports (800×600 and larger), diagnostics opt-in and console inspection.

Human browser/device: **not recorded — revised review pending**. User feedback identifies earlier problems, not a recorded acceptance pass for this revision. jsdom and static HTTP checks do not establish GPU rendering, pointer-lock permission, drag geometry, ray picking, accessibility-tool compatibility or performance. Drag-only spell assignment is a requested keyboard-accessibility tradeoff, not equivalent keyboard support.

## Run locally

The complete build already exists locally. Start `npm run serve` for production output, or `npm run dev` for frontend hot reload. Fresh checkout: `npm ci && ./scripts/build-web.sh` first. Publish only `dist/`. Use `npm run format` for normal multi-line formatting and `npm run test:watch` for both fast test loops. See [frontend boundaries](../docs/FRONTEND.md) and [ADR 0007](../docs/decisions/0007-react-combat-ui.md).
