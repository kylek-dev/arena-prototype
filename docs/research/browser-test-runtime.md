# Browser test runtime investigation

Historical report: automated rendered checks were subsequently replaced as development/CI gates by [ADR 0004](../decisions/0004-unit-first-validation.md). E2E review is now manual.

Date: 2026-11-06. Scope: functional test turnaround, not hardware performance acceptance.

## Evidence and method

PR #8's [passing CI run](https://github.com/kylek-dev/arena-prototype/actions/runs/42329626554) took 6.3 minutes for ten Chromium tests, with one optional asset test skipped. Its saved diagnostics report first simulation state at 661 ms, median requestAnimationFrame interval 966.6 ms, and p95 1000 ms. The renderer was ANGLE/SwiftShader, Chromium 149.0.7827.55. Compilation succeeded before testing. The [parallel push run](https://github.com/kylek-dev/arena-prototype/actions/runs/42329623079) ran the same commit again and failed the fixed-duration both-button movement assertion.

Local experiments use Playwright 1.61.1's Chromium 149.0.7827.55, SwiftShader, one worker, Ubuntu 24.04 / WSL2, and an AMD Ryzen 7 9800X3D. Tracing is disabled. Browser runtime libraries were extracted outside the repository and supplied through LD_LIBRARY_PATH. No machine configuration or downloaded runtime libraries are committed.

The first local baseline used the existing web build and the optional model: six tests passed, then left-drag timed out at 141 seconds including cleanup. The run was interrupted after 213 seconds; this is not a successful whole-suite baseline. Median frame interval in its diagnostics was 166.6 ms, p95 250 ms.

A half-density (0.5) experiment kept the 1024×768 CSS viewport but failed actual click targeting. It was rejected: changing render density would require investigating the client coordinate adapter. The implementation instead keeps normal pixel density and reduces viewport dimensions.

An 800×600 experiment used the original test bodies and a generated no-model manifest to match clean CI. Gameplay took 11.7 seconds and diagnostics 9.8 seconds, versus 18.6 and 15.9 seconds in the model-enabled local baseline. Median frame interval was 100 ms and p95 183.3 ms. Asset availability differs between these experiments, so these results do not isolate viewport savings. The old mouse synchronization remained problematic.

## Changes

- Smaller, normal-density interaction viewport; the camera/diagnostic scenario retains 1024×768 desktop rendering and screenshot evidence.
- General gameplay scenarios always use the procedural character. Dedicated character tests retain optional model success, absent-model startup, and download-failure recovery coverage.
- Both-button tests wait for real movement while holding real input, then verify position stability while simulation ticks advance. A single in-page wait avoids repeated DevTools polling during capture. No state, clock, or synthetic DOM input injection is added.
- JSON test reports include per-test and named Chromium input-step timings; graphics diagnostics include actual render-buffer dimensions and pixel ratio.
- CI runs once per PR update and on main pushes, with superseded runs cancelled. Feature branches can run CI manually before opening a PR.

## Verification and selected configuration

The selected interaction viewport is **800×600**, with the diagnostic scenario retained at 1024×768. The first [revised CI run](https://github.com/kylek-dev/arena-prototype/actions/runs/42331670878) passed all ten applicable browser tests (one optional model test skipped) in **256.0 seconds / 4.3 minutes**, versus approximately **379 seconds / 6.3 minutes** in the original CI run: about **32% less browser-test time**. Both use GitHub-hosted Ubuntu, the same pinned browser, SwiftShader, a procedural character, and one worker. They are separate runner instances, so this is observed CI turnaround, not a controlled hardware benchmark or guaranteed speedup.

The revised run retained all scenarios, including full desktop diagnostics. Its two button-order scenarios took 33.3 and 31.8 seconds. Named input steps show captured mouse movement itself can cost several seconds. Compilation, native checks, WASM Clippy, and the scripted headless scenario also passed in CI.

The 640×500 experiment was rejected. Fixed mouse coordinates initially hit the diagnostics overlay; centering the input anchor fixed capture, but a later both-button scenario still timed out. A test-only trusted-event observer was also investigated as a way to avoid waiting on delayed protocol acknowledgements; it is not part of the selected implementation. Smaller dimensions and more complex input delivery were not justified by reliable end-to-end results.

Local full-density/model-enabled and 640×500 runs encountered timeouts; the intermediate original-test 800×600 run took 220 seconds with nine passes, one skipped model test, and one WASM `unreachable` failure. These are diagnostic results, not passing baselines. Local wall-clock anomalies were also observed (including a negative shell build duration). Use the linked CI timing for the turnaround comparison; do not interpret local wall-clock durations as precise performance measurements.

Local native checks, all seven simulation tests, WASM Clippy, the web build, and the scripted headless scenario passed after selecting the final configuration. Browser artifacts and per-step timings remain ignored build/test outputs; only this summarized evidence is committed.

The [repeat CI run on the final code](https://github.com/kylek-dev/arena-prototype/actions/runs/42332325683) also passed all ten applicable browser tests, with the optional model test skipped, in **294.8 seconds / 4.9 minutes**. Both native and browser jobs passed. Across the two revised runs, observed browser-test time fell **22–32%** from the original 6.3-minute run; runner variability remains visible. This is still a multi-minute software-rendered suite.

The fresh local build's final full browser run passed **9 of 11** tests, including the optional model and full desktop diagnostics. Both button-order scenarios timed out at the second tick-progress wait after partial release. A follow-up using ordinary state polling also stalled and was interrupted; that alternative was reverted. These local capture/timing failures remain unresolved. The branch does not claim universal browser stability or that every form of pointer-lock flakiness is fixed. Both button-order checks passed in each revised CI run.
