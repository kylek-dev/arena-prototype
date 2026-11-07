# Browser development

This is a local solo practice scene. Start the original Rust headless example with `cargo run -p arena-headless`, or build the browser client below. The public build runs with a procedural character and needs no optional downloads or credentials. To use the local PixelLabs reference model, follow [the asset setup instructions](../assets/characters/README.md), then rebuild.

## Build and play

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.122 --locked
npm ci
./scripts/build-web.sh
npm run serve
```

Open http://127.0.0.1:8000. Rust generates `web/public/pkg` and `web/public/assets`; Vite copies them with the React overlay to `dist/`. All output is ignored. Publish the entire `dist/` directory, not `web/`. URLs remain relative for subpath hosting. Use HTTPS, Brotli/gzip and `application/wasm`. A simple alternative is `python3 -m http.server 8000 --directory dist --bind 127.0.0.1` (no compression).

After one complete build, `npm run dev` provides frontend hot reload using the existing engine. `npm run build` rebuilds only TypeScript/CSS and copies existing engine assets; it cannot make a fresh checkout playable without the complete build. [Frontend boundaries and behavior](FRONTEND.md) documents ownership, profiles and input modes.

Controls:

- **WASD:** move relative to character facing. W/S move forward/backward; A/D strafe left/right. Movement never turns the character.
- **Left-drag:** orbit the camera only. Character facing and movement direction remain independent of the camera, including after release or stopping and restarting movement.
- **Right-drag:** orbit the camera and turn the character to face that direction.
- **Left + right held:** walk forward while steering and turning with the camera.
- **Wheel:** zoom. **Tab or ordinary left-click:** select. **Escape:** game menu. **Shift+Escape:** clear target.
- **1 / Pulse:** attack. **R / Reset:** restart.
- **F:** set focus from selected target. **O:** settings. **P:** spellbook. Bind clear/target focus and other interface commands in Settings.

Single-button presses leave the cursor and look/turn modes unchanged until cumulative movement reaches 4 pixels; ordinary left/right clicks do not request pointer lock or flash its browser notice. Crossing that drag threshold, or holding both buttons to walk, hides and locks the cursor. Releasing one button stops mouse-driven walking and keeps the remaining button’s look/turn mode; releasing both restores the cursor. Drags and button chords never count as targeting clicks. Lost pointer lock, focus loss, and tab hiding cancel held mouse movement. If a browser denies pointer lock, the cursor is hidden over the canvas and dragging works within the page.
All live unit frames (player, target, focus and opponents) support left-click to target their represented unit and right-click to focus it. Player self-targeting is supported, but Pulse cannot attack the player. Layout mode instead selects/drags sample frames. Unit tags reserve Opponent1–3 and Team1–3; only one opponent and no teammates exist today. Rebuild the complete client for the appended self-selection bridge field. Human browser review of these interactions is pending.

Mouse gestures hide and lock the cursor. Releasing one button stops mouse-driven walking and keeps the remaining button’s look/turn mode; releasing both restores the cursor. Drags and button chords never count as targeting clicks. Lost pointer lock, focus loss, and tab hiding cancel held mouse movement. If a browser denies pointer lock, the cursor is hidden over the canvas and dragging works within the page.

Canvas focus is reclaimed after ordinary HUD interaction; gameplay also routes while a non-keyboard-owning HUD control briefly has focus. Text fields, modal dialogs, binding capture, quick bind and layout editing own/suspend keyboard input and clear holds; plain HUD clicks and the nonmodal spellbook do not. Window focus loss, hiding and unexpected pointer-lock loss still cancel input. Shift+Tab explicitly opts into UI navigation without automatic focus reclamation; clicking in the page or explicitly refocusing the canvas resumes gameplay routing.

Escape opens the game menu instead of a top-right toolbar. Its `menu` action is remappable in Settings alongside movement, targeting, reset, focus, slots and UI toggles. Physical unmodified and Shift chords are supported; Ctrl/Alt/Meta/function keys and Shift+Tab remain reserved. Escape cancels captures except when assigning the menu action; use Cancel to leave that capture. Old default Escape-clear bindings migrate to the menu and Shift+Escape-clear; custom Escape uses and saved locks are preserved. Printed labels may differ on non-US layouts. Mobile controls are not implemented.

Spellbook contains only Pulse as an action-slot-sized draggable icon; tooltips show rule-owned metadata. New/default profiles start unlocked without a warning banner. Catalog drag copies; slot-to-slot drags move/swap; Shift-drag copies. Drop a slotted spell on the canvas to remove its assignment while retaining slot/bindings. Catalog-to-canvas and cancelled/invalid drops do nothing. Assignment is deliberately drag-only per user request: no dropdown or alternative placement/removal forms. Lock/unlock in Settings → Action bars; saved locks are preserved. Click/key activation shares assignments and authoritative cooldowns; dragging never casts. Unavailable actions explain why without submitting an attack; Rust validates submitted intents.

Quick bind edits a hovered/focused slot's primary key, offers Unbind, and exits with Escape/Done. Settings edits both entries with explicit Replace/Cancel for conflicts. Target/focus hide when absent. Layout editing always shows labelled sample frames, including hidden/absent units. Click a frame/bar to select it and drag its whole body (or use arrows): no frame dropdown, X/Y sliders or frame drag bars. Move the editor window by its title; adjust selected visibility/scale/lock and reset selected/all; Done/Escape exits. Assignment/position locks are independent. Limits remain four bars, twelve slots each, eight local profiles. Import adds unique profiles without overwriting another; invalid/future storage is preserved until explicit recovery. Export when saving fails. Minimum desktop viewport: 800×600; oversized content fits the viewport. Styling uses Radix Themes ([ADR 0008](decisions/0008-gameplay-first-ui.md)).


The opponent is stationary and does not attack. Its health reaching zero ends the round. Walls and the two square pillars block movement. Pillars also block Pulse's line of sight. All unit frames show only the unit name, green health bar and blue resource bar, with current/maximum values centered inside the taller bars. Resource values remain placeholders (identified in the tooltip and accessible text): resource costs and regeneration are not implemented. Role, selection/focus labels and frame descriptions are not displayed. Default/reset placement puts player and target side by side at the upper left, focus toward the center-right, and opponents at the right; existing saved positions are preserved. Use Layout → Reset all positions to adopt the new defaults in an existing profile.

The practice-round box is removed; reset remains available through R and the game menu. A dedicated top-center area shows bold red errors only when an attempted spell cast fails (including missing target, range, line of sight, recharge or an authoritative rejection). Passive availability changes do not show errors. Messages expire after 2.5 seconds; repeated failed attempts restart that interval, while a valid cast attempt or reset clears them immediately. Empty slots and suspended gameplay/editing do not count as cast attempts. Startup failures and recovery remain in the loading panel, not the combat-error area. Human rendered review of these HUD changes is pending; automated DOM tests, frontend lint/format checks, WASM Clippy and the complete web build passed locally on 2026-11-07. No rendered/browser pass is claimed.

Action-bar slots show spell icons and keybinding labels, without printed spell names or separate slot-number labels. Tooltips retain spell details and bindings, accessible names retain slot identification, and cooldown/key-held feedback is unchanged. Human rendered review of this simplification is pending.

## Fast automated verification

Assigned action buttons use a subtle red tint for defeated targets, round completion, out-of-range targets or blocked line of sight. With no target selected, buttons stay neutral. Other unavailable states tint grey, including recharge and any future resource rejection reported by the engine; resource costs are not implemented today. Empty slots stay neutral. Keybinding labels and cooldown overlays remain visible. Human rendered review of these tints is pending.

```sh
npm test
npm run test:watch
cargo test -p arena-sim --locked
```

`npm test` runs the original Node input suite plus Vitest domain/DOM/runtime tests after `npm ci`, without a browser, server, assets, renderer or WASM build. `npm run test:input` remains Node-only with no npm install. Tests cover physical chords, alternate holds/modifier release, repeats, conflicts, capture/quick-bind isolation, typing, focus selection, atomic slots, bounds, profile operations/import, malformed/future storage, dialogs/focus, errors and settings. The original mouse/cursor and positional Rust input contract regressions remain intact. DOM tests do not implement browser layout. Rust tests validate combat and presentation contracts.

For the complete code checks and a browser build:

```sh
./scripts/check.sh
cargo clippy -p arena-client --target wasm32-unknown-unknown --locked -- -D warnings
./scripts/build-web.sh
cargo run -p arena-headless
```

CI runs Rust rules/headless linting/tests and workspace formatting. Its required `browser` job installs pinned npm packages and checks frontend typechecking, lint, formatting, input/domain/DOM tests and a frontend production build. Neither automatic fast job compiles Bevy/WASM or installs a browser. See [ADR 0007](decisions/0007-react-combat-ui.md).

The separate `Full client build` workflow checks changed files on every PR and main push. Changes to Rust crates, Cargo manifests/locks, the Rust toolchain, `.cargo` configuration, assets, scripts, or workflow files trigger full native workspace checks, WASM Clippy, and the web build, uploading `browser-build`. Docs-only and JavaScript-only changes skip those expensive jobs. The required `client-build` check passes only when both builds succeed or change detection confirms neither is needed; detection failures block it too. It does not install or launch a browser.

Before release, or to check any branch on demand, run the complete local checks above or dispatch `gh workflow run client-build.yml --ref YOUR_BRANCH`, which always requests full builds. A successful `client-build` check on an unrelated change means compilation was intentionally skipped, not observed to pass. Record human review separately. See [ADR 0006](decisions/0006-selective-client-builds.md).

## Manual E2E acceptance

E2E testing is a human review step, not an automated development or CI gate. Build, run `npm run serve`, and review these scenarios in the intended browser:

1. **Startup and assets:** a clean clone loads the procedural player with no missing model requests. If the optional character is installed, verify its appearance and movement. An unavailable configured model should retain a playable fallback and show a notice.
2. **Combat loop:** WASD approaches the target; Tab and an ordinary click select it; 1 reports no target/out of range/recharge correctly, deals four hits to finish, and R or Reset starts a fresh round.
3. **Mouse modes:** left drag changes only the view; hold W, left-drag, and release left-click to verify that character heading, walking direction, and camera angle remain unchanged. Stop, orbit the camera, release, then press each WASD key: movement must remain character-relative, with A/D strafing and S backing up without turning. Right drag turns without walking; both buttons walk and steer in either press order. Releasing either button stops mouse walking and retains the remaining look/turn mode. Releasing both restores the cursor. Drags/chords must not select a target.
4. **Cancellation and focus:** movement survives ordinary mouse release and HUD focus/clicks. Typing/modal/capture/editing owners, window blur, hiding and unexpected lock loss clear holds. Confirm canvas recovery never steals typing focus or Shift+Tab navigation and the spellbook stays open while moving. Verify Escape menu/rebinding and Shift+Escape clear. Independent focus follows current health and clears on reset; absent target/focus hide.
5. **Presentation and errors:** inspect the HUD, arena, target indicator, model, and camera at desktop dimensions; check the console. `?debug=1` exposes read-only diagnostics, while `/` does not expose `arenaDebug`. Loading failures must remain visible.
6. **Configuration:** remap movement and both binding entries, exercise Replace/Cancel, reserved shortcuts and capture cancellation. Quick-hover/focus binding must not cast, reset, select or move. Tab must navigate UI normally.
7. **Spell placement:** default unlocked without warning, catalog-copy, slot move/swap/Shift-copy, cancelled/invalid drops and slot-to-canvas removal (binding retained). Confirm icon sizes and no assignment forms. Drags must never turn the camera or cast. Lock to protect assignments; verify shared click/key mapping and cooldown.
   Icons and bar tracks now use a shared 64×64 px base size. Check the captured-pointer ghost/target outline, short click versus drag, release over icon text/cooldown layers, fast drags, Escape cancellation and outside release. If a saved profile is locked, explicitly use Unlock spell dragging in the spellbook. Native HTML drag events are no longer used.
   Verify the cooldown reveals clockwise smoothly rather than stepping with snapshots; duplicated Pulse slots must stay in phase. Test tab hiding/resume and reduced-motion static shading. Animation must never enable casting before Rust reports ready.
   Bound slots show a simple white highlight while their key is held, including empty slots and rejected casts (no target/recharging). Verify primary/alternate/Shift bindings, release and focus-loss cleanup, and no highlight while typing or binding. This feedback reflects input only, not spell activation. Human rendered review of this highlight is pending.
8. **Layout and persistence:** always-sample frames, direct click selection, whole-frame dragging/arrows, movable editor title, visibility/scale/locks and resets; no frame dropdown/coordinate sliders/drag bars. Resize at 800×600 and larger. Verify menu reachability, profiles/import/export/reload and storage-failure feedback.

**Revised UI review status (2026-11-07): pending human review.** The user reported focus, layout and presentation issues in the initial implementation. This revision addresses them with automated regressions; no browser/device, console inspection or rendered pass for the revision has been observed. See [implementation verification](../planning/REACT-UI-VERIFICATION.md).

Record browser/device, scenarios, console errors, and actual review status in the PR. Unit tests and a successful build do not establish DOM wiring, pointer-lock permissions, GPU rendering, ray picking, or browser compatibility. Rendering/input integration review remains pending until a person performs it.

## Optional Playwright diagnostics

The existing suite is retained for a person to invoke explicitly when useful; agents and CI should not run it automatically:

```sh
npm ci
npx playwright install --with-deps chromium
npm run test:browser
```

This command builds on an existing web build and starts its own static server. The default interaction viewport is 800×600 at normal density; the diagnostic scenario retains 1024×768. Use `ARENA_TEST_VIEWPORT=1024x768` for all scenarios at desktop dimensions. General control scenarios force the procedural character; character scenarios cover the optional asset separately. Artifacts and per-test timings stay in ignored `test-results/` and are not uploaded by CI. Keep one worker and tracing off for software-GPU diagnostics; use `ARENA_TRACE=1` only when a trace is useful. These runs are not hardware performance benchmarks.

On Linux, Playwright may require system libraries and sudo for `--with-deps`. Browser installation is optional for the normal unit-test loop. To opt into Firefox diagnostics, install its browser/runtime dependencies, then use `ARENA_TEST_FIREFOX=1 npm run test:browser -- --project=firefox`.

## Inspect and measure

Open `/?debug=1` for the diagnostics panel and read-only `window.arenaDebug.state` and `.metrics`; toggle it through Menu → Toggle diagnostics. State includes rules tick, integer positions, selection, cooldown, feedback and camera. `firstStateMs` measures initialization to the first snapshot, **not** the first GPU frame or a network benchmark. Frame percentiles are requestAnimationFrame intervals after warmup, not isolated GPU time. Optional diagnostics save browser version and transfer sizes.

The JavaScript shell-to-engine adapter exists to support this practice scene. No debug command injection or remote control endpoint is exposed. Remove or gate the bridge when designing public competitive sessions.

For an incremental build timing, change a small client detail and rerun the build script; it prints wall time and module size. Record the profile, machine, dependency cache, and what changed. Fresh dependency compilation, warmed no-change builds and real incremental edits are different measurements. Shipping bindings strip WASM debug names; use `ARENA_DEBUG_SYMBOLS=1 ./scripts/build-web.sh` to retain them for readable crash stacks at the cost of download size.

The mouse adapter tracks `mousedown`/`mouseup` because `pointerdown` fires only on the first button press in a chord ([MDN](https://developer.mozilla.org/en-US/docs/Web/API/Element/mousedown_event)). Cursor capture uses the [Pointer Lock API](https://developer.mozilla.org/en-US/docs/Web/API/Pointer_Lock_API); zero-button recentering events are ignored instead of being treated as drags.

## Character asset

Bevy's glTF loader and PNG support load the optional local player model. The build generates an availability manifest, and the browser loads the model only when present. Loading waits for the character scene; failure retains the capsule with a visible notice. The model follows movement, facing and reset but has no embedded skeleton or animations. Use `window.arenaDebug.state.character` with `?debug=1` to inspect status and spawned mesh count. Source geometry/textures are preserved, and the downloadable GLB remains outside public Git history under its separate license.

## Historical browser test timing

The previous E2E runtime investigation and rejected optimization experiments are recorded in [browser-test-runtime.md](research/browser-test-runtime.md). Those rendered timings predate the unit-first policy and do not describe the current CI gates.
