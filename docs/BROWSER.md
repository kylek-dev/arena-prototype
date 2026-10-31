# Browser development

This is a local solo practice scene. Start the original Rust headless example with `cargo run -p arena-headless`, or build the browser client below. No third-party game assets or credentials are required.

## Build and play

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.122 --locked
./scripts/build-web.sh
python3 -m http.server 8000 --directory web --bind 127.0.0.1
```

Open http://127.0.0.1:8000. Build output is generated under web/pkg and ignored by Git. The shell uses relative URLs, so it can also run under a static hosting subpath. Production static hosting should use HTTPS and Brotli/gzip compression for WASM, JavaScript and CSS, with `application/wasm` MIME type for the module. The simple Python server does not compress downloads.

Controls: WASD to move; right mouse drag to orbit; wheel to zoom; Tab or click Sentinel to select; Escape to clear; 1 or the Pulse button to attack; R or Reset to restart. Click the canvas to return keyboard focus after using other controls. Movement input clears on focus loss. Shift+Tab leaves the canvas for ordinary keyboard navigation; modified browser shortcuts remain available. Mobile controls and key remapping are not implemented yet.

The opponent is stationary and does not attack. Its health reaching zero ends the round. Walls and the two square pillars block movement. Pillars also block Pulse's line of sight. The HUD shows attack rejection feedback, range, target health, and cooldown.

## Verify

```sh
./scripts/check.sh
cargo clippy -p arena-client --target wasm32-unknown-unknown --locked -- -D warnings
npm ci
npx playwright install --with-deps chromium
./scripts/build-web.sh
npm run test:browser
```

The Rust check script type-checks the client natively, tests headless rules, and checks browser-shell syntax. WASM Clippy checks its target-specific bridge. Browser tests verify actual inputs, ray selection, cooldown rejection, win/reset, camera/zoom, focus loss, opt-in diagnostics, and visible loading failures. Screenshots, traces on failures, and metrics are written to ignored test-results. Playwright starts the local static server automatically; an existing server on port 8000 must serve this project's web directory.

On Linux the Playwright browser needs system libraries; `install --with-deps` may require administrative access. Do not assume a successful download means the runtime libraries are present. On the development host, Chromium was run with locally extracted runtime libraries supplied through LD_LIBRARY_PATH; see the results report. This host workaround is not required on machines with Playwright's documented system dependencies installed.

## Inspect and measure

Open `/?debug=1` to expose the diagnostics panel and read-only `window.arenaDebug.state` and `.metrics`. The HUD can be toggled with Diagnostics. The state includes rules tick, integer positions, target selection, cooldown, feedback, and camera state. `firstStateMs` measures module loading/initialization to the first simulation snapshot, **not** the first GPU frame or a network benchmark. Frame percentiles measure requestAnimationFrame intervals after engine warmup, including any stalls; they do not isolate GPU time. Tests save the browser version and transfer sizes alongside them.

The JavaScript shell-to-engine adapter exists to support this practice scene. No debug command injection or remote control endpoint is exposed. Remove or gate the bridge when designing public competitive sessions.

For an incremental build timing, change a small client detail and rerun the build script; it prints wall time and module size. Record the profile, machine, dependency cache, and what changed. Fresh dependency compilation, warmed no-change builds and real incremental edits are different measurements. Shipping bindings strip WASM debug names; use `ARENA_DEBUG_SYMBOLS=1 ./scripts/build-web.sh` to retain them for readable crash stacks at the cost of download size.

To attempt the optional Firefox checks with its installed runtime dependencies: `ARENA_TEST_FIREFOX=1 npm run test:browser -- --project=firefox`. This is optional coverage, not part of the default Chromium CI job.
