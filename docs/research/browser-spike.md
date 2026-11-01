# Browser spike results

Date: 2026-10-31. **Conclusion: functional prototype verified; retain Bevy for evaluation. Production stack acceptance remains open.**

## What runs

A local, original primitive arena in Bevy 0.19/WASM/WebGL2. Camera-relative WASD movement, right-drag orbit, wheel zoom, Tab or ray-based click targeting, Escape to clear, targeted Pulse attacks, health/cooldown/range/line-of-sight feedback, a four-hit round, reset, and an optional read-only diagnostics bridge. The practice opponent does not move or attack. The shared Rust rules enforce movement normalization, world/pillar collision, range, line of sight, cooldown, and round completion.

## Verification observed

- Native formatting/Clippy and seven headless rule tests passed.
- WASM-target Clippy passed, including the browser-specific input/state bridge.
- Six Playwright interaction checks passed: the same three scenarios in Chromium 149.0.7827.55 and Firefox 151.0. These exercise gameplay, camera/zoom, focus loss, keyboard exit, diagnostics visibility, and visible loading failures.
- Screenshots were captured and the rendered graybox was visually inspected. Test screenshots/traces remain generated artifacts, not source assets.
- There were no captured JavaScript/console errors in the successful gameplay/diagnostics runs. WebGL capability/software-rendering warnings are expected; this does not imply a warning-free renderer.

A regression discovered during testing was Firefox retaining canvas focus after Shift+Tab. The browser shell now explicitly focuses Diagnostics, and the regression checks pass in both browsers. The fix required no Rust rebuild. The Rust collision/line-of-sight change was verified with native rules tests, compiler/Clippy feedback and browser interaction tests, demonstrating an AI-assisted code/test/browser loop.

## Initial measured baseline

Host: AMD Ryzen 7 9800X3D, approximately 30 GiB RAM, Ubuntu 24.04 under WSL2/Linux 6.6.87.2. Headless 1440×900 browsers, software GPU, uncompressed localhost HTTP, no network shaping. Other compilation activity occurred during these short samples. This is functional evidence, not a controlled hardware benchmark.

| Measurement | Chromium | Firefox |
| --- | --- | --- |
| Reported GPU | ANGLE / SwiftShader | llvmpipe or similar |
| First simulation snapshot | 479 ms | 940 ms |
| rAF interval p50 | 283.3 ms | 49.8 ms |
| rAF interval p95 | 516.7 ms | 66.7 ms |
| Frame samples | 23 | 25 |

First-snapshot time measures loader-to-state initialization, not first rendered GPU frame or time to interact over a real connection. Frame samples are short, include stalls, and measure rAF intervals rather than GPU execution. The proposed 60 fps target is **not demonstrated** by these results.

The initial prototype’s stripped module is **27,042,976 bytes** uncompressed and **5,896,097 bytes** with locally computed gzip compression; JS bindings are 101,044 bytes. The Python development server serves the uncompressed module. These gzip bytes are a size calculation, not an observed compressed-network transfer.

The first broad-feature build was 83,177,705 bytes uncompressed / 12,360,656 gzip. Restricting Bevy to PBR/windowing and removing unused engine features and debug-name sections reduced it substantially. A build after pruning reported **145 seconds** including bindings with partly warm dependencies. This is not a clean-build benchmark. A controlled incremental Rust edit timing is still pending; DOM HUD/input edits only need refresh. See planning/browser-spike-results.json for the machine-readable measurements and resource transfer entries.

The host lacked some standard browser libraries. Chromium/Firefox ran with pre-existing extracted libraries via LD_LIBRARY_PATH. Normal setup is documented in docs/BROWSER.md with Playwright's install --with-deps command; no host-specific library path is required by committed tooling.

## Remaining decision gates

1. Playtest on an agreed real desktop/laptop GPU and current user browsers; measure frame-time percentiles and input feel under stable conditions.
2. Test Safari on an actual supported Apple device. Playwright's Linux browsers do not establish Safari support.
3. Measure first visible/usable frame and compressed cold download over a representative connection; choose an explicit download budget.
4. Measure a controlled clean build and an actual incremental Rust edit, separately from warmed no-change builds and DOM edits.
5. Add authoritative networking only after the local slice is reviewed; later test two clients under latency/jitter as specified in ADR 0001.

Keep issue #1 open for these gates. If Bevy misses the agreed real-device load/performance/iteration targets after bounded optimization, evaluate Babylon.js before expanding production content. Current evidence supports continuing the small experiment, not committing to a full engine migration or production engine choice.

## Mouse-control follow-up

Left-drag now provides camera-only free-look; right-drag also turns the character; holding both buttons walks forward while steering. Pointer lock hides the cursor, partial release retains the remaining button's mode, and capture loss cancels mouse movement. Ordinary clicks still select targets. Zero-button pointer-lock recentering events are ignored to prevent camera jumps.

Native/WASM Clippy and seven rules tests passed. All eight interaction scenarios passed in Chromium 149.0.7827.55 and Firefox 151.0: Chromium was verified in complementary five- and three-scenario runs, Firefox in one eight-scenario run. Functional checks use a 1024×768 viewport; this does not replace the initial benchmark or real-device performance gates. Chromium mouse input uses a persistent trusted CDP input session to avoid high-level drag interception; button-hold checks use defined input durations and verify movement stops while rendered frames continue. Traces are opt-in because software-GPU screencasting adds substantial overhead.

Developer onboarding from a fresh machine is documented in [DEVELOPMENT.md](../DEVELOPMENT.md), including prerequisites for Linux, macOS and Windows/WSL, and the full clone/build/test/run sequence.
