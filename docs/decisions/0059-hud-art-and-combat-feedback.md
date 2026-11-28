# ADR 0059: HUD art, obsidian interface and combat feedback

Date: 2026-11-28. **Status: proposed (implemented, at the user's direction
through a day of iteration; human rendered review pending).**

Builds on [ADR 0042](0042-closed-color-palette.md) (the closed palette),
[ADR 0048](0048-server-authoritative-casting-and-resources.md)
(authoritative casting) and
[ADR 0052](0052-art-scope-and-quality-bar.md) (art scope and quality bar).

## Context

The HUD drew its unit frames, bars and windows from flat CSS in the blue-black
night colours. The user wanted a richer, painted look (black stone and antique
gold, no brown, no blue) and clearer combat feedback: what a cast did, when a
unit was just hit or healed, who is targeted. Cast endings were not visible to
the page at all: the client turned only the player's own interrupts into a
feedback line.

## Decision

- **Painted frames, live bars on top.** Unit frames, cast bars and windows wear
  AI-generated art prepared by `scripts/prepare_ui_art.py` (provenance in
  `assets/README.md`; masters stay out of Git). Unit frames lay the art behind
  and draw the live bars on top, sized to its painted troughs (generated CSS
  gives their places); target, focus and opponent frames mirror it. A frame
  can hide its portrait (a per-frame layout setting). Cast bars and windows use
  their art as CSS 9-slice borders, so ornate ends and corners keep their shape
  at any size.
- **Cast endings come from the rules.** The client records every unit's
  `CastCompleted` and `CastInterrupted` (with its reason) for about a second and
  publishes them as `castEnds` in the HUD snapshot; the page plays one ending
  per id: completed, cancelled (the caster stopped, moved, lost range, sight or
  its target) or interrupted (an interrupt or lockout). Instant spells have no
  cast bar, though an instant interrupt reports completing, so their endings are
  skipped (a regression the user found: one crashed the HUD). Nothing is
  inferred from snapshots, and nothing changes the simulation or the protocol.
- **Health reactions are presentation.** A loss leaves a draining trail, a heal
  grows the bar with a wave, a living unit at or below 25% pulses, and a spark
  rides each partly filled bar, from health changes the page already receives.
  Frames and nameplates share them.
- **Targeting.** The 3D client draws ground brackets under the target (ember
  for a foe, teal for a friend); the targeted team or opponent frame glows and
  the targeted nameplate has an orbiting light.
- **Obsidian interface.** The palette gains an obsidian family (near-black glass
  with a faint violet sheen) and Radix gray and gold scales drawn from it: the
  interface's neutrals are obsidian and its accent gold. The HUD's own uses of
  the night colours take obsidian inside the interface only; the 3D scene keeps
  night. Smaller additions: ash (a neutral bar track), ochre (the cast fill) and
  cerulean (now unused, from the admin tag, which the user had removed).
- **Interface scale.** One setting (60–160%) multiplies every HUD frame's own
  scale; nameplates, dock windows and Radix windows follow it.

## Consequences

- The painted art is used as generated, not remapped to palette swatches; ADR
  0042's rule for painted textures still applies before it ships.
- `castEnds` is additive in the client-to-page snapshot; older clients omit it.
- The style lab (`web/dev/unit-frames.html`) predates the painted frames; its
  copied CSS no longer matches them.
- Rendered review is the user's, in the browser (`docs/BROWSER.md`).
