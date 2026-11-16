Make the server authoritative for cast times, the global cooldown, per-spell cooldowns, resources and interrupts, completing the original server-authority goal (ADR 0044: wire them in after the network duel).

`crates/arena-sim/src/casting.rs`, `combat_state.rs`, `spells.rs`, `classes.rs`, `specs.rs` and `talents.rs` exist but are not compiled (not declared in `lib.rs`), so nothing tests them.

Acceptance criteria:
- Compile and test those modules; remove the duplicate `TICKS_PER_SECOND` in `spells.rs`.
- Integrate casting with the arena's per-actor input slots and phased resolution: starting, completing and interrupting casts, GCD, cooldowns and resource costs judged by the rules, with range and line of sight re-checked at completion.
- Extend the protocol (snapshot fighter state, cast/interrupt events) and bump `PROTOCOL_VERSION`/`RULES_VERSION` as their rules require.
- Show casts and real resources in the HUD (resources are labelled placeholders today); decide what, if anything, the client predicts.
- Headless tests for each mechanic's boundaries and exploits, and bot runs showing no state divergence.
