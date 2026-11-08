# ADR 0024: Outgoing floating combat text and nameplate notices

Date: 2026-11-08. **Status: accepted**, following the user's request. Human rendered review is pending.

## Decision

Show damage numbers in gold and healing numbers in green with a plus sign, rising and fading above the recipient for 1.2 seconds. Only local-player outgoing health changes qualify; never display other actors' contributions or text on the local player, including self-healing. Selection and friendly/enemy nameplate preferences do not govern combat text.

The presentation adapter records actual `ArenaEvent::Hit` actor, recipient and applied damage, not attack input, cooldown edges or inferred health differences. Its bounded 32-entry queue accepts signed health changes (negative damage, positive healing), filters actor/recipient identity before queuing, ages with presentation time and clears on practice reset. Healing presentation is supported and tested, but no healing ability or simulation event exists yet; a future authoritative healing event must supply its source, recipient and applied amount through the same filter. No combat rules change.

An optional `combatText` snapshot field carries active IDs, source/recipient local tags, signed amount, normalized overhead projection and lifetime progress. Projection uses interpolated fighter transforms after camera updates, rises above the nameplate anchor, and omits offscreen/behind-camera entries. Like nameplates, this overlay is visible through geometry; combat line of sight remains unchanged. The existing render-rate scene store delivers it before ordinary HUD throttling. The bridge checks bounds, integers, unique IDs and queue limits; the frontend also filters nonlocal sources and self recipients. Older engines without the field show nothing. The text is pointer-transparent and has no gameplay authority. Local practice actor IDs/tags are not an authenticated multiplayer protocol.

Explicit friendly/enemy nameplate changes, through bindings or Settings, produce a yellow enabled/disabled notice in the existing top-center combat-error area. Each notice expires after 2.5 seconds; another toggle restarts its lifetime. Red cast errors retain their independent message/timer and can coexist. Initial loading and profile hydration do not generate notices. Reset clears notices.

## Verification

`./scripts/check.sh` passed locally: 65 client tests, 12 simulation tests, 9 asset-adapter tests, 28 Node input tests and 144 Vitest tests. WASM Clippy with warnings denied and the complete `./scripts/build-web.sh` passed, including static-resource/MIME checks. The build used the procedural character fallback because BaseMale was not installed. New regressions cover source/self filtering for damage and healing, zero changes, bounded bursts, expiry/reset, successful hit versus range/cooldown rejection, render-rate positions/fade, missing optional fields, malformed projections and yellow notice replacement/expiry alongside red errors.

Human browser review is pending. Check Pulse numbers on the opponent (including the final hit), no numbers for rejected casts or self-targeting, successive numbers, camera/movement/zoom/DPI tracking, nameplate toggles independent of numbers, pointer pass-through and reset. Check yellow V/Shift+V and Settings notices, repeated toggles, their expiry, simultaneous red errors and browser console output. Healing's visual integration awaits a real authoritative healing mechanic. No rendered or performance pass is claimed.
