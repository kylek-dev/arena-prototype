# ADR 0039: Render warmup behind the loading screen

Date: 2026-11-14. **Status: accepted for local integration**, at the user's
request to pre-render the models behind the loading screen so play does not
start with choppy frames. Human browser review is pending.

## Problem

The loading overlay hid after the character asset reported ready and 10 frames
had passed. Bevy uploads meshes/images and compiles render pipelines lazily,
for what the camera actually draws. On the web each new shader variant and
large texture upload can stall a frame. That work therefore landed in the
first seconds of play: the newly visible dressed character, scenery outside the
first view, and the Pulse beam on first use. The draft startup gate from the
ADR 0035 work existed but was not wired in.

## Decision

Wire in `crates/arena-client/src/startup.rs` (presentation only):

- **Assets first.** Wait for the character to finish (ready, failed or
  placeholder), as before. Arena textures and sky are also waited for, but for
  at most 60 s, because they are decoration.
- **Render behind the overlay.** The overlay stays up while Bevy keeps
  rendering underneath. During warmup the camera holds each of four 90°
  headings around the player for 6 frames and then 10 frames at the real view.
  The eye snaps instead of easing, so no camera swing shows after loading. The
  Pulse beam is drawn during warmup.
- **Readiness from the render world.** During extraction the gate counts
  pipelines still waiting to compile, spawned meshes without GPU copies,
  material/sky textures without GPU images, and the total pipeline count.
  Readiness needs a full sweep plus the home view with nothing pending, no new
  pipeline and at least 0.35 s of fresh frames. Any new pipeline or upload
  restarts the sweep. Once ready, the gate never re-locks.
- **Never trap the player.** Warmup gives up after 15 s and reports
  `timedOut: true`. Pipeline errors do not block; Bevy still logs them.
- **Bridge.** The snapshot gains an optional, validated
  `startup: { phase, ready, pending, timedOut }`. The loading screen waits for
  `startup.ready` and shows "Preparing the arena…" once assets are in. Older
  clients without the field keep the previous rule.

The draft's unused in-memory character preloading (an extra asset source fed
by browser-fetched bytes) was not wired in and is removed. A copy of the draft
was kept outside the repository during the change.

## Verification

Rust tests cover waiting for the character and GPU uploads, the four-heading
sweep and home settle, restart on a new pipeline, ignoring stale render
reports, the warmup timeout, the scenery limit and never re-locking. Vitest
covers the loading screen staying up through warmup and the snapshot rejecting
inconsistent or unknown startup states. `./scripts/check.sh`, WASM Clippy with
warnings denied and the full web build passed locally.

No browser was launched, so neither the hitch improvement nor the added loading
time (expected well under a second when nothing new compiles) has been
observed. WebGL drivers may still defer some shader work to first use, and
effects that never appear during warmup can still compile on first sight.
Human review should check the first seconds after loading (turning the camera,
first Pulse, first jump/strafe), loading duration and console warnings.
