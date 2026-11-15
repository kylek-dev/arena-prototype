# ADR 0025: Team frames and practice rosters

Date: 2026-11-08. **Status: accepted**, following the user's request for left-side team frames (excluding self) and 1v1/2v2/3v3 practice options. Human rendered review is pending.

## Decision

Escape → Practice mode chooses equal-sized local teams, starting a fresh round. Default is 1v1; R/menu reset preserves the current size. Non-player actors remain stationary and do not attack; this is not multiplayer or a bot playtest.

The planar simulation owns a bounded 2–6 actor roster, alternating friendly/enemy actors: player 0, Opponent1 1, Team1 2, Opponent2 3, Team2 4, Opponent3 5. Friendly fire and commands by defeated actors are rejected. A round finishes only after all opponents on one side are defeated. Caller-supplied command order, integer ticks, collision, range and cooldowns are unchanged. The original 1D headless duel is unchanged.

Snapshots append team size, selected actor and a validated roster. Legacy player/target fields retain player/Opponent1 meaning; selected-target distance and attack status use the actual selection. Local stable IDs are not network identities. Input indices 17/18 append one-shot actor selection (-1 absent) and practice size (0 absent), preserving indices 0–16. Invalid/out-of-roster selections are ignored. Reset/mode initialization precedes selection, then clear and world picking. Older snapshots retain the single-opponent fallback.

Precreated procedural actors hide when outside the active roster and are excluded from picking/projections. Every active unit supports frame/model/nameplate targeting and focus. Tab cycles living opponents only. Nameplates and outgoing combat text use actual roster tags. Team frames use the same bars and interactions as opponent frames; default placement mirrors opponents on the left, with independent saved layout and additive profile migration. Self is excluded from both the team roster and rendered team list. Layout mode previews two teammates and three opponents.

## Verification

Rules tests cover all sizes, invalid sizes, friendly fire, defeated actors, elimination and deterministic replay. Frontend tests cover counts/no self, slot identity, target/focus, Tab, mode/reset intent, snapshot validation and old-profile migration. Existing input ABI expectations include the appended fields. Run the repository checks, WASM Clippy and complete web build.

Local verification on 2026-11-08: `./scripts/check.sh` passed (13 rules tests, 66 client tests, 9 asset-adapter tests, 28 Node input tests and 157 Vitest tests), WASM Clippy with warnings denied passed, `./scripts/build-web.sh` passed including static-resource/MIME checks, and `cargo run -p arena-headless` completed the original scenario. The web build used the procedural character fallback because BaseMale was absent. No browser was launched; no rendered or performance pass is claimed.

Manual review required: switch 1v1 → 2v2 → 3v3 → 1v1; verify 0/1/2 team frames and 1/2/3 opponents, left/right placement, no self, health updates, target/focus from every frame/model/nameplate, friendly/enemy nameplate toggles, no friendly damage, all-opponent elimination, R preserving size, clearing old focus/selection/combat text and movement capture on mode change. Check layout dragging/scale/visibility, existing profile positions, 800×600 bounds and browser console. Automated checks do not establish rendered quality or performance.
