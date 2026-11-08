# ADR 0022: World health nameplates

Date: 2026-11-08. **Status: accepted**, following the user's request for clickable floating health bars, separate friendly/enemy settings and keybinds, and no self nameplate.

## Decision

Render compact health-only HTML bars above other characters, with red enemy and green friendly fills. No printed names, numbers, resource bars or fake teammates. Left-click targets and right-click focuses through existing unit-tag commands; no combat authority or positional input ABI changes. The current practice roster has one opponent and no teammates. Friendly filtering supports the reserved Team1–3 tags but cannot display real teammates until the engine/roster supplies them.

Rust appends an optional `nameplates` projection array to snapshots: stable unit tag plus normalized viewport X/Y. Publish after Bevy 0.20.1 camera updates and transform propagation, using interpolated fighter transforms and a 2.25 m overhead anchor rather than fixed-tick positions. Behind-camera/offscreen anchors are omitted. Nameplates remain visible and clickable through pillars, per the user's follow-up correction; no geometry-occlusion check applies to this overlay. This is presentation visibility, not an authoritative multiplayer visibility rule. The player is never emitted; the frontend additionally excludes the player's identity.

The bridge validates unique non-self projection tags and finite viewport bounds. A separate render-rate store receives validated snapshots before ordinary HUD throttling, so movement/camera updates and health do not wait for the 80 ms HUD cadence. Only the nameplate feature subscribes to this store. Older engines without projections display no bars. Menus/settings, editing/capture, spell drags and startup errors suppress the live layer; interaction retains existing gameplay guards. Bars do not initiate canvas camera gestures; left drags beyond the normal click threshold do not target.

Add `nameplates.friendly/enemy` to profile configuration with additive enabled defaults and a Settings → Nameplates section. Profile persistence, export/import and recovery apply normally. New remappable actions default to V (enemy) and Shift+V (friendly); migration preserves existing custom uses and explicit unbindings. Toggles use ordinary one-shot gameplay routing, including repeat/typing/modal suppression. Nameplate-only preference changes preserve held movement and mouse steering; other configuration edits retain their existing cancellation behavior.

## Verification

Domain/DOM/runtime regressions cover visibility categories, no self/absent units, live projection/health updates independent of HUD delivery, target/focus intents, drag-click suppression, editing suppression, settings, profile migration/round-trip, custom key preservation, remapping, held movement and keyboard ownership. Rust tests cover rendered translation/height, behind-camera/offscreen rejection, high-DPI normalization, missing viewport and segment-limited pillar occlusion. `./scripts/check.sh` passed locally on 2026-11-08 (60 client tests, 12 simulation tests, 9 asset-adapter tests, 26 Node input tests and 139 Vitest tests), as did WASM Clippy with warnings denied. Human rendered review remains pending: moving/jumping characters, camera orbit/zoom, viewport resizing/DPI, bar hit areas, pillar occlusion, loading/reset, persistence, keyboard toggles and console errors. No rendered or performance pass is claimed.

## Human review update — 2026-11-08

The user verified and approved all implemented UI changes, including world health nameplates and all earlier pending revisions. Historical pending human-review statements above are superseded. This does not validate unimplemented teammate integration or establish measured performance. See [review record](../UI-REVIEW.md).

## Through-pillar correction — 2026-11-08

Following that approval, the user requested nameplates remain visible through pillars. Remove only the nameplate anchor's occlusion test; retain world-model hover picking, camera collision and simulation line-of-sight checks. Nameplate buttons already submit identity-based target/focus intent independently of world picking. A Rust regression places a unit behind a pillar and verifies world-hover picking remains blocked while its overhead nameplate stays projected at the correct position. Existing offscreen/behind-camera, rendered-movement and viewport tests remain. Human rendered review of this new correction is pending; the earlier UI approval is not retroactive.
