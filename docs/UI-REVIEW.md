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
