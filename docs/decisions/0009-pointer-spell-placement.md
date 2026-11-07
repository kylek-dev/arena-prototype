# ADR 0009: Captured pointer gestures for spell placement

Date: 2026-11-07. **Status: accepted for the practice UI**, implementing the user's request to fix unusable spell dragging and use square 64×64 px icons. Supersedes ADR 0007's native HTML drag implementation, not its pure assignment transactions or Rust combat authority.

Native HTML dragging was coupled to automatic canvas focus recovery and source rendering changes. Synthetic dragstart/drop tests did not exercise the real press-to-drag sequence. The user reported that dragging did not work. A specific browser failure has not been independently reproduced; do not describe DOM tests as rendered validation.

Use a small pointer-capture adapter in `runtime/spell-drag.ts`: arm on left press, preserve source focus, cross a four-pixel threshold, then resolve release coordinates with `elementFromPoint`. A visible pointer-transparent icon and destination outline give feedback. Native HTML dragging is disabled, avoiding competing gesture systems. The adapter calls the existing pure copy/move/swap/remove transitions; it does not cast, turn the camera or author combat state. Shift copies; canvas release clears the source assignment only. Empty slots remain drop targets even when combat activation is unavailable.

Escape, pointer cancellation/lost capture, blur, hiding, outside releases, stale sources, saved locks and profile switches cannot remove unrelated assignments. A post-drag click is consumed before it reaches activation; sub-threshold presses remain clicks. Focus recovery waits while a spell press is armed, not only after dragging has begun. The controller is installed/disposed with the runtime.

Keep existing saved lock choices; show an explicit **Unlock spell dragging** button in the spellbook only while locked. No placement dropdowns or alternative assignment forms return. All bar tracks, spellbook icons and drag previews use the shared `--action-slot-size: 64px` token, including borders. Existing layout scaling/viewport fitting still apply to the base size.

No dependency was added. Tests use the production pointer handlers and assignment reducers, mocking only missing jsdom pointer capture/hit testing; human review of actual hit geometry, click-versus-drag behavior and browser compatibility remains required. Playwright is not run automatically.
