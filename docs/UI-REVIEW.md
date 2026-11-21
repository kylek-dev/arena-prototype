# Human UI review

## 2026-11-08 — verified and approved by the user

The user confirmed: “all UI has been verified for all pending UI changes that are waiting human approval (for other changes we made in the past too)”.

This closes the outstanding human UI/rendered-interaction approval gate for all implemented changes through world health nameplates (ADR 0022), including earlier revisions:

- React overlay, game menu, keyboard/mouse focus routing and remappable bindings.
- Unit frames, self targeting, target/focus interactions and unit/world tooltips.
- Action bars, cast errors, availability tints, held-key feedback and cooldown animation.
- Spellbook window/grid, captured-pointer spell placement, sizing, locking and canvas removal.
- Layout editing, movable windows, visibility/scaling, local profiles and persistence UI.
- Graphics settings, arena/camera/jump presentation, procedural/imported characters and animations.
- Cosmetic weapons, palettes, mounts, geometry refinements, visibility, glow and lighting corrections.
- Temporary lighting/color-grading controls and the expanded tone-mapping panel.
- Floating health-only nameplates, target/focus clicks and friendly/enemy visibility controls.

**Evidence:** the user's confirmation in this conversation, not an agent-run browser session. This dated approval supersedes earlier pending human UI-review statements in README, browser/frontend/architecture docs, ADRs 0007–0022 and the React UI implementation/verification handoff. Historical failed reviews and automated-check results remain historical facts; this approval applies to the current revisions, not rejected earlier implementations.

Browser/version, device/OS/GPU, viewport, individual scenario logs and console results were not supplied. Do not invent them or treat this as a measured performance benchmark, exhaustive compatibility/accessibility certification, hosted CI result, binary-asset license approval, or authorization to publish/merge. Unimplemented features (including real teammates) are not validated by this approval. Future UI changes still require their own human review.

## Subsequent changes

2026-11-08: following the successful frame-pacing fix, the user requested publication and confirmed “push these changes everything is good!”. This approves the current change set: 1v1/2v2/3v3 practice rosters and self-excluding team frames, small nameplate names, targeted-unit borders, compact browser/engine timing diagnostics and the walking/targeting pacing correction (ADRs 0025–0027 and the nameplate follow-ups). This supersedes their historical pending human-review notes in README, browser/architecture docs and decision records. The Windows Chrome observations are recorded in ADR 0027; approval does not establish exhaustive compatibility, GPU-presented FPS, a controlled performance benchmark or hosted CI results. Publication is authorized through a new branch and pull request, not a direct push to main or bypass of required reviews/checks.

2026-11-08: after reproducing walking/targeting choppiness in both development and production, the user tested the browser frame-pacing correction (ADR 0027) in the production 3v3 scene and confirmed: “yep that worked its great now”. This completes the human acceptance check for that choppiness correction. The Windows Chrome MCP observations and automated checks are recorded separately in ADR 0027; this feedback is subjective acceptance, not a controlled performance benchmark, exhaustive scenario review, approval of unrelated pending changes, or authorization to publish/merge. Historical pending subjective-review notes for ADR 0027 are superseded by this confirmation.

2026-11-08: the user subsequently requested nameplates remain visible through pillars. That correction follows the approval above and requires its own human rendered review. World-hover occlusion and combat line of sight remain unchanged. See [ADR 0022](decisions/0022-world-health-nameplates.md#through-pillar-correction--2026-11-08).

2026-11-08: the user then requested right-click focus on character models and explicitly approved the completed change (“approved”). The human UI approval gate for model focus, including shared model-click picking, is complete. This is user-reported approval, not an agent-run browser test; no additional browser/device or console details were supplied. See [ADR 0023](decisions/0023-world-model-focus.md). This approval does not authorize publication or merge.

2026-11-15: **pending human review.** The network match UI ([ADR 0044](decisions/0044-authoritative-match-server.md), Phase 5): playing a match from development join links, the match banner (connecting, waiting for players, reconnecting, match over, rejoin), greyed "Disconnected" unit frames and nameplates, "Opponent N" names, the menu's Leave match / Rejoin last match controls with practice-only controls hidden during a match, and the network rows in the diagnostics overlay. Automated DOM tests, frontend checks, WASM Clippy and the complete web build passed locally; no browser was launched and no rendered or networked pass is claimed.

2026-11-16: **pending human review.** The home screen and Play online panel ([ADR 0046](decisions/0046-queues-parties-and-home-screen.md)):

- **Home screen.** Customize character, Keybinds, Spells, Practice and Play online.
- **Parties.** Invite friends (Copy invite link), join with a code, leave a party, and members waiting for the leader.
- **Queues.** The five queue buttons, with the ones a party cannot use disabled and their reasons, and the search banner with time, players queued and Cancel.
- **Messages.** Refusals and ended searches explained in words, and entering the arena by itself when a match is found from the home screen.

Automated DOM tests and frontend checks passed locally. No browser was launched, and no rendered or networked pass is claimed.

2026-11-16: the user tried the queue flow locally and reported "this worked". Browser, OS, and which modes and party flows were exercised were not stated, so this does not cover a cross-browser review or every control listed above (issue `network-ui-review`).

2026-11-17: **pending human review.** The queue bar and match-ready window ([ADR 0047](decisions/0047-accepting-matches-and-queue-status.md)):

- **Queue bar.** At the top on the home screen and in the arena: queue name, live timer, expected wait or "not enough recent matches", players in the queue, Cancel, and the requeued and left-the-queue messages.
- **Match-ready window.** Title, 15-second countdown and bar, acceptance markers, Accept (focused) and Decline, the "waiting for N more" state, and the page title countdown in a background tab.
- **Play online panel.** Three queue buttons (1v1, 2v2, 3v3).

Automated DOM tests, frontend checks and the frontend build passed locally. No browser was launched, and no rendered or networked pass is claimed.

2026-11-21: **pending human review.** Casting & resource combat readability ([ADR 0048](decisions/0048-server-authoritative-casting-and-resources.md)):

- **Unit frames.** Live authoritative resource bars (replacing the placeholder meter when resource data is present), unit-frame cast bars showing spell name and remaining seconds, and a "Locked out" status badge while silenced/interrupted.
- **World nameplates.** Slim overhead cast bar beneath the health bar during timed casts and a compact "Locked" tag while locked out.
- **3D casting & multi-bolt VFX.** Raised-arms procedural channeling pose (`animation.rs`), authored `Clip::Cast` playback while channeling (`character_animation.rs`), and concurrent multi-bolt lightning rendering (`lightning.rs`) with accurate hit feedback text.
- **Multi-spell action bar & Spellbook.** Default 4-spell kit on slots 1–4 (`1` Pulse, `2` Rift Bolt, `3` Quell, `4` Mend) with additive migration for existing profiles, all 4 spells draggable in the Spellbook, per-spell cooldown sweeps & tooltips, and auto-self-casting Mend when targeting an enemy or nobody.

Automated DOM tests, Rust unit tests, frontend checks, WASM Clippy, and the complete web build passed locally. No browser was launched by the agent, and no rendered pass is claimed.
