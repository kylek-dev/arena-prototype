# ADR 0004: Unit-first validation and manual browser review

Date: 2026-11-06. **Status: accepted**, following the user's preference for rapid, code-based development validation and manual E2E testing. Supersedes ADR 0002's automated browser-check policy; engine, game rules, and production-stack status are unchanged.

## Decision

Run deterministic Rust simulation tests and Node 24 built-in unit tests as the ordinary development and CI gates. Extract browser input intent into `web/input.js`, independent of DOM events, graphics, OS clocks, or network I/O. Test explicit key/button transitions and input sampling directly, including both chord orders, partial release, cancellation, click suppression, and the JS-to-Rust array contract. Keep `web/shell.js` responsible for DOM bindings, focus, cursor capture, loading, and HUD presentation.

Retain native linting, WASM Clippy, the web build, and the headless scenario in CI. Preserve the required job names `rust` and `browser` so existing branch protection remains valid; `browser` now means compile-time browser-target validation and build artifact production. CI neither installs nor launches Playwright browsers.

Humans review E2E behavior before merging/releasing client changes, using the checklist in docs/BROWSER.md. Keep the existing Playwright suite and pinned dependency as an optional, explicitly invoked diagnostic tool. Do not run it automatically during implementation or CI. No new test dependencies are needed for the default JavaScript loop.

## Evidence and consequences

The previous rendered suite took 6.3 minutes in CI; viewport/input-wait improvements reduced observed runs to 4.3–4.9 minutes. Local captured-input and timing failures remained. See the historical [runtime investigation](../research/browser-test-runtime.md). These costs are unsuitable for the user's desired inner development loop.

Unit tests validate the production state machine, not a reimplementation or mocked browser. Rust tests continue to validate actual combat rules with explicit commands and ticks. Neither unit tests nor compilation establish pointer-lock permissions, DOM event wiring, ray picking, GPU rendering, model loading, browser compatibility, or performance. These remain manual acceptance work; report pending review honestly. Future domain logic should be testable below the rendering boundary before adding E2E coverage.

## Validation commands

- `npm test`: JavaScript unit tests, no browser, assets, server, or WASM build required.
- `npm run test:watch`: repeat those tests on code changes.
- `cargo test -p arena-sim --locked`: combat rules and regressions.
- `./scripts/check.sh`: native formatting/linting/rules plus JavaScript syntax and unit tests.
- WASM Clippy and `./scripts/build-web.sh`: browser-target compilation; see docs/BROWSER.md.

Node's built-in runner is documented in the [Node 24 test documentation](https://nodejs.org/docs/v24.12.0/api/test.html). Existing Rust/Bevy/WASM dependency pins remain unchanged.
