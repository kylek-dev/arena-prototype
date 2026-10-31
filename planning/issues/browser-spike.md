Build the first browser graybox and evaluate ADR 0001 before accepting the full stack.

Acceptance criteria:
- Pin an engine version and version-specific documentation.
- Render an original primitive arena, character, follow camera, movement and target indicator.
- Report build time, download size, time to interact, frame times, console errors and named browser/device coverage.
- Document one AI-assisted code change verified through compiler/tests and browser observation.
- Record accept/reject evidence and the Babylon.js fallback decision.

See docs/decisions/0001-stack.md. No production art required.


## Implementation update — 2026-10-31

The browser practice scene is implemented with pinned Bevy/WASM tools, shared Rust arena rules, plain HTML/CSS HUD, camera and ray targeting, cooldown/range/line-of-sight feedback, round completion, and reset. See docs/BROWSER.md and docs/research/browser-spike.md for verification and measurements. The production stack decision remains proposed. This issue stays open for representative hardware performance, Safari coverage where available, realistic network load/startup measurements, and incremental build evaluation.
