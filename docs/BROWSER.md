# Browser development

This is a local solo practice scene. Start the original Rust headless example with `cargo run -p arena-headless`, or build the browser client below. The public build runs with a procedural character and needs no optional downloads or credentials. To use the local PixelLabs reference model, follow [the asset setup instructions](../assets/characters/README.md), then rebuild.

## Build and play

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.122 --locked
./scripts/build-web.sh
python3 -m http.server 8000 --directory web --bind 127.0.0.1
```

Open http://127.0.0.1:8000. Build output is generated under web/pkg and ignored by Git. The shell uses relative URLs, so it can also run under a static hosting subpath. Production static hosting should use HTTPS and Brotli/gzip compression for WASM, JavaScript and CSS, with `application/wasm` MIME type for the module. The simple Python server does not compress downloads.

Controls:

- **WASD:** move relative to character facing. W/S move forward/backward; A/D strafe left/right. Movement never turns the character.
- **Left-drag:** orbit the camera only. Character facing and movement direction remain independent of the camera, including after release or stopping and restarting movement.
- **Right-drag:** orbit the camera and turn the character to face that direction.
- **Left + right held:** walk forward while steering and turning with the camera.
- **Wheel:** zoom. **Tab or ordinary left-click:** select. **Escape:** clear/cancel capture.
- **1 / Pulse:** attack. **R / Reset:** restart.

Mouse gestures hide and lock the cursor. Releasing one button stops mouse-driven walking and keeps the remaining button’s look/turn mode; releasing both restores the cursor. Drags and button chords never count as targeting clicks. Lost pointer lock, focus loss, and tab hiding cancel held mouse movement. If a browser denies pointer lock, the cursor is hidden over the canvas and dragging works within the page.

Click the canvas to return keyboard focus after using other controls. Shift+Tab leaves it for ordinary keyboard navigation; modified browser shortcuts remain available. Mobile controls and key remapping are not implemented yet.


The opponent is stationary and does not attack. Its health reaching zero ends the round. Walls and the two square pillars block movement. Pillars also block Pulse's line of sight. The HUD shows attack rejection feedback, range, target health, and cooldown.

## Fast automated verification

```sh
npm test
npm run test:watch
cargo test -p arena-sim --locked
```

`npm test` runs the production `web/input.js` state machine through Node's built-in test runner. It needs Node 24, with no npm install, browser, server, assets, renderer, or WASM build. Tests cover keyboard filtering, repeated actions, movement, click/drag thresholds, both button orders, partial release, held keyboard movement, cancellation, lock loss/late grants/denial, and the positional Rust input contract. Rust tests validate damage, cooldowns, range, collision, line of sight, command budgets, replay order, and terminal rounds.

For the complete code checks and a browser build:

```sh
./scripts/check.sh
cargo clippy -p arena-client --target wasm32-unknown-unknown --locked -- -D warnings
./scripts/build-web.sh
```

CI runs Rust rules/headless linting and tests, workspace formatting, JavaScript syntax checks, and input unit tests on PRs and pushes to `main`; superseded runs for the same PR are cancelled. The required `browser` job keeps its existing name but now runs JavaScript checks only. Neither automatic job compiles Bevy or WASM.

The separate `Full client build` workflow checks changed files on every PR and main push. Changes to Rust crates, Cargo manifests/locks, the Rust toolchain, `.cargo` configuration, assets, scripts, or workflow files trigger full native workspace checks, WASM Clippy, and the web build, uploading `browser-build`. Docs-only and JavaScript-only changes skip those expensive jobs. The required `client-build` check passes only when both builds succeed or change detection confirms neither is needed; detection failures block it too. It does not install or launch a browser.

Before release, or to check any branch on demand, run the complete local checks above or dispatch `gh workflow run client-build.yml --ref YOUR_BRANCH`, which always requests full builds. A successful `client-build` check on an unrelated change means compilation was intentionally skipped, not observed to pass. Record human review separately. See [ADR 0006](decisions/0006-selective-client-builds.md).

## Manual E2E acceptance

E2E testing is a human review step, not an automated development or CI gate. Build, run `npm run serve`, and review these scenarios in the intended browser:

1. **Startup and assets:** a clean clone loads the procedural player with no missing model requests. If the optional character is installed, verify its appearance and movement. An unavailable configured model should retain a playable fallback and show a notice.
2. **Combat loop:** WASD approaches the target; Tab and an ordinary click select it; 1 reports no target/out of range/recharge correctly, deals four hits to finish, and R or Reset starts a fresh round.
3. **Mouse modes:** left drag changes only the view; hold W, left-drag, and release left-click to verify that character heading, walking direction, and camera angle remain unchanged. Stop, orbit the camera, release, then press each WASD key: movement must remain character-relative, with A/D strafing and S backing up without turning. Right drag turns without walking; both buttons walk and steer in either press order. Releasing either button stops mouse walking and retains the remaining look/turn mode. Releasing both restores the cursor. Drags/chords must not select a target.
4. **Cancellation and focus:** held keyboard movement survives ordinary mouse release; loss of capture, canvas/window focus, or tab visibility clears held input. Shift+Tab leaves the canvas and browser shortcuts work. Wheel zoom remains usable.
5. **Presentation and errors:** inspect the HUD, arena, target indicator, model, and camera at desktop dimensions; check the console. `?debug=1` exposes read-only diagnostics, while `/` does not expose `arenaDebug`. Loading failures must remain visible.

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

Open `/?debug=1` to expose the diagnostics panel and read-only `window.arenaDebug.state` and `.metrics`. The HUD can be toggled with Diagnostics. The state includes rules tick, integer positions, target selection, cooldown, feedback, and camera state. `firstStateMs` measures module loading/initialization to the first simulation snapshot, **not** the first GPU frame or a network benchmark. Frame percentiles measure requestAnimationFrame intervals after engine warmup, including any stalls; they do not isolate GPU time. Tests save the browser version and transfer sizes alongside them.

The JavaScript shell-to-engine adapter exists to support this practice scene. No debug command injection or remote control endpoint is exposed. Remove or gate the bridge when designing public competitive sessions.

For an incremental build timing, change a small client detail and rerun the build script; it prints wall time and module size. Record the profile, machine, dependency cache, and what changed. Fresh dependency compilation, warmed no-change builds and real incremental edits are different measurements. Shipping bindings strip WASM debug names; use `ARENA_DEBUG_SYMBOLS=1 ./scripts/build-web.sh` to retain them for readable crash stacks at the cost of download size.

The mouse adapter tracks `mousedown`/`mouseup` because `pointerdown` fires only on the first button press in a chord ([MDN](https://developer.mozilla.org/en-US/docs/Web/API/Element/mousedown_event)). Cursor capture uses the [Pointer Lock API](https://developer.mozilla.org/en-US/docs/Web/API/Pointer_Lock_API); zero-button recentering events are ignored instead of being treated as drags.

## Character asset

Bevy's glTF loader and PNG support load the optional local player model. The build generates an availability manifest, and the browser loads the model only when present. Loading waits for the character scene; failure retains the capsule with a visible notice. The model follows movement, facing and reset but has no embedded skeleton or animations. Use `window.arenaDebug.state.character` with `?debug=1` to inspect status and spawned mesh count. Source geometry/textures are preserved, and the downloadable GLB remains outside public Git history under its separate license.

## Historical browser test timing

The previous E2E runtime investigation and rejected optimization experiments are recorded in [browser-test-runtime.md](research/browser-test-runtime.md). Those rendered timings predate the unit-first policy and do not describe the current CI gates.
