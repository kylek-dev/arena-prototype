# ADR 0027: Browser frame pacing independent of canvas focus

Date: 2026-11-08. **Status: accepted for the local browser prototype.** Addresses the user's observed walking/targeting choppiness. Human review of the correction remains pending.

**Human acceptance update (2026-11-08):** after testing the updated production 3v3 scene, the user confirmed “yep that worked its great now”. The walking/targeting choppiness correction is accepted; pending subjective-review statements below are superseded. This does not claim exhaustive scenario coverage or a controlled performance benchmark. See [human review record](../UI-REVIEW.md#subsequent-changes).

## Evidence

The user reported the same choppiness in Vite development and production builds. A Windows Chrome DevTools MCP recording failed to download (`IO.read: Read failed`); a direct loopback CDP capture using event delivery recovered diagnostic traces outside Git. Recording startup itself incurred profiler overhead and must not be counted as an ordinary game hitch. These are diagnostic observations, not repeatable hardware benchmarks.

A separate five-second production observation without the CPU profiler counted 1199 browser animation callbacks (239.8/s), but only 363 engine publications (72.6/s). Median engine publication interval was 16.3 ms, P95 21.1 ms and maximum 22.4 ms; the document was visible/focused and its active element was the arena canvas. Explicitly blurring/refocusing that canvas, without rebuilding, produced 1201 engine publications (240.2/s) over five seconds, median 4.2 ms, P95 4.6 ms and maximum 8.0 ms. Thus the independent browser FPS counter was not measuring actual engine update cadence.

The pinned Bevy 0.20.1 `WinitSettings::game()` uses continuous focused updates and an event-reactive approximately 60 Hz unfocused mode. The observations implicate canvas-focus-dependent runner pacing; the exact upstream focus-event ordering that left it throttled has not been isolated. The browser HUD deliberately supports gameplay across ordinary DOM focus changes (ADR 0008), so render pacing must not depend on that focus boundary.

## Decision

Insert `WinitSettings::continuous()` for the WASM practice client. Preserve `game()` defaults for native execution. The browser still controls hidden-tab animation throttling; existing blur/visibility/modal/typing/input cancellation stays intact. No forced focus loop, synthetic gameplay commands, simulation tick-rate change or extrapolation is introduced. Simulation remains 30 Hz, with existing interpolation.

Add a second bounded interval series to the diagnostics overlay, sampled from monotonically advancing engine frame publications before the ordinary HUD throttle. Label it **engine Hz**, not GPU/display FPS, and retain **browser FPS** separately. Exclude warmup, duplicate frames, frame-counter reset boundaries and hidden-tab gaps. Reset samples clears both series and baselines. Add engine P99/maximum and a cyan graph alongside the browser's gold graph; summary publication remains at most four Hz. Neither series proves physical presentation cadence or isolates all causes of choppiness.

## Verification

Unit regressions verify continuous browser update policy with and without canvas focus, unchanged native policy, and independent engine timing including duplicates, resets and hidden intervals. Run repository checks, WASM Clippy and a complete build. Then manually compare the updated engine Hz/browser FPS while stationary, walking, turning and targeting via frames/models/nameplates; include keyboard UI navigation, typing, dialogs, blur, hidden-tab resume and console errors. Record actual measurements separately from subjective acceptance. Raw captures remain outside Git.

Local automated verification on 2026-11-08: `./scripts/check.sh` passed (13 rules tests, 67 client tests, 9 asset-adapter tests, 28 Node tests and 166 Vitest tests), WASM Clippy with warnings denied passed, and the complete web build passed. The build used the procedural character fallback.

Observed post-build Windows Chrome check in production 3v3: over three seconds, browser and engine rates both measured 118.85 Hz; after focusing a teammate frame and allowing ordinary canvas-focus recovery, both measured 118.38 Hz over the next three seconds. Engine P95 intervals were 9.1/9.3 ms; maxima were 14.5/12.6 ms. No engine errors were reported. The browser callback rate in this check was approximately 120, not the approximately 240 of the earlier observation, so this is evidence that engine cadence tracked browser cadence across HUD focus—not a controlled cross-run speedup or GPU benchmark. Diagnostic sampling was reset after the check. Subjective user acceptance of walking/targeting remains pending.

Version-specific references: [Bevy 0.20.1 WinitSettings](https://docs.rs/bevy_winit/0.20.1/bevy_winit/struct.WinitSettings.html), installed `bevy_winit-0.20.1/src/winit_config.rs` and `state.rs`, and [winit 0.30.13 web implementation](https://docs.rs/crate/winit/0.30.13/source/src/platform_impl/web/).
