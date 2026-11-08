# ADR 0026: Lightweight frame-timing diagnostics

Date: 2026-11-08. **Status: accepted**, following the user's request for an FPS/frame-time overlay to investigate visible stutters. Rendered review and measured overhead remain pending.

## Decision

Replace the default debug JSON panel with compact FPS, P50/P95/P99, maximum interval, a spike count and a six-second frame-time graph. Keep the existing `?debug=1` gate and menu visibility toggle. Mount the engine JSON subscription only after explicitly expanding **Show engine snapshot**, so ordinary engine delivery does not drive the compact panel.

Collect browser `requestAnimationFrame` intervals in fixed-size numeric rings (3600 samples), independent of engine ticks. Summaries publish at most four times per second and only while the debug overlay is enabled and visible. FPS is 1000 divided by the mean of samples ending within the last second; percentiles use nearest-rank over the retained sample window. Maximum and spike count use that same window, with spikes strictly greater than 1000/30 ms. Reset samples clears all frame history, not combat state or startup/error diagnostics. Counts are neither cumulative session totals nor refresh-rate-relative dropped-frame counts.

The graph shows the maximum interval per 50 ms bucket across the last six seconds, rather than averaging away single-frame spikes. Its vertical scale is fixed at 100 ms; larger spikes clip visually but retain their full numeric maximum. Dotted guides mark 16.7/33.3 ms. At very high refresh rates the 3600-sample capacity may retain less than six seconds. Empty buckets are zero. Hide/resume clears the sampling baseline, excluding hidden-tab gaps even if no animation callback runs while hidden. Not-ready periods do not contribute startup samples. Runtime teardown cancels the animation loop and visibility listener.

These are browser frame intervals, not GPU timings or proof of a stutter's cause. No automatic rendered/performance test or combat/network change is introduced.

## Verification and manual review

Deterministic tests cover empty history, nearest-rank tails, peak preservation, FPS, graph expiry, bounded rollover, spike thresholds, reset, invalid values, readiness, hidden-tab gaps, throttled publication and cleanup. DOM tests cover compact display, explicit JSON expansion, reset and hiding.

Local verification on 2026-11-08: `npm test` passed (28 Node input tests and 165 Vitest tests), frontend typechecking/lint/format checks passed, WASM Clippy with warnings denied passed, and the complete web build passed including static-resource/MIME checks. No browser was launched and no rendering/overhead measurement is claimed.

Manually open `/?debug=1`, keep the JSON collapsed, reset samples and reproduce a stutter. Compare 1v1/3v3 and graphics settings, noting P99/maximum/spikes and visible graph peaks. Check hide/resume and reset, menu toggling, intended screen sizes and console errors. A low-overhead design is not a measured performance guarantee; human browser review is pending.
