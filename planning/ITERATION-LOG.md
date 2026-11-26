# Autonomous Iteration Log

Append-only record of iterations run via `.agents/skills/iterate-game/SKILL.md`.
Each fresh agent session reads `## Codebase Patterns` and the latest entries here
before editing code so context compounds on disk instead of decaying in long chats.

## Codebase Patterns

Reusable repo invariants, canonical helpers, and CI traps promoted across iterations:

- **Simulation purity:** `arena-sim` must stay pure, fixed-tick, integer-based, and deterministic (no graphics, OS clocks, network I/O, or runtime LLM calls).
- **Static web pages:** `web/public/` is generated and gitignored (`scripts/build-web.sh`); add tracked pages such as `web/privacy.html` as Vite `rollupOptions.input` entries. `vite.config.ts` is type-checked without Node types, so it cannot import `node:*`.
- **CI `browser` vs `scripts/check.sh`:** `./scripts/check.sh` checks `typecheck`, `lint`, `format:check`, and `npm test`, while `.github/workflows/ci.yml`'s `browser` job also runs `npm run build`. Run `.agents/skills/iterate-game/scripts/run-critics.sh --auto` (or `--web-build`) whenever `web/` changes.
- **CI `client-build` trigger:** `.github/workflows/client-build.yml` runs full native + WASM builds only when `crates/**`, `Cargo.toml`, `Cargo.lock`, `rust-toolchain*`, `.cargo/**`, `assets/**`, `scripts/**`, or `.github/workflows/**` change; doc/skill/planning-only PRs skip them cleanly.
- **Python unit tests in `check.sh`:** `./scripts/check.sh` automatically runs `python3 -m unittest discover -s tests/unit -p 'test_*.py'`, so Python harness/hook tests belong in `tests/unit/test_*.py`.
- **Closed color palette:** Non-VFX colors must come from `docs/palette.json` and pass `node scripts/palette.mjs --check`. The literal check flags `#` plus 3–8 hex digits anywhere in `web/src`, comments included, so write handles as `Name#number`, not `Name#1234`.
- **Canonical state hashing:** Always reuse `Arena::hash_state` instead of duplicating state-hash logic in `arena-client`.
- **Canonical fighter validation & prediction boundaries:** Reuse `Combatant::is_valid(roster_len)` across `Arena::restore` and `ServerMessage::decode`, and strip non-predicted combat state (`active_cast`) inside `Predictor::reconcile` so local movement replay never triggers phantom cast completions, kills, or movement freezes.
- **Wire & snapshot boundaries:** Validate protocol limits at decode time (for example, `Welcome.round_ticks` in `1..=MAX_ROUND_TICKS`, `Welcome` team/seat/tick-rate bounds, `Snapshot` even `2..=MAX_FIGHTERS` roster and event actor/result bounds), use saturating tick/cooldown math near `u64::MAX`, and validate JS bridge integers with `Number.isSafeInteger` in `adaptSnapshot`.
- **WebSocket control-frame rate limiting:** Count WebSocket control `Ping`/`Pong` frames against `RateWindow` both before and after `Hello` on `/play` and `/api/queue` so control-frame floods cannot bypass ingress rate limits.
- **Shared bot policy & combat telemetry:** Reuse `arena_headless::policy::{Rng, decide}` and `arena_headless::telemetry::{OutcomeSummary, DurationSummary, CombatMetrics}` across `arena-headless` and `arena-bot`, record server running-tick compute time 1-to-1 with `arena_ticks_total` via `Metrics::record_running_tick` (keeping `tick_max_us` cache-line isolated), and track per-tick public event recording separately from hash presence so empty-events reconnect `attach()` snapshots never drop combat events.
- **Netcode measurement accuracy (`arena-bot` & `arena-protocol`):** Compute steady-state active-combat snapshot bitrates against active stream player-seconds (`snapshots_30hz / tick_rate`) rather than wall-clock `players * seconds` (which includes waiting/linger/reconnect idle gaps), and account for `arena-sim::casting`'s validation order (`resolve_target` → `Cooldown`/`GlobalCooldown` → `check_target_geometry`) and post-Phase-4c `Hit` events so in-flight cooldown spam never dilutes targeting-fairness denominators and killing blows never register as target-dead disagreements.
- **Bounded lag-comp & adaptive interp boundaries:** Keep `Arena`'s inline `PositionHistory` (`MAX_LAG_COMP_TICKS = 6`) outside `Arena::state_hash` and `PartialEq` (preserving `Arena::restore == original`), require simultaneous range+LoS at the same historical tick on cast starts only (keeping target liveness and Phase 4a completions strictly present-tick), enforce server-side monotonic `max_view_tick` clamped to `<= post_tick`, and gate `InterpDelay::observe_underrun` and `last_view_tick` updates on `Phase::Playing` + tick-rewind resets.
- **End-of-match scoreboard & rematch clearing:** Place seat-independent final totals (`Snapshot.scoreboard`, excluded from `Arena::state_hash`) between `fighters` and `events` so `EncodedSnapshotBody` encodes it once per tick; credit killing blows from pre-tick health against ordered `Hit` events (1 KB per death), preserve per-seat scores across disconnects/reconnects, and clear `arena-client`'s snapshot buffer on any `Welcome` arriving after `newest.result.is_some()` so finished scoreboards never bleed across same-seat rematches.
- **Top-center HUD stacking & exclusive UI suppression:** Coordinate centered top overlays across three non-overlapping tiers (`RoundClock` at `12px`, `CombatErrors` at `max(50px, 7%)`, mutually exclusive `MatchStatus` / `.round-result` at `max(84px, 12%)`) with shared `isQueueBarVisible` `below-queue` offsets (`108px` / `146px` / `180px`) so 800×600 viewports and 3-line `"requeued"` `QueueBar` states never collide; suppress non-modal combat HUD widgets during exclusive modes (`layout`, `quick`, `capture`, `menu`, `settings`, fatal `error`) and gate `MatchStatus` outcome retention on `status.phase` (`!= null && != "connecting"`) so fast rematches never leak prior outcomes.
- **Post-match scoreboard modal & fast-rematch lifecycle (`web/src/state/scoreboard.ts`):** Centralize capture/clear/dismiss logic in `syncPostMatchScoreboard(snapshot, linkState)` (used by both `start.ts` and `<Scoreboard />`), clearing on `connecting`/`waiting`/`playing` and only capturing when `status.phase === "finished"` or `MatchOver` so stale Match 1 finished snapshots never pop back up during Match 2's `waiting` window; close transient combat modals/editors (`setPanel(null)`, `pendingBinding`, `customizing`) whenever the scoreboard opens or `returnToHome()` runs.
- **Per-fighter kits & appearance relay (`PROTOCOL_VERSION = 6`):** Share kit GLBs and `AnimationGraph`s via `character::KitCache` and deduplicate overridden `StandardMaterial`s per `(KitInstance, GltfMaterialName)` storing the original base material alongside the handle; protect both `Welcome` (variant 0) and `Appearances` (variant 4) from `MAX_QUEUED` eviction in `web/src/runtime/network.ts` and clear `remoteAppearances` synchronously on `end()` and new socket generations.
- **Private custom lobbies & top-bar coordination:** Keep custom lobby state inside `Matchmaker` (`matchmaker_custom.rs`) sharing `/api/queue`'s `OpenPerAddress` + `RateWindow` and enforcing `door.creations` per-address rate limiting on `start_lobby`; coordinate top-center `.below-queue` spacing across `QueueBar`, `RoundClock`, `MatchStatus`, `CombatErrors`, and `Scoreboard` through the single canonical `useQueueBarVisible()` hook (suppressing the minimized lobby pill behind `Scoreboard` and active matches).
- **Live spectating & anti-ghosting boundaries (`PROTOCOL_VERSION = 7`, ADR 0050):** Separate player and watch-only credentials structurally via signed `ticket::Role::{Player { seat }, Spectator}` (`allow_spectators` enforced in both front door and match server); always drain `spectator_queue` in both `tick()` and `tick_encoded()` even when temporarily empty (and flush due frames before `Phase::Finished` close so `linger == 0` never drops the final scoreboard); refuse `Phase::Finished` spectator attachments unless a queued or finished snapshot exists; share one serialized `Arc<[u8]>` (`Bytes::from_owner`) across all spectators per tick; clear incompatible combat UI modes (`spellbook`/`layout`/`quick`) when entering spectating and hold `result[14]` while `Space` is active in free-cam without changing seated-player edge-triggered jumping.
- **Server/door logs & metrics (ADR 0054):** Log with `tracing` events carrying explicit `match_id = %logging::MatchId(id)`, `seat`, `conn_id`, `error_code` fields (never wrap the 30 Hz match task in a span), never log tickets/links/keys (`tests/logging.rs` captures a door-issued match with `--debug-json` and fails on any leak; wire payloads go under `payload`, not `message`, which collides with the event text in JSON), and add latency distributions with the lock-free `metrics::Histogram<N>` (`&'static` bounds, inclusive `le`) instead of new max/total pairs.
- **Control-plane traces (ADR 0054 Phase 2):** Create spans only with `crate::control_span!` (records `trace_id`/`span_id` for log↔trace links) on control-plane steps; give long-lived spans (`match`, `queue.search`) an explicit `parent: None`, use them only as parents (never `enter()` them across ticks or `.await`s), end every per-connection span on all exit paths (including `Closed::Replaced`, which sends no `Gone`), report anomalies through `anomaly::SharedThrottle` statics, and keep `logging::init` before the Tokio runtime (the OTLP HTTP client blocks).

## Entry Template

See [.agents/skills/iterate-game/references/pr-and-log-templates.md](../.agents/skills/iterate-game/references/pr-and-log-templates.md).

---

## 2026-11-21 — Autonomous iteration skill & safety harness
- **Branch / Commit:** `auto/iterate-game-skill`
- **Base / PR:** `main` (base of stacked PR with `auto/round-clock`)
- **Lane:** A (machine-verified tooling / harness)
- **Task:** Established and hardened `.agents/skills/iterate-game/` (progressive-disclosure `SKILL.md`, `references/review-rubric.md`, `references/pr-and-log-templates.md`, `scripts/orient.sh`, `scripts/run-critics.sh`), `.agents/hooks/` (`guard-commands.py`, `stop-check.py`), `tests/unit/test_agent_hooks.py`, `planning/BACKLOG.md`, `planning/ITERATION-LOG.md`, and `docs/research/autonomous-iteration-skills.md` based on proven public GitHub agent skills.
- **Checks Observed:**
  - `./scripts/check.sh`: pass (fmt, clippy `-D warnings`, Rust workspace tests + 147 `arena-client --lib` tests with 1 pre-existing ignored, 52 Python unit tests including 13 hook test methods, `node --check web/input.js`, `scripts/palette.mjs --check`, `tsc --noEmit`, Biome lint/format, 32 Vitest files / 242 tests).
  - `cargo run -p arena-headless --locked`: pass (`winner=Some(0)` at tick 90).
  - `python3 scripts/test-client-build-needed.py`: pass (5 tests).
  - `.agents/skills/iterate-game/scripts/orient.sh` & `bash -n`: pass.
- **Fresh-Eyes Review:** Addressed all Critical and Important findings from read-only two-stage review: replaced regex-based `git push`/`reset`/`clean` guards in `guard-commands.py` with full shell/token parsing (blocking global `-C`/`-c`, subshells, comments with quotes, `/usr/bin/git`, `-S -n`/`-u -n`, `gh pr -R ... merge --admin=true`, and URL-remote pushes on `main`, while eliminating false positives on `auto/main-menu` and commit messages); updated `stop-check.py` to resolve repo root via `git rev-parse --show-toplevel` and use `-uall -z` NUL records for subdirs/spaces/untracked dirs; aligned `run-critics.sh --auto` path triggers with `ci.yml` and `scripts/client-build-needed.sh` with strict `--base` validation and SIGPIPE-safe here-strings; expanded `tests/unit/test_agent_hooks.py` regression coverage.
- **Learnings:** Putting literal merge-conflict markers inside a unit-test file trips `stop-check.py`; construct test fixtures programmatically and match conflict markers at line starts (`^<{7} |^>{7} `) to avoid false positives on Setext markdown headings.
- **Next Recommended Slice:** Pick the top Lane A item in `planning/BACKLOG.md` (**Round clock & time-limit result**) or run a Critic Audit pass.

## 2026-11-21 — Authoritative round clock & time-limit draw result (Lane A)
- **Branch / Commit:** `auto/round-clock` (`c6ddc7a`)
- **Base / PR:** `auto/iterate-game-skill` (stacked PR on top of `auto/iterate-game-skill`)
- **Lane:** A (machine-verified)
- **Task:** [round-clock](issues/round-clock.md) — Lane A slice from [planning/BACKLOG.md](BACKLOG.md)
- **Changes:**
  - `arena-sim`: added `RoundResult::TimeLimitDraw` + `is_draw()`, explicit per-round tick limits (`with_round_limit` / `set_round_limit` bounded to `1..=MAX_ROUND_TICKS`), `max_round_ticks()` / `remaining_ticks()`, distinct `state_hash` encoding (`[2, 1]`), and saturating tick/cooldown arithmetic near `u64::MAX`.
  - `arena-protocol` (bumped to `PROTOCOL_VERSION = 2`, `RULES_VERSION = 3`): added `Welcome.round_ticks` (decode-validated to `1..=MAX_ROUND_TICKS`), `WireResult::TimeLimitDraw`, `Predictor::set_round_limit`, and updated `docs/PROTOCOL.md` + Rust/TS golden frames.
  - `arena-server` & `arena-bot`: added `Config.round_ticks` (validated at `start()`), CLI `--round-seconds` (`1..=1200`), `arena_matches_finished_total{result=...}` Prometheus metrics, session + live WebSocket time-limit draw tests, and strict `arena-bot::bot::result_matches` verification across wins, mutual-KO draws, and time-limit draws.
  - `arena-client` & `web`: removed duplicate `snapshot_state_hash` in favor of `Arena::hash_state`, translated `View.result` into local team parity, exposed `maxRoundTicks` / `tickRate` / `roundResult` on engine snapshots with `adaptSnapshot` validation, and added pure TS helpers in `web/src/domain/round-clock.ts` (`remainingRoundSeconds`, `formatRoundClock`, `isRoundClockWarning`, `roundResultLabel`).
- **Checks Observed:**
  - `.agents/skills/iterate-game/scripts/run-critics.sh --auto --base auto/iterate-game-skill` (`smoke=true web_build=true wasm=true`): pass
    - `./scripts/check.sh`: pass (fmt, clippy `-D warnings`, all workspace Rust tests + 147 `arena-client --lib` tests with 1 pre-existing ignored, 52 Python unit tests, `node --check web/input.js`, `scripts/palette.mjs --check`, `tsc --noEmit`, Biome lint/format, 33 Vitest files / 247 tests).
    - `cargo run -p arena-headless --locked`: pass (`winner=Some(0)` at tick 90).
    - `cargo build -p arena-server -p arena-bot --locked && ./scripts/server-smoke.sh`: pass (4×2v2 = 16 bots, 1143 cross-seat hash checks, 0 mismatches, 0 wrong round results, 17/17 rejoins, clean SIGTERM drain).
    - `npm run build`: pass (358 modules transformed).
    - `cargo clippy -p arena-client --target wasm32-unknown-unknown --locked -- -D warnings`: pass.
  - `./scripts/build-web.sh`: pass (WASM + Vite production build, 55 static resources; no browser launched).
- **Fresh-Eyes Review:** Addressed all 7 findings (3 moderate, 4 minor) from read-only review: preserved `TimeLimitDraw` feedback across subsequent local practice ticks, wired `Predictor::set_round_limit` across custom round limits with a boundary test, moved `round_ticks` off `session::Limits` onto `Config` with startup validation, saturated attack cooldown addition at `u64::MAX`, tightened `adaptSnapshot` to `Number.isSafeInteger` + `needsImmediateDelivery` on `maxRoundTicks`, updated `OverLimit` docs, and added client `Session::receive` negative tests for invalid `Welcome.round_ticks`.
- **Learnings:** Promoted `Arena::hash_state` reuse and protocol/bridge boundary validation patterns into `## Codebase Patterns`.
- **Next Recommended Slice:** Either the Lane B follow-up (**Round clock HUD timer & time-limit result banner**) or the next Lane A item (**Server-authoritative casting, cooldowns, resources & interrupts**).

## 2026-11-21 — Server-authoritative casting, cooldowns, resources & interrupts (Lane A)
- **Branch / Commit:** `auto/server-casting-resources`
- **Base / PR:** `main` (standalone PR)
- **Lane:** A (machine-verified)
- **Task:** [server-casting-resources](issues/server-casting-resources.md) — Lane A slice from [planning/BACKLOG.md](BACKLOG.md), documented in [ADR 0048](../docs/decisions/0048-server-authoritative-casting-and-resources.md)
- **Changes:**
  - `arena-sim`: compiled and unit-tested `casting`, `combat_state`, `talents`, `spells`, `classes`, and `specs`; removed the duplicate `TICKS_PER_SECOND` from `spells.rs`; replaced `HashMap` with deterministic `BTreeMap` in `combat_state.rs` and `casting.rs`; defined a 4-spell kit (`Pulse`, `RiftBolt`, `Quell`, `Mend`) with `MAX_RESOURCE = 100` and deterministic passive regen (`+1` every 3 ticks); integrated casting, starter-haste GCD (29 ticks via `formulas::calculate_gcd_ticks`), per-spell cooldowns, off-GCD interrupts + 60-tick lockout, movement/cancel fake-casting, healing (`effects::apply_heal`), and completion-tick range/LoS/target-life re-checks into `Arena::advance`'s order-independent phased resolution; expanded `Combatant` (kept `Copy` with canonical `Combatant::is_valid`) and `Arena::hash_state`.
  - `arena-protocol` (bumped to `PROTOCOL_VERSION = 3`, `RULES_VERSION = 4`): replaced `InputFrame.attack` with `WireAction` (`None` / `Cast` / `CancelCast`), extended `FighterState` with resource/GCD/lockout/cooldowns/active_cast, added `CastStarted` / `CastCompleted` / `CastInterrupted` / `Healed` events and new rejections, enforced strict decode validation via `Combatant::is_valid`, stripped `active_cast` in `Predictor::reconcile` so casts are never client-predicted, and updated `docs/PROTOCOL.md` + Rust/TS golden frames.
  - `arena-server` & `arena-bot`: updated session snapshot encoding and added session unit + live WebSocket E2E tests for timed casts, movement cancels, off-GCD interrupts, cast lockout, per-seat `Silenced` rejections, and reconnect preservation; upgraded `arena-bot` AI to cast `RiftBolt` and interrupt casting enemies with `Quell`.
  - `arena-client` & `web`: translated cast/interrupt/heal events and local cast targets in `network.rs`, kept combat fields server-owned when merging movement prediction in `network_receive`, updated `character_animation.rs` to detect Pulse cooldown edges, published authoritative `maxResource`, `resource`, `gcdRemaining`, `lockoutRemaining`, and `cast` on engine snapshots with strict `adaptSnapshot` validation, and added pure TS helpers in `web/src/domain/casting.ts`; queued HUD cast bars and real resource meters in `planning/BACKLOG.md` under Lane B.
- **Checks Observed:**
  - `.agents/skills/iterate-game/scripts/run-critics.sh --auto --base main` (`smoke=true web_build=true wasm=true`): pass
    - `./scripts/check.sh`: pass (fmt, clippy `-D warnings`, 168 workspace Rust tests + 148 `arena-client --lib` tests with 1 pre-existing ignored, 52 Python unit tests, `node --check web/input.js`, `scripts/palette.mjs --check`, `tsc --noEmit`, Biome lint/format, 34 Vitest files / 250 tests).
    - `cargo run -p arena-headless --locked`: pass (`winner=Some(0)` at tick 90).
    - `cargo build -p arena-server -p arena-bot --locked && ./scripts/server-smoke.sh`: pass (4×2v2 = 16 bots, 1143 cross-seat hash checks, 0 mismatches, 0 wrong round results, 17/17 rejoins, 498 ticks, 0 overruns, slowest tick 50 µs, clean drain).
    - `npm run build`: pass (359 modules transformed).
    - `cargo clippy -p arena-client --target wasm32-unknown-unknown --locked -- -D warnings`: pass.
  - `./scripts/build-web.sh`: pass (WASM + Vite production build, 55 static resources; no browser launched).
- **Fresh-Eyes Review:** Addressed all 3 Important and 4 Minor findings from read-only two-stage review: stripped `active_cast` inside `Predictor::reconcile` (with regression test) so local movement prediction can never trigger phantom cast completions/kills/movement freezes; extracted shared `Combatant::is_valid` across `arena-sim` and `arena-protocol` and restricted `CastStarted` / `isValidActiveCast` to timed spells; deferred Phase 4a completion effect queuing until `still_casting_after_4a` is finalized; added 4 combat boundary tests (lethal damage + simultaneous heal, target death mid-cast, same-tick cast start + Quell whiff, zero-vector move vs jump/airborne cast rules); fixed `arena-bot` comment and `i64` coordinate widening; updated `docs/PROTOCOL.md` wording; queued Lane B HUD follow-up in `planning/BACKLOG.md`.
- **Learnings:** Promoted canonical `Combatant::is_valid` reuse and stripping non-predicted `active_cast` state in `Predictor::reconcile` into `## Codebase Patterns`.
- **Playtest Follow-Up (2026-11-21 — Combat Readability & Bot Pacing on `auto/server-casting-resources`):**
  - Diagnosed user-reported perceived freeze/burst in 2v2 bot matches using live `/metrics` (`0` tick overruns, `133 µs` max tick, `0` dropped frames, `29.8–30.0` ticks/s pacing): the cause was zero visual feedback for timed casts/lockouts combined with all 3 bots freezing in lockstep at `12 m` to cast 1.5 s `RiftBolt`, freezing at `< 4.8 m` during cooldowns/2 s lockouts, and single-bolt `Practice.zap` overwriting concurrent hits.
  - Updated `crates/arena-bot/src/bot.rs` to stagger `RiftBolt` starts (`rng.unit() < 0.08`), strafe/orbit at close range instead of standing frozen during GCDs/cooldowns/lockouts, and keep moving while interrupting with off-GCD `Quell`.
  - Added raised-arms procedural channeling pose in `crates/arena-client/src/animation.rs`, authored `Clip::Cast` channeling playback in `crates/arena-client/src/character_animation.rs`, bounded multi-bolt lightning rendering (`MAX_ZAPS = 12`) in `crates/arena-client/src/lightning.rs`, and accurate damage-aware hit feedback in `crates/arena-client/src/lib.rs`.
  - Wired live authoritative resources, unit-frame cast bars + `"Locked out"` badges (`UnitFrame.tsx`), overhead nameplate cast bars + `"Locked"` tags (`Nameplates.tsx`), and immediate HUD delivery on target/roster cast & lockout transitions (`snapshot.ts`); appended `pending human review` entry to `docs/UI-REVIEW.md`.
  - Wired all 4 arena spells (`1` Pulse, `2` Rift Bolt, `3` Quell, `4` Mend) end-to-end through `web/input.js`, `arena-client` (`lib.rs`, `network.rs` `input_action`, per-spell snapshot cooldowns/statuses, auto-self-cast Mend), the default action bar + Spellbook + per-spell cooldown animations, and additive profile migration in `schema.ts`.
  - Fixed bot in-flight cast self-cancels (`pending_cast_ack`) and added a 14-tick `Quell` reaction delay so cast bars and mid-cast interrupts are clearly legible in combat.
  - Fresh verification: `.agents/skills/iterate-game/scripts/run-critics.sh --auto --base main` (all Rust/Python/palette/Biome/tsc checks, 34 Vitest files / 252 tests, `arena-headless`, `server-smoke.sh` 1142 hash checks / 0 mismatches / 42 µs max tick, `npm run build`, `wasm32` clippy `-D warnings`) + `./scripts/build-web.sh` (55 static resources; no browser launched).
- **Next Recommended Slice:** Either the next Lane A item (**Netcode stress & exploit hardening**) or one of the queued Lane B HUD slices (**Round clock HUD timer** / **Multi-spell player action bar & keybinds**).

## 2026-11-21 — Netcode stress & exploit hardening (Lane A)
- **Branch / Commit:** `auto/netcode-hardening`
- **Base / PR:** `main` (standalone PR)
- **Lane:** A (machine-verified)
- **Task:** [netcode-hardening](issues/netcode-hardening.md) — top Lane A slice from [planning/BACKLOG.md](BACKLOG.md)
- **Changes:**
  - `arena-protocol`: tightened `ServerMessage::decode` validation (`valid_welcome`, `valid_result`, `valid_snapshot`) to enforce `team_size` in `1..=MAX_TEAM`, `actor < 2 * team_size`, `tick_rate > 0`, even `2..=MAX_FIGHTERS` rosters, `WireResult::Won { team <= 1 }`, event actor indices `< count`, and timed-spell-only `CastInterrupted`, without changing the wire layout (`PROTOCOL_VERSION = 3`, `RULES_VERSION = 4`); added `MAX_FIGHTERS == MAX_TEAM * 2` compile-time guard, adversarial postcard varint/huge-length/discriminant tests, and a 24,000-round seeded dual client/server mutation fuzzer verifying canonical round-trip, `peek_hello_protocol`, `EncodedSnapshotBody`, JSON debug dump, and structural invariants.
  - `arena-server`: closed pre- and post-hello WebSocket control `Ping`/`Pong` rate-limit bypasses on both `/play` (`crates/arena-server/src/lib.rs`) and `/api/queue` (`crates/arena-server/src/door/queue.rs`), made `RateWindow::allow` saturating with edge-case unit tests, updated `docs/PROTOCOL.md`, and added live WebSocket tests for control-ping floods (on both `/play` and `/api/queue`), mid-match malformed-frame/invalid-intent/stale-seq isolation, and rapid double-reconnect seat takeovers with rejected old-socket inputs.
  - `arena-bot`: extended `Link` / `delay_line` with simulated TCP head-of-line packet-loss retransmit stalls (`--loss-pct`, `with_loss`) and relay clumping (`--clump-ms`, `with_clump`) while strictly preserving TCP order; wired browser-parity `Pacer` + `TickRate::observe_frame` batch handling into `bot::session` (with `--unpaced` opt-out); added `inputs_folded_total` / `inputs_starved_total` parsing and reporting; and expanded `tests/bot.rs` with 3v3 high-jitter, 2v2 TCP loss + relay clumping, and 2v2 aggressive reconnect + jitter + loss matrix tests.
- **Checks Observed:**
  - `.agents/skills/iterate-game/scripts/run-critics.sh --auto --base main` (`smoke=true web_build=false wasm=true`): pass
    - `./scripts/check.sh`: pass (fmt, clippy `-D warnings`, 180 workspace Rust tests + 150 `arena-client --lib` tests with 1 pre-existing ignored, 52 Python unit tests, `node --check web/input.js`, `scripts/palette.mjs --check`, `tsc --noEmit`, Biome lint/format, 34 Vitest files / 254 tests).
    - `cargo run -p arena-headless --locked`: pass (`winner=Some(0)` at tick 90).
    - `cargo build -p arena-server -p arena-bot --locked && ./scripts/server-smoke.sh`: pass (4×2v2 = 16 bots, 1141 cross-seat hash checks, 0 mismatches, 0 wrong round results, 17/17 rejoins, 496 ticks, 0 overruns, slowest tick 152 µs, 0 dropped frames, 0 overflows, 0 folded, clean drain).
    - `cargo clippy -p arena-client --target wasm32-unknown-unknown --locked -- -D warnings`: pass.
- **Fresh-Eyes Review:** Addressed all 3 Important and 4 Minor findings from read-only two-stage review: fixed `arena-bot` missed-tick pacing schedule to `now + step` so >1-tick stalls never double-fire back-to-back inputs; strengthened `rapid_double_reconnect_and_takeover_preserves_seat_and_rejects_old_socket_inputs` to actually send conflicting backward inputs from replaced sockets and assert `ack == 0` / unchanged `z`; strengthened `malformed_frames_and_invalid_intents_mid_match_isolate_offender_without_disrupting_peer` to assert `InvalidTarget`, peer rejection isolation, `inputs_stale >= 2`, and unchanged spawn position; added `RateWindow` saturating edge-case unit tests; replaced boolean `FirstError` in `door/queue.rs`; added saturating duration math + `--clump-ms <= 10000` validation; added `MAX_FIGHTERS` static assertion and full `Snapshot` invariant checks inside the fuzzer.
- **Learnings:** Promoted WebSocket control-frame (`Ping`/`Pong`) rate-limiting before and after `Hello` into `## Codebase Patterns`; queued the remaining measurement-gated candidates from `planning/issues/netcode-hardening.md` in `planning/BACKLOG.md`.
- **Next Recommended Slice:** **Headless combat telemetry** (next Lane A item in `planning/BACKLOG.md`) or **Round clock HUD timer & time-limit result banner** (top Lane B item).

## 2026-11-21 — Headless combat telemetry (Lane A)
- **Branch / Commit:** `auto/headless-combat-telemetry`
- **Base / PR:** `main` (standalone PR)
- **Lane:** A (machine-verified)
- **Task:** Headless combat telemetry — top Lane A item in [planning/BACKLOG.md](BACKLOG.md)
- **Changes:**
  - `arena-headless`: added library crate modules (`policy`, `telemetry`, `runner`) with deterministic `xorshift64*` `Rng` and canonical bot combat `decide`, serde-serializable `HeadlessReport` (`schema_version = 1`, `OutcomeSummary`, `DurationSummary`, `CombatMetrics`, `TickComputeSummary` with nanosecond sampling and unclamped `max_ns`, and optional `MatchRecord` entries), and zero-network `run_matches` timing `Arena::advance` directly while folding deterministic `combined_state_hash`; extended `arena-headless` CLI with `--simulate`, `--matches`, `--team-size`, `--round-seconds` / `--round-ticks`, `--seed`, `--map`, `--per-match`, and `--json` while preserving exact backwards-compatible scripted 1D `Duel` behavior with no arguments.
  - `arena-server`: added `Metrics::tick_total_us` (`arena_tick_microseconds_total`) co-located on `_pad_ticks` (keeping read-mostly `tick_max_us` isolated) and `Metrics::record_running_tick`, wired in `crates/arena-server/src/lib.rs` so cumulative tick compute time matches `arena_ticks_total` (`Phase::Running`) 1-to-1 without inflating from `Waiting`/`Finished` ticks.
  - `arena-bot`: delegated `bot::decide` and `link::Rng` to canonical `arena_headless::policy::{decide, Rng}`; extended `Shared` to deduplicate public combat events and finished-match outcomes/durations per `match_id` (tracking `public_events_recorded` independently of hash presence so reconnect `attach()` snapshots never drop tick events, and ignoring evicted ticks); added reconnect-safe per-seat `WireEvent::Rejected` recording (`record_seat_rejections`); extended `Report` with `outcomes`, `round_duration`, and `combat`, and extended `ServerMetrics` / `parse_metrics` with `tick_microseconds_total`, `tick_mean_microseconds`, and `matches_finished_{won,draw,time_limit_draw}`; documented CLI telemetry flags in `docs/DEVELOPMENT.md`.
- **Checks Observed:**
  - `.agents/skills/iterate-game/scripts/run-critics.sh --auto --base main` (`smoke=true web_build=false wasm=true`): pass
    - `./scripts/check.sh`: pass (fmt, clippy `-D warnings`, 198 workspace Rust tests + 150 `arena-client --lib` tests with 1 pre-existing ignored, 52 Python unit tests, `node --check web/input.js`, `scripts/palette.mjs --check`, `tsc --noEmit`, Biome lint/format, 34 Vitest files / 254 tests).
    - `cargo run -p arena-headless --locked`: pass (`winner=Some(0)` at tick 90).
    - `cargo build -p arena-server -p arena-bot --locked && ./scripts/server-smoke.sh`: pass (4×2v2 = 16 bots, 1141 cross-seat hash checks, 0 mismatches, 0 wrong round results, 17/17 rejoins, 495 ticks, 0 overruns, mean tick 12.7 µs, slowest tick 46 µs, 0 dropped frames, 0 overflows, 0 folded, clean drain).
    - `cargo clippy -p arena-client --target wasm32-unknown-unknown --locked -- -D warnings`: pass.
- **Fresh-Eyes Review:** Addressed all 3 Important and 4 Minor findings from read-only two-stage review: gated `arena_tick_microseconds_total` on `Phase::Running` via `Metrics::record_running_tick` and moved `tick_total_us` onto `_pad_ticks` to preserve `_pad_max_us` cache-line isolation; tracked `public_events_recorded` separately from hash presence in `arena-bot::Shared` so empty-events reconnect `attach()` snapshots never drop a tick's public events (with regression test + evicted-tick test); extracted and unit-tested `record_seat_rejections` and `load_map` + expanded CLI negative tests (`--map`, `--round-ticks > MAX_ROUND_TICKS`, reverse flag conflicts, non-numeric values); added unclamped `max_ns` in `TickTimings`, `min_s` in `DurationSummary`, overflow-safe `f64::from` subtraction in `policy::decide`, clearer `Display` formatting for `Report` and `HeadlessReport`, and `arena-bot --json` documentation.
- **Learnings:** Promoted shared `arena_headless` policy/telemetry reuse, 1-to-1 running-tick server timing, and reconnect-safe `attach()` event deduplication into `## Codebase Patterns`.
- **Next Recommended Slice:** **Netcode measurement follow-ups** (next Lane A item in `planning/BACKLOG.md`) or **Round clock HUD timer & time-limit result banner** (top Lane B item).

## 2026-11-21 — Netcode measurement follow-ups (Lane A)
- **Branch / Commit:** `auto/netcode-measurements`
- **Base / PR:** `main` (standalone PR)
- **Lane:** A (machine-verified)
- **Task:** Netcode measurement follow-ups ([netcode-hardening](issues/netcode-hardening.md)) — top Lane A item in [planning/BACKLOG.md](BACKLOG.md)
- **Changes:**
  - `arena-protocol`: added offline/diagnostic lossless `delta` module (`SnapshotDelta`, `FieldChangeCounts`, `DeltaSize`, `full_snapshot_encoded_len`) with field-mask + timer/cast countdown-decay prediction and full `valid_snapshot` decode validation without changing `PROTOCOL_VERSION = 3` or `RULES_VERSION = 4`.
  - `arena-bot`: added browser-parity `ViewBuffer` + `--interp-delay-ms` (default 100 ms = 3 ticks) and `FairnessTracker` (`fairness.rs`) correlating targeted casts 1-to-1 with `Snapshot.ack` to measure un-diluted `OutOfRange` / `LineOfSight` / `InvalidTarget` disagreements and distance errors; added O(1)-memory online `BandwidthTracker` (`bandwidth.rs`) comparing steady-state active-combat 30 Hz full snapshots against lossless 30 Hz deltas (with/without `hash`) and event-preserving 20 Hz / 15 Hz decimation; upgraded `--server-pid` Linux `/proc` + `/metrics` profiling into `ServerProfile` (user/kernel/match-tick/non-tick-user CPU cores, peak RSS, threads, per-thread context switches, and µs/snapshot); added unit and live integration tests.
  - `docs` & `planning`: ran 17 release measurement scenarios (`taskset -c 0-7` server vs `8-15` bots) and published exact observed findings and decisions in [docs/research/netcode-measurements-2026-11-21.md](../docs/research/netcode-measurements-2026-11-21.md); cross-linked [docs/research/match-server-load.md](../docs/research/match-server-load.md), updated [docs/PROTOCOL.md](../docs/PROTOCOL.md), [docs/DEVELOPMENT.md](../docs/DEVELOPMENT.md), [planning/issues/netcode-hardening.md](issues/netcode-hardening.md), and [planning/BACKLOG.md](BACKLOG.md) (queuing **Bounded targeting lag compensation / adaptive interpolation delay**).
- **Checks Observed:**
  - `.agents/skills/iterate-game/scripts/run-critics.sh --auto --base main` (`smoke=true web_build=false wasm=true`): pass
    - `./scripts/check.sh`: pass (fmt, clippy `-D warnings`, 209 workspace Rust tests + 150 `arena-client --lib` tests with 1 pre-existing ignored, 52 Python unit tests, `node --check web/input.js`, `scripts/palette.mjs --check`, `tsc --noEmit`, Biome lint/format, 34 Vitest files / 254 tests).
    - `cargo run -p arena-headless --locked`: pass (`winner=Some(0)` at tick 90).
    - `cargo build -p arena-server -p arena-bot --locked && ./scripts/server-smoke.sh`: pass (4×2v2 = 16 bots, 1,142 cross-seat hash checks, 0 mismatches, 0 wrong round results, 17/17 rejoins, 492 ticks, 0 overruns, mean tick 14.1 µs, slowest tick 116 µs, 0 dropped frames, 0 overflows, 0 folded, clean drain).
    - `cargo clippy -p arena-client --target wasm32-unknown-unknown --locked -- -D warnings`: pass.
- **Fresh-Eyes Review:** Addressed all 4 Important and 3 Minor findings from read-only two-stage review: switched `BandwidthStats::summarize` to active stream player-seconds (`snapshots_30hz / tick_rate`) so steady-state `kbit/s` matches `mean_bytes * 30 * 8 / 1000` exactly (and re-ran the three 25 s release bandwidth measurements to update docs with freshly observed figures); fixed `server_target_was_alive` to recognize killing-blow `Hit` events on post-Phase-4c snapshots (with regression test); deduplicated `percentiles` into `crate::percentiles`; enforced `2..=MAX_FIGHTERS` even roster bounds in `SnapshotDelta::from_pair` and added full negative decode tests + cast-completion decay test; fixed `had_pending` carried-event detection, strict 2-tick 15 Hz stride, `metrics_start.zip(server_metrics)` delta guard, and doc wording polish.
- **Learnings:** Promoted active-stream bitrate denominator and `arena-sim::casting` validation-order / post-Phase-4c `Hit` fairness accounting into `## Codebase Patterns`.
- **Next Recommended Slice:** **Bounded targeting lag compensation / adaptive interpolation delay** (top Lane A item in `planning/BACKLOG.md`) or **Round clock HUD timer & time-limit result banner** (top Lane B item).

## 2026-11-21 — Bounded targeting lag compensation & adaptive interpolation delay (Lane A)
- **Branch / Commit:** `auto/targeting-lag-comp`
- **Base / PR:** `main` (standalone PR)
- **Lane:** A (machine-verified)
- **Task:** Bounded targeting lag compensation / adaptive interpolation delay ([netcode-hardening](issues/netcode-hardening.md), [ADR 0049](../docs/decisions/0049-bounded-targeting-lag-comp-and-adaptive-interp.md)) — top Lane A item in [planning/BACKLOG.md](BACKLOG.md)
- **Changes:**
  - `docs/decisions/0049-bounded-targeting-lag-comp-and-adaptive-interp.md`: authored Proposed ADR 0049 detailing both mechanisms, anti-exploit boundaries, and exact observed release matrix results; updated `docs/PROTOCOL.md`, `docs/ARCHITECTURE.md`, `docs/DEVELOPMENT.md`, `docs/research/netcode-measurements-2026-11-21.md`, `planning/issues/netcode-hardening.md`, and `planning/BACKLOG.md`.
  - `arena-sim`: added zero-allocation inline `PositionHistory` (`MAX_LAG_COMP_TICKS = 6` = 200 ms, cleared on `restore`/`restore_in_place`/`relayout`, excluded from `PartialEq`/`state_hash`), `ActorInput.rewind_ticks: u8`, and Phase 4b `target_context_with_lag_comp` checking present tick first then `[clamped, clamped - 1]` with simultaneous range+LoS from the caster's present authoritative position (zero range creep on stationary targets, strict present-tick target liveness and Phase 4a timed cast completions); added behavioral, pillar-LoS, anti-mix-and-match, and replay determinism tests.
  - `arena-protocol` & `web`: added clock-skew-invariant `InterpDelay` (`1.25–5.0` ticks, backlog-immune, per-tick deduplicated underruns, finiteness guards) to `arena_protocol::prediction`; bumped `PROTOCOL_VERSION = 4` (Rust + `web/src/domain/network.ts` + golden frames) and `RULES_VERSION = 5`, adding `InputFrame.view_tick: u64`.
  - `arena-server`: added per-seat monotonic `max_view_tick` clamped to `<= post_tick` (retained across reconnects, fold-safe across `MAX_BACKLOG` folding, `view_tick == 0` = no rewind without mutating `max_view_tick`), plus session unit tests and a live WebSocket test.
  - `arena-client` & `arena-bot`: wired `InterpDelay` and monotonic `last_view_tick` into `arena-client::network::Session` (with waiting-phase and same-seat rematch regression tests) and `arena-bot` (`--adaptive-interp`, `DecisionView.remote_tick`, `casts_saved_by_lag_comp`, `effective_interp_delay_ms`, and live adaptive-interp integration test); verified on the 12-case 20×2v2 release matrix (cutting `OutOfRange` disagreements from 7.94–9.26% to 0.18–0.57% across 0–100 ms RTT and from 15.46–18.40% to 6.42–8.12% at 200 ms RTT / 2% TCP loss + 40 ms clumping, with 0 hash mismatches, 0 tick overruns, and 2.2–3.3 µs mean tick).
- **Checks Observed:**
  - `.agents/skills/iterate-game/scripts/run-critics.sh --auto --base main` (`smoke=true web_build=true wasm=true`): pass
    - `./scripts/check.sh`: pass (fmt, clippy `-D warnings`, 219 workspace Rust tests + 152 `arena-client --lib` tests with 1 pre-existing ignored, 52 Python unit tests, `node --check web/input.js`, `scripts/palette.mjs --check`, `tsc --noEmit`, Biome lint/format, 34 Vitest files / 254 tests).
    - `cargo run -p arena-headless --locked`: pass (`winner=Some(0)` at tick 90).
    - `cargo build -p arena-server -p arena-bot --locked && ./scripts/server-smoke.sh`: pass (4×2v2 = 16 bots, 1,141 cross-seat hash checks, 0 mismatches, 0 wrong round results, 17/17 rejoins, 496 ticks, 0 overruns, mean tick 12.1 µs, slowest tick 72 µs, 0 dropped frames, 0 overflows, 0 folded, clean drain).
    - `npm run build`: pass (359 modules transformed).
    - `cargo clippy -p arena-client --target wasm32-unknown-unknown --locked -- -D warnings`: pass.
  - `./scripts/build-web.sh`: pass (WASM + Vite production build, 55 static resources; no browser launched).
- **Fresh-Eyes Review:** Addressed both Important and all 4 Minor findings from read-only two-stage review: gated `InterpDelay::observe_underrun` on `Phase::Playing && newest_u64 > 0` (plus `newest_tick == 0` guard in `InterpDelay`) so `Phase::Waiting` frames never bump delay above 3.0 ticks; cleared `snapshots`/`view_tick`/`last_view_tick` on `Welcome` tick rewinds and `Snapshot` rewinds and gated `last_view_tick` updates on `Phase::Playing` (with regression tests for waiting frames and same-seat rematches); passed `def.range` directly into `target_context_with_lag_comp` to avoid duplicate `spell.definition()` allocation; added `session/tests.rs` coverage for `view_tick == 0`, `> MAX_LAG_COMP_TICKS`, and `MAX_BACKLOG` folding; added finiteness guards in `InterpDelay::observe_frame`; updated `docs/PROTOCOL.md` v4 input byte note and `docs/ARCHITECTURE.md`.
- **Learnings:** Promoted `PositionHistory` equality/hash boundary, simultaneous same-tick range+LoS rewind, server-clamped monotonic `max_view_tick`, and `Phase::Playing`-gated `observe_underrun`/`last_view_tick` invariants into `## Codebase Patterns`.
- **Next Recommended Slice:** **Server-authoritative end-of-match scoreboard statistics (Lane A)** (`planning/issues/scoreboard.md`) or **Round clock HUD timer & time-limit result banner (Lane B)** (`planning/issues/round-clock.md`).

## 2026-11-22 — Server-authoritative end-of-match scoreboard statistics (Lane A)
- **Branch / Commit:** `auto/scoreboard-stats`
- **Base / PR:** `main` (standalone PR)
- **Lane:** A (machine-verified)
- **Task:** Server-authoritative end-of-match scoreboard statistics ([scoreboard](issues/scoreboard.md)) — top Lane A item in [planning/BACKLOG.md](BACKLOG.md)
- **Changes:**
  - `arena-protocol` (`PROTOCOL_VERSION = 5`, `RULES_VERSION = 5` unchanged): added `FighterScore` (`damage_done`, `damage_taken`, `healing_done`, `healing_received`, `killing_blows`) and `Snapshot.scoreboard: Option<Vec<FighterScore>>` (`None` = 1 byte while running; `Some(scores)` on finished snapshots, excluded from `Arena::state_hash`); updated `EncodedSnapshotBody` to encode `scoreboard` once per tick in `tail` and `SnapshotDelta` to stay lossless across finished ticks; added strict decode invariants (`scoreboard.is_some()` requires `result.is_some()` and `scores.len() == fighters.len()`), golden suffix bytes, adversarial OOM/tag tests, and fuzz checks; updated `docs/PROTOCOL.md` (including the 5 s finish-linger timing rationale).
  - `arena-server` & `arena-bot`: accumulated authoritative per-seat `damage_done`, `damage_taken`, effective `healing_done`/`healing_received` (overheal excluded), and single-per-death `killing_blows` in `Session::record_tick_scores` (preserved across disconnects/reconnects and emitted identically by `tick()` and `tick_encoded()`); added unit tests covering self/teammate heals, simultaneous team kills, overkill, mutual draws, time-limit draws, reconnects, and `tick_encoded` parity plus a live WebSocket E2E test; extended `arena-bot` to verify scoreboard presence and cross-seat equality on finished matches.
  - `arena-client` & `web`: renumbered `scoreboard` into local actor order via `self.order` in `arena-client::network::Session`, cleared buffered snapshots on `Welcome` after any finished snapshot (`newest.result.is_some()`), and serialized `"scoreboard"` in `publish()` JSON; bumped `PROTOCOL_VERSION = 5` in `web/src/domain/network.ts`, added strict `adaptSnapshot` validation and `needsImmediateDelivery` edge in `web/src/bridge/snapshot.ts`, and added pure TS domain helpers in `web/src/domain/scoreboard.ts` (`buildMatchScoreboard`, `sortScoreboardRows`, `toggleScoreboardSort`, `sumTeamTotals`, `formatMatchDuration`, `isValidScoreboardEntry`, `expectedActorTag`) with unit tests in `tests/domain/scoreboard.test.ts` and `tests/domain/bridge.test.ts`.
- **Checks Observed:**
  - `.agents/skills/iterate-game/scripts/run-critics.sh --auto --base main` (`smoke=true web_build=true wasm=true`): pass
    - `./scripts/check.sh`: pass (fmt, clippy `-D warnings`, 224 workspace Rust tests + 154 `arena-client --lib` tests with 1 pre-existing ignored, 52 Python unit tests, `node --check web/input.js`, `scripts/palette.mjs --check`, `tsc --noEmit`, Biome lint/format, 35 Vitest files / 260 tests).
    - `cargo run -p arena-headless --locked`: pass (`winner=Some(0)` at tick 90).
    - `cargo build -p arena-server -p arena-bot --locked && ./scripts/server-smoke.sh`: pass (4×2v2 = 16 bots, 1,141 cross-seat hash checks, 0 mismatches, 0 wrong round results, 17/17 rejoins, 496 ticks, 0 overruns, mean tick 12.5 µs, slowest tick 151 µs, 0 dropped frames, 0 overflows, 0 folded, clean drain).
    - `npm run build`: pass (361 modules transformed).
    - `cargo clippy -p arena-client --target wasm32-unknown-unknown --locked -- -D warnings`: pass.
  - `./scripts/build-web.sh`: pass (WASM + Vite production build, 55 static resources; no browser launched).
- **Fresh-Eyes Review:** Addressed both Important and both Minor findings from read-only two-stage review: tightened the live WebSocket scoreboard test guard in `crates/arena-server/tests/server.rs` to require both opening hits (`fighters[0].health < 100 && fighters[1].health < 100 && fighters[1].gcd_remaining == 0`) before Mend and `snap.ack >= last_attack_seq` between Pulses; updated `arena-client::network::Session` `Welcome` buffer clearing to also clear whenever `newest.result.is_some()` (with a rematch regression test where `welcome.tick >= prev_finish_tick`); added a 2v2 teammate-healing (`actor != target`) scoreboard test in `crates/arena-server/src/session/tests.rs`; exported and reused `isNonNegativeSafeInt` from `web/src/domain/scoreboard.ts` in `web/src/bridge/snapshot.ts`.
- **Learnings:** Promoted `Snapshot.scoreboard` wire placement, single-per-death killing-blow accounting, and `newest.result.is_some()` rematch buffer clearing into `## Codebase Patterns`.
- **Next Recommended Slice:** Run a Critic Audit pass to populate the next Lane A queue item (e.g., custom matches or spectating backend), or pick up a Lane B UI slice (**Round clock HUD timer & time-limit result banner** or **End-of-match scoreboard** modal).

## 2026-11-22 — Round clock HUD timer & time-limit result banner (Lane B)
- **Branch / Commit:** `auto/round-clock-hud`
- **Base / PR:** `main` (standalone PR)
- **Lane:** B (pending human browser/UI review)
- **Task:** Round clock HUD timer & time-limit result banner ([round-clock](issues/round-clock.md)) — top Lane B item in [planning/BACKLOG.md](BACKLOG.md)
- **Changes:**
  - `web/src/domain/round-clock.ts` & `web/src/bridge/snapshot.ts`: added `snapshotRemainingSeconds`, `roundClockFromSeconds`, `roundClockView`, and `roundResultBanner` (handling missing metadata, warning minute while active, non-expired last-tick eliminations, and practice vs networked result copy); updated `needsImmediateDelivery` to deliver whole-second countdown transitions and `tickRate` changes immediately without 80 ms HUD-throttle jitter.
  - `web/src/features/round-clock/{RoundClock.tsx,round-clock.css}`, `web/src/app/App.tsx`, `web/src/styles/global.css`: added centered authoritative `MM:SS` countdown (`role="timer"`, `aria-live="off"`, tabular numerals, final-minute warning and expired palette styles) and outcome banner (`"Time's up · draw"`, `"Victory"`, `"Defeat"`, `"Draw"`, plus local **Reset practice** button); suppressed during exclusive modes (`layout`, `quick`, `capture`, `menu`, `settings`, fatal `error`, `customizer`, `StartScreen`) while staying visible with the non-modal Spellbook; yielded `.round-result` to `MatchStatus` whenever a match banner is active.
  - `web/src/domain/{matchmaking,network}.ts`, `web/src/features/{queue/QueueBar.tsx,combat-errors,match-status}`: shared `isQueueBarVisible` across `QueueBar`, `RoundClock`, `CombatErrors`, and `MatchStatus` for collision-free 3-tier top-center stacking (`12px` / `max(50px, 7%)` / `max(84px, 12%)`, shifting to `108px` / `146px` / `180px` under `QueueBar`); retained the final networked `roundResult` in `MatchStatus` across `MatchOver` (`"Time's up · draw · The match is over."`) including batched/reconnecting closes while preventing stale outcome leakage across fast rematches.
  - Tests & docs: added `tests/ui/round-clock.test.tsx`, expanded `tests/domain/{round-clock,bridge,network,matchmaking}.test.ts` and `tests/ui/network-match.test.tsx`, updated `docs/FRONTEND.md`, `planning/BACKLOG.md`, and `planning/ROADMAP.md`, and appended a `pending human review` entry to `docs/UI-REVIEW.md`.
- **Checks Observed:**
  - `.agents/skills/iterate-game/scripts/run-critics.sh --auto --base main` (`smoke=false web_build=true wasm=false`): pass
    - `./scripts/check.sh`: pass (fmt, clippy `-D warnings`, 224 workspace Rust tests + 154 `arena-client --lib` tests with 1 pre-existing ignored, 52 Python unit tests, `node --check web/input.js`, `scripts/palette.mjs --check`, `tsc --noEmit`, Biome lint/format, 36 Vitest files / 274 tests).
    - `cargo run -p arena-headless --locked`: pass (`winner=Some(0)` at tick 90).
    - `npm run build`: pass (362 modules transformed).
  - `cargo clippy -p arena-client --target wasm32-unknown-unknown --locked -- -D warnings`: pass.
  - `./scripts/build-web.sh`: pass (WASM + Vite production build, 55 static resources; no browser launched; human browser review pending in `docs/UI-REVIEW.md`).
- **Fresh-Eyes Review:** Addressed all 4 Important and 3 Minor findings from read-only two-stage review: decoupled `.round-result` from `.round-clock` into a 3-tier top-center stack with explicit `z-index` (`6`/`7`/`8`) and coordinated `below-queue` offsets (`108px`/`146px`/`180px`) across `RoundClock`, `CombatErrors`, and `MatchStatus`; suppressed `.round-result` whenever `MatchStatus` is active to eliminate duplicate/contradictory `MatchOver` banners; hardened `MatchStatus` outcome retention against batched/reconnecting closes and fast-rematch stale leaks (with regression tests); suppressed `RoundClock` during exclusive UI modes (`layout`, `quick`, `capture`, `menu`, `settings`) and fatal engine errors; prevented last-tick eliminations from styling the clock as `expired`; simplified `needsImmediateDelivery` via `snapshotRemainingSeconds`; added explicit `pointer-events: auto` on `.round-result button`, updated `docs/FRONTEND.md`, and recorded `pending human review` in `docs/UI-REVIEW.md`.
- **Learnings:** Promoted 3-tier top-center HUD stacking, `below-queue` coordination, exclusive-mode suppression, and `status.phase`-gated `MatchStatus` outcome retention into `## Codebase Patterns`.
- **Next Recommended Slice:** Either the next Lane B UI item (**End-of-match scoreboard** modal or **Networked character appearance**) or a Critic Audit pass to queue the next Lane A backend slice.

## 2026-11-22 — End-of-match scoreboard modal (Lane B)
- **Branch / Commit:** `auto/scoreboard-ui`
- **Base / PR:** `main` (PR #1 of Lane B stack)
- **Lane:** B (pending human browser/UI review)
- **Task:** End-of-match scoreboard ([scoreboard](issues/scoreboard.md)) — Lane B item in [planning/BACKLOG.md](BACKLOG.md)
- **Changes:**
  - `web/src/domain/{round-clock,scoreboard}.ts`: exported shared `roundResultTone` and added `scoreboardQueueMode` / `scoreboardTone`.
  - `web/src/state/{scoreboard,session,appearance}.ts` & `web/src/runtime/{start,input-router,network}.ts`: added `postMatchScoreboard` / `scoreboardVisible` with canonical `syncPostMatchScoreboard(snapshot, linkState)` (deduplicating 80 ms linger ticks, preventing stale Match 1 scoreboard leaks across fast rematch `connecting`/`waiting`/`playing` transitions, closing transient modals/editors on open and `returnToHome()`, and forgetting finished match tickets in `link.leave()`); added `scoreboardVisible` to `blocksGameplay()` and `installInput`.
  - `web/src/features/scoreboard/{Scoreboard.tsx,scoreboard.css}`, `RoundClock.tsx`, `MatchStatus.tsx`, `App.tsx`, `global.css`: rendered the sortable team-grouped scoreboard dialog (local-player `"You"` highlight, `"Disconnected"` badges, per-team totals, `aria-sort`, responsive narrow columns, `below-queue` spacing under `QueueBar`, **Leave** to home screen, and **Queue again** using the finished match's mode with party-leader/size checks, double-click guard, and refusal notices); replaced `"match closing shortly"` and `MatchOver` banners and retained the modal after `MatchOver`.
  - Tests & docs: added `tests/ui/scoreboard.test.tsx`, expanded `tests/domain/scoreboard.test.ts` and `tests/ui/network-match.test.tsx`, updated `docs/FRONTEND.md`, `planning/BACKLOG.md`, and `planning/ROADMAP.md`, and appended a `pending human review` entry to `docs/UI-REVIEW.md`.
- **Checks Observed:**
  - `.agents/skills/iterate-game/scripts/run-critics.sh --auto --base main` (`smoke=false web_build=true wasm=false`): pass
    - `./scripts/check.sh`: pass (fmt, clippy `-D warnings`, Rust workspace & `arena-client` tests, 52 Python unit tests, `node --check web/input.js`, `scripts/palette.mjs --check`, `tsc --noEmit`, Biome lint/format, 37 Vitest files / 282 tests).
    - `cargo run -p arena-headless --locked`: pass (`winner=Some(0)` at tick 90).
    - `npm run build`: pass (364 modules transformed).
  - `cargo clippy -p arena-client --target wasm32-unknown-unknown --locked -- -D warnings`: pass.
  - `./scripts/build-web.sh`: pass (WASM + Vite production build, 55 static resources; no browser launched; human browser review pending in `docs/UI-REVIEW.md`).
- **Fresh-Eyes Review:** Addressed all 1 Critical, 5 Important, and 2 Minor findings from read-only two-stage review: centralized `syncPostMatchScoreboard` in `state/scoreboard.ts` gating capture on `finished`/`MatchOver` and clearing on `connecting`/`waiting`/`playing` to prevent Match 1 scoreboard leaks during fast rematches and eliminate redundant 80 ms linger re-renders; preferred `scoreboardQueueMode(board) ?? queueState.mode` over stale queue modes; closed transient modals/editors (`setPanel(null)`, `pendingBinding`, `customizing`) on scoreboard open and `returnToHome()`; added in-flight double-click protection and refusal notice display on **Queue again**; shared `roundResultTone`; recorded `pending human review` in `docs/UI-REVIEW.md`; expanded negative/regression tests.
- **Learnings:** Promoted `syncPostMatchScoreboard` fast-rematch `waiting`-window guard and transient-modal cleanup into `## Codebase Patterns`.
- **Next Recommended Slice:** **Networked character appearance** ([networked-appearance](issues/networked-appearance.md)) on `auto/networked-appearance` stacked on `auto/scoreboard-ui`.

## 2026-11-22 — Networked character appearance & per-fighter kits (Lane B)
- **Branch / Commit:** `auto/networked-appearance`
- **Base / PR:** `auto/scoreboard-ui` (stacked PR #2 of Lane B stack)
- **Lane:** B (pending human browser/UI review)
- **Task:** Networked character appearance ([networked-appearance](issues/networked-appearance.md)) — Lane B item in [planning/BACKLOG.md](BACKLOG.md)
- **Changes:**
  - `arena-protocol` (bumped to `PROTOCOL_VERSION = 6`, `RULES_VERSION = 5` unchanged): added bounded ASCII-identifier `WireAppearance` (`MAX_APPEARANCE_ID = 32`, `MAX_APPEARANCE_GROUPS = 10`, `MAX_APPEARANCE_VARIANTS = 8`, `build <= 100`, unique keys, no path chars) on `ClientMessage::Hello` and per-seat `ServerMessage::Appearances` (variant 4, keeping `Error` at variant 3) with golden-frame, malformed, path-traversal, duplicate-key, invalid-UTF-8, and oversized tests.
  - `arena-server` & `arena-bot`: validated `Hello.appearance`, locked each seat's first-join look (including `None`) across disconnects/reconnects in `Session::attach_with_appearance`, and broadcast `ServerMessage::Appearances` on join and new-seat lock with unit and live WebSocket E2E tests.
  - `arena-client`: added `KitCache` (deduplicating GLBs & `AnimationGraph`s across fighters), per-fighter `FighterBody`/`FallbackActor`/`KitInstance` with per-fighter material slot deduplication and `CharacterModel` A→B→A recovery, per-fighter `character_animation::Controller`, `Session` local-order `Appearances` renumbering + JSON bridge (`onAppearances`), and measured 3v3 budgets against ADR 0035 (typical 3v3: 2 cached GLBs / 25.57 MB, 72 visible draw calls, ≤ 36 material instances).
  - `web/`: updated `PROTOCOL_VERSION = 6` and `encodeHello` (byte-pinned against Rust, locking look on join across reconnects), protected `Appearances` alongside `Welcome` during `MAX_QUEUED` background-tab eviction, cleared `remoteAppearances` synchronously on link reset, added deterministic `practiceAppearances` and catalog-validated `resolveRosterSpecs` with default fallback, skipped `StartScreen` on active join-link/reload matches, hid `Customize character` in `GameMenu` during matches, and appended a `pending human review` entry to `docs/UI-REVIEW.md`.
- **Checks Observed:**
  - `.agents/skills/iterate-game/scripts/run-critics.sh --auto --base auto/scoreboard-ui` (`smoke=true web_build=true wasm=true`): pass
    - `./scripts/check.sh`: pass (fmt, clippy `-D warnings`, Rust workspace & `arena-client` tests, 52 Python unit tests, `node --check web/input.js`, `scripts/palette.mjs --check`, `tsc --noEmit`, Biome lint/format, 37 Vitest files / 284 tests).
    - `cargo run -p arena-headless --locked`: pass (`winner=Some(0)` at tick 90).
    - `cargo build -p arena-server -p arena-bot --locked && ./scripts/server-smoke.sh`: pass (4×2v2 = 16 bots, 1,142 cross-seat hash checks, 0 mismatches, 17/17 rejoins, mean tick 6.0 µs, slowest tick 62 µs, clean drain).
    - `npm run build`: pass (364 modules transformed).
    - `cargo clippy -p arena-client --target wasm32-unknown-unknown --locked -- -D warnings`: pass.
  - `./scripts/build-web.sh`: pass (WASM + Vite production build, 55 static resources; no browser launched; human browser review pending in `docs/UI-REVIEW.md`).
- **Fresh-Eyes Review:** Addressed all 3 Important and 5 Minor findings from read-only two-stage review: protected `ServerMessage::Appearances` (variant 4) from `MAX_QUEUED` overflow eviction (replacing older queued copies in place); restored `CharacterModel` ready state when actor 0 reverts A→B→A before Kit B finishes and stored base `StandardMaterial` inside `KitInstance::slot_materials`; added synchronous `onReset` clearing of `remoteAppearances` on link end, cleared `Session.appearances` on new socket generations, closed `StartScreen`/`Customizer` on direct join links and skipped `prematch` on active join-link reloads; aligned relayed `null` fallback on seat 0 with peer rendering; added `check_loading` warning for failed remote kits and expanded negative/regression tests.
- **Learnings:** Promoted `KitCache`, per-fighter `(base, handle)` material slot deduplication, and `isAppearances` queue-overflow protection into `## Codebase Patterns`.
- **Next Recommended Slice:** **Private custom matches** ([custom-matches](issues/custom-matches.md)) on `auto/custom-matches` stacked on `auto/networked-appearance`.

## 2026-11-22 — Private custom matches (Lane B)
- **Branch / Commit:** `auto/custom-matches`
- **Base / PR:** `auto/networked-appearance` (stacked PR #3 of Lane B stack)
- **Lane:** B (pending human browser/UI review)
- **Task:** Private custom matches ([custom-matches](issues/custom-matches.md)) — Lane B item in [planning/BACKLOG.md](BACKLOG.md)
- **Changes:**
  - `arena-server::ticket` & `arena-server::App`: added versioned, allowlisted `MatchSettings` (`version = 1`, `MapId::Colosseum`, `300/600/900/1200 s` round limits) to signed `Claims` (`issue_custom_match`, validated in `Claims::is_playable()`), enforced by `App::match_for` when starting a custom match.
  - `arena-server::matchmaker` (`crates/arena-server/src/matchmaker_custom.rs`) & `door/queue.rs`: added private custom lobby state machine sharing `/api/queue`'s per-address connection/rate limits plus `door.creations` per-address rate limiting on `start_lobby` and `arena_door_custom_matches_total` metric; collision-free 6-char invite codes (never seat credentials), Team A / Team B / 4 Spectator slots, self-move, host move/swap across full teams, kick, host transfer, size changes with spectator overflow, versioned settings validation, full-lobby start guard, mutual exclusion with parties/queues, and persistent lobby membership for instant rematches without new invites; added unit + live WebSocket E2E tests in `matchmaker_custom.rs`, `door/queue.rs`, `tests/door.rs`, and `tests/server.rs`.
  - `web/`: added `CustomLobby` modal (`web/src/features/custom-lobby/{CustomLobby.tsx,custom-lobby.css}`), `#custom=CODE` fragment invite links, custom lobby requests/parsers/notices in `domain/matchmaking.ts` & `runtime/matchmaking.ts`, `PlayOnline` entry, shared `useQueueBarVisible()` minimized lobby pill across top-center HUD overlays, `blocksGameplay()` / Escape integration, and **Return to lobby** rematch action on `Scoreboard`; updated `docs/PROTOCOL.md`, `docs/FRONTEND.md`, `planning/BACKLOG.md`, `planning/ROADMAP.md`, and appended a `pending human review` entry to `docs/UI-REVIEW.md`.
- **Checks Observed:**
  - `.agents/skills/iterate-game/scripts/run-critics.sh --auto --base auto/networked-appearance` (`smoke=true web_build=true wasm=true`): pass
    - `./scripts/check.sh`: pass (fmt, clippy `-D warnings`, Rust workspace & `arena-client` tests including 55 `arena-server` unit + 8 door E2E + 19 server E2E, 52 Python unit tests, `node --check web/input.js`, `scripts/palette.mjs --check`, `tsc --noEmit`, Biome lint/format, 38 Vitest files / 289 tests).
    - `cargo run -p arena-headless --locked`: pass (`winner=Some(0)` at tick 90).
    - `cargo build -p arena-server -p arena-bot --locked && ./scripts/server-smoke.sh`: pass (4×2v2 = 16 bots, 1,142 cross-seat hash checks, 0 mismatches, 17/17 rejoins, mean tick 18.7 µs, slowest tick 48 µs, clean drain).
    - `npm run build`: pass (365 modules transformed).
    - `cargo clippy -p arena-client --target wasm32-unknown-unknown --locked -- -D warnings`: pass.
  - `./scripts/build-web.sh`: pass (WASM + Vite production build, 55 static resources; no browser launched; human browser review pending in `docs/UI-REVIEW.md`).
- **Fresh-Eyes Review:** Addressed all 3 Critical, 4 Important, and 3 Minor findings from read-only two-stage review: unified top-center visibility via `useQueueBarVisible()` and suppressed minimized lobby `QueueBar` behind `Scoreboard`; gated `CustomLobby` auto-open on `!inMatch && !scoreboardOpen` and cleared `customLobbyVisible` in `closeTransientModes()` and `returnToHome()`; added per-address `door.creations` rate limiting on `start_lobby` plus client pending double-click guard; fixed Escape precedence above `Customizer` and stopped eager `GameMenu` close on `Host custom match`; updated `docs/PROTOCOL.md` & `door/queue.rs` docs; avoided broadcasting idempotent re-joins; keyed `LobbyInvite` by code; disabled full non-swappable host move buttons; added negative/edge tests across Rust and Vitest.
- **Learnings:** Promoted `Matchmaker` custom-lobby `door.creations` rate limiting and shared `useQueueBarVisible()` top-bar coordination into `## Codebase Patterns`.
- **Next Recommended Slice:** **Spectating** ([spectating](issues/spectating.md)) on `auto/spectating` stacked on `auto/custom-matches`.

## 2026-11-22 — Live spectating, anti-ghosting delay & spectator HUD (Lane B)
- **Branch / Commit:** `auto/spectating`
- **Base / PR:** `auto/custom-matches` (stacked PR #4 of Lane B stack)
- **Lane:** B (pending human browser/UI review)
- **Task:** Spectating ([spectating](issues/spectating.md), [ADR 0050](../docs/decisions/0050-spectating-and-anti-ghosting-delay.md)) — final Lane B item in [planning/BACKLOG.md](BACKLOG.md)
- **Changes:**
  - `arena-protocol` (bumped to `PROTOCOL_VERSION = 7`, `RULES_VERSION = 5` unchanged): added `Welcome.spectator: bool` (enforcing `actor == 0` when `spectator` in `valid_welcome`), `Welcome::seat()`, and `WireEvent::for_spectator` (unconditionally stripping private `ArenaEvent::Rejected` events), with updated golden wire layout and spectator privacy/welcome tests.
  - `arena-server`: replaced `Claims.seat` with signed `ticket::Role::{Player { seat }, Spectator}` and added `allow_spectators: bool` (default `true`) to versioned `MatchSettings`; implemented 5 s (`150` ticks) anti-ghosting delay queue, zero-leak mid-match reconnects, `Phase::Finished` delayed drain (up to `spectator_delay + linger`), `DEFAULT_MAX_SPECTATORS = 4` cap (`MatchFull` on distinct overflow, `Replaced` on same-id reconnect), strict server drop of all `ClientMessage::Input` from spectators (preserving `RateWindow` and `Ping`/`Pong`), `arena_spectators_open` metric, zero-copy `Arc<[u8]>` (`Bytes::from_owner`) broadcast fanout, `--spectator-delay-seconds` / `--max-spectators` CLI flags, and `--dev` `POST /dev/match?spectators=M`; updated `Matchmaker` (`matchmaker_custom.rs`) & `door/queue.rs` to enforce `allow_spectators` (`spectators_disabled` / `spectators_occupied`) and issue live signed `ToClient::Match { spectator: true, ... }` tickets on `start_lobby`.
  - `arena-bot` & measurements: added watch-only `bot::Spectator` runner, `--spectators N` CLI flag, privacy check (`spectator_private_events == 0`), and integration test; measured isolated release `25 × 3v3` (150 bots + `0`/`25`/`50`/`100` spectators) in [docs/research/spectator-cost-2026-11-22.md](../docs/research/spectator-cost-2026-11-22.md) (`+0.04` cores for `+100` spectators ≈ `+0.4` millicores/spectator, `+13.5 MiB` RSS ≈ `138 KiB`/spectator, `~37.5 kbit/s` steady-state 3v3 outbound payload, `0` overruns, `0` private events across `29,924` snapshots, `0` hash mismatches across `72,630` checks).
  - `arena-client` & `web/`: added `SpectatorCamera` (clamped free-camera flight + follow-a-player orbit), watch-only `network::Session` (no `Input` emission, server actor order, actor-0 nameplate projection, `spectating` / `spectatorFollow` snapshot fields), `web/input.js` index 20 `spectatorCmd`, `SpectatorHud` (`web/src/features/spectator/{SpectatorHud.tsx,spectator.css}` with Team A / Team B unit frames showing health/resource/cast bars/lockout/disconnected and camera toolbar), blocked combat/targeting/spellbook/layout/quick-bind commands while spectating, host **Allow spectators** toggle in `CustomLobby`, neutral `Team A` / `Team B` labels in `RoundClock` & `Scoreboard` (with **Return to lobby** / **Leave**), and spectator-specific `GameMenu` & `MatchStatus` copy; authored [ADR 0050](../docs/decisions/0050-spectating-and-anti-ghosting-delay.md), updated `docs/PROTOCOL.md`, `docs/FRONTEND.md`, `planning/BACKLOG.md`, `planning/ROADMAP.md`, and appended a `pending human review` entry to `docs/UI-REVIEW.md`.
- **Checks Observed:**
  - `.agents/skills/iterate-game/scripts/run-critics.sh --auto --base auto/custom-matches` (`smoke=true web_build=true wasm=true`): pass
    - `./scripts/check.sh`: pass (fmt, clippy `-D warnings`, 235 workspace Rust tests including 57 `arena-server` unit + 8 door E2E + 20 server E2E + 10 `arena-bot` integration tests + 161 `arena-client --lib` tests with 1 pre-existing ignored, 52 Python unit tests, 42 Node unit tests, `node --check web/input.js`, `scripts/palette.mjs --check`, `tsc --noEmit`, Biome lint/format, 39 Vitest files / 292 tests).
    - `cargo run -p arena-headless --locked`: pass (`winner=Some(0)` at tick 90).
    - `cargo build -p arena-server -p arena-bot --locked && ./scripts/server-smoke.sh`: pass (4×2v2 = 16 bots, 1,141 cross-seat hash checks, 0 mismatches, 17/17 rejoins, mean tick 15.0 µs, slowest tick 81 µs, clean drain).
    - `npm run build`: pass (366 modules transformed).
    - `cargo clippy -p arena-client --target wasm32-unknown-unknown --locked -- -D warnings`: pass.
  - `./scripts/build-web.sh`: pass (WASM `34,527,354` B / `7,434,762` B gzip + Vite production build, 55 static resources; no browser launched; human browser review pending in `docs/UI-REVIEW.md`).
- **Fresh-Eyes Review:** Addressed all 5 Important and 6 Minor findings from read-only two-stage review: kept `tick_encoded()` draining `spectator_queue` when temporarily empty; refused `Phase::Finished` spectator attachments with only a stale non-finished `last_spectator_snapshot`; flushed due spectator snapshots before `Phase::Finished` close so `linger == 0` never drops the final scoreboard; kept `Space` rise held continuously in spectator free-camera (`input-router.ts`) while preserving edge-triggered player jumping; cleared incompatible combat UI modes (`spellbook`/`layout`/`quick`) on queue match join and entering spectating; added `LinkState.spectator` for spectator-accurate `Replaced` and `MatchFull` banners; guarded `pacer.observe` for spectators; synchronized `docs/PROTOCOL.md` `Welcome` field order, `door/queue.rs` docs, `SpectatorHud` hint copy, and live WS seat-0 stillness + waiting-tick synchronization.
- **Learnings:** Promoted spectator `tick_encoded` draining, `Phase::Finished` zero-linger flush, and `spectating` UI-mode clearing into `## Codebase Patterns`.
- **Next Recommended Slice:** All 4 Lane B items in `planning/BACKLOG.md` are now complete and queued as stacked PRs awaiting human browser review!
- **Playtest Follow-Up (2026-11-22 on `auto/spectating`):** User verified all 4 stacked features locally ("all worked") and requested two tweaks: removed the scoreboard's horizontal scrollbar (fixed on `auto/scoreboard-ui`: `940px` modal, wrapping stat headers, `880px` compact breakpoint; merged up the stack), and changed `DEFAULT_SPECTATOR_DELAY` to `0` (live/real-time by default, delivering live snapshots immediately on mid-match joins while preserving optional `--spectator-delay-seconds 0..=60` anti-ghosting support, and removing `"5s delay"` HUD/lobby labels).

## 2026-11-22 — Charge cast costs on completion (Lane A, playtest inbox)
- **Branch / Commit:** `auto/cast-cost-on-completion`
- **Base / PR:** `main` — https://github.com/kylek-dev/arena-prototype/pull/82 (base of the 2026-11-22 playtest-inbox stack)
- **Lane:** A (machine-verified)
- **Task:** Playtest Inbox — "Spend the resource when a cast finishes, not when it starts" ([ADR 0048 amendment](../docs/decisions/0048-server-authoritative-casting-and-resources.md))
- **Changes:**
  - `arena-sim`: extracted `check_costs` / `pay_costs` in `casting.rs`; `try_cast` checks costs at start and charges only instants; `CastingState::tick` (now `&mut CasterContext`) re-checks costs on the completion tick and charges them only when the cast resolves; `Arena::advance` 4a writes the charged health/resource back. Health costs follow the same rule.
  - `arena-protocol`: `RULES_VERSION` 5 → 6 (golden Welcome frame updated; wire layout unchanged, `PROTOCOL_VERSION` stays 7).
  - Tests: casting unit tests (no spend at start, single charge at completion, resource/health completion re-check failures spend nothing), arena tests (range/LoS loss, Quell interrupt, cancel/move jukes, Mend) and a restored mid-cast fighter paying exactly once; session test with a part-spent pool; WebSocket test asserts cancelled Mend and interrupted Rift Bolt spent nothing.
  - Docs: ADR 0048 amendment (with balance consequences), `docs/PROTOCOL.md`, `docs/ABILITIES.md` open-decision list.
- **Checks Observed:**
  - `.agents/skills/iterate-game/scripts/run-critics.sh --auto --base main` (`smoke=true web_build=false wasm=true`): pass
    - `./scripts/check.sh`: pass (399 Rust tests passed, 1 pre-existing ignored; 52 Python; 42 Node; 39 Vitest files / 292 tests).
    - `cargo run -p arena-headless --locked`: pass (`winner=Some(0)`).
    - `./scripts/server-smoke.sh`: pass (1,141 cross-seat hash checks, 0 mismatches, 17/17 rejoins, 0 overruns).
    - WASM clippy: pass. `npm run build` / `./scripts/build-web.sh`: not applicable (no `web/` or `arena-client` changes).
- **Fresh-Eyes Review:** No Critical or Important findings. Addressed Minors: refuse a completing cast without a cost re-check before `combat.tick()` starts its cooldown, extra health-failure assertions, restored mid-cast test, ADR balance consequences and wording, `ABILITIES.md` decided items.
- **Learnings:** The practice client, HUD `spell_status` and predictor all route through `arena-sim`, so rule changes need no separate client parity code; the golden Welcome frame pins the `RULES_VERSION` byte.
- **Next Recommended Slice:** Playtest inbox — practice-mode regeneration while the tab is hidden.

## 2026-11-22 — Practice catches up on ticks missed in a hidden tab (Lane A investigation + client fix)
- **Branch / Commit:** `auto/practice-hidden-tab-regen`
- **Base / PR:** `auto/cast-cost-on-completion` — https://github.com/kylek-dev/arena-prototype/pull/83 (stacked on PR #82)
- **Lane:** A investigation with a client fix (pending human browser check in `docs/UI-REVIEW.md`)
- **Task:** Playtest Inbox — "Resource regeneration stops while the tab is hidden" ([ADR 0051](../docs/decisions/0051-practice-catches-up-hidden-tab-ticks.md))
- **Changes:**
  - Confirmed online keeps regenerating: new `idle_seats_keep_regenerating_on_the_servers_clock` session test (90 ticks, no input).
  - Root cause in practice: `FixedUpdate` is fed by `Time<Virtual>`, which Bevy 0.20.1 clamps to 250 ms per frame, so withheld browser frames pause the practice sim.
  - Decision (ADR 0051, proposed): practice catches up, capped at 60 s (1,800 ticks), with an idle local player. `practice_catch_up` (chained before `network_receive`) skips network sessions and frames behind the startup overlay; `CatchUp::take_ticks` keeps the sub-tick remainder; `apply_practice_events` and `fighter_positions` are shared with `simulate`.
  - Tests: `CatchUp` arithmetic/cap/guards, and a Bevy `TimeUpdateStrategy::ManualDuration` app test (10 s gap → ~300 ticks and a refilled pool; no catch-up behind the overlay or in a session). Manual scenario 11 added to `docs/BROWSER.md`.
- **Checks Observed:**
  - `.agents/skills/iterate-game/scripts/run-critics.sh --auto --base auto/cast-cost-on-completion` (`smoke=true web_build=false wasm=true`): pass
    - `./scripts/check.sh`: pass (402 Rust tests, 1 pre-existing ignored; 52 Python; 42 Node; 39 Vitest files / 292 tests).
    - `cargo run -p arena-headless --locked`: pass (`winner=Some(0)`).
    - `./scripts/server-smoke.sh`: pass (1,142 cross-seat hash checks, 0 mismatches, 17/17 rejoins, 0 overruns).
    - WASM clippy: pass.
  - `./scripts/build-web.sh`: pass (55 static resources; no browser launched).
- **Fresh-Eyes Review:** No Critical or Important findings. Addressed Minors: moved catch-up before `network_receive` so a match ending in a hidden tab never ages the fresh practice round, gated catch-up on the startup overlay, deduplicated position collection, and tightened ADR wording (any >250 ms frame triggers it, test length, cited perf figure, browser behaviour stated as expected rather than observed).
- **Learnings:** In `bevy_time` 0.20.1 `Time<Virtual>::effective_speed` already includes the `max_delta` clamp; compare `Time<Real>` × `relative_speed` with the virtual delta to measure lost time.
- **Next Recommended Slice:** Playtest inbox — smooth cast bars.

## 2026-11-22 — Smooth cast bars (Lane B, playtest inbox)
- **Branch / Commit:** `auto/smooth-cast-bars`
- **Base / PR:** `auto/practice-hidden-tab-regen` — https://github.com/kylek-dev/arena-prototype/pull/84 (stacked on PR #83)
- **Lane:** B (pending human browser review in `docs/UI-REVIEW.md`)
- **Task:** Playtest Inbox — "Make cast bars smooth"
- **Changes:**
  - `web/src/domain/cast-bar.ts`: pure `CastTimeline` (snap on new spell/duration/remaining going up, pull forward to fresh samples, resync on >0.2 s lag, stop 0.25 s past a stalled sample and report `settled`).
  - `web/src/runtime/cast-bar-animation.ts`: one shared rAF loop that sleeps when every bar is settled and wakes on new samples.
  - `web/src/features/cast-bar/CastBar.tsx`: frame and nameplate variants writing only fill width and time text; `UnitFrame` (player/target/focus/team/opponents/spectator) and `Nameplates` use it; keyed by unit id so a target switch snaps. Removed the now-unused `castRemainingSeconds`.
  - Tests: `tests/domain/cast-bar.test.ts` (timeline + animator), `tests/ui/cast-bar.test.tsx` (per-frame fill without React re-renders, no backwards step, unit switch, new cast, nameplates). Docs: `docs/FRONTEND.md` "Cast bar presentation".
- **Checks Observed:**
  - `.agents/skills/iterate-game/scripts/run-critics.sh --auto --base auto/practice-hidden-tab-regen` (`smoke=false web_build=true wasm=false`): pass
    - `./scripts/check.sh`: pass (402 Rust tests, 1 pre-existing ignored; 52 Python; 42 Node; 41 Vitest files / 304 tests).
    - `cargo run -p arena-headless --locked`: pass. `npm run build`: pass (369 modules).
    - Server smoke / WASM clippy: not applicable (no crate changes).
  - `./scripts/build-web.sh`: pass (55 static resources; no browser launched).
- **Fresh-Eyes Review:** No Critical or Important findings. Addressed Minors: key `CastBar` by unit id, sleep the loop when settled, pull forward to samples ahead of the bar, removed unused export, required `label` for the frame variant, docs, loop and unit-switch tests. Not done (optional): sharing the `FrameClock` type with the cooldown animator, `transform`-based fill.
- **Learnings:** `combatEdgesDiffer` in `web/src/bridge/snapshot.ts` already delivers cast start/end without the 80 ms HUD throttle, so presentation only has to animate between edges.
- **Next Recommended Slice:** Playtest inbox — "Stop casting" keybind and Esc stops casting.

## 2026-11-22 — Stop casting keybind and Esc stops casting (Lane B, playtest inbox)
- **Branch / Commit:** `auto/stop-casting`
- **Base / PR:** `auto/smooth-cast-bars` — https://github.com/kylek-dev/arena-prototype/pull/85 (stacked on PR #84)
- **Lane:** B (pending human browser review in `docs/UI-REVIEW.md`)
- **Task:** Playtest Inbox — "Add a 'Stop casting' keybind, and make Esc stop casting"
- **Changes:**
  - Found that no client path produced `CastAction::Cancel`. `web/input.js` appends ABI field 21 (`requestStopCast`, drained once, cleared by `cancel()`); `arena-client` decodes it (older shells read as no stop), keeps it through the character close-up, and `take_stop_cast` turns it into `CastAction::Cancel` for practice and network ticks (a same-tick spell press wins).
  - Web: rebindable `stopCasting` action (unbound, "Stop casting", movement group); `isCasting`/`stopCasting`/`stopRecentlyRequested` in `runtime/commands.ts`; the input router intercepts plain Escape while casting (after binding capture/text entry, before panels/menu) via `escapeStopsCast`, with a 500 ms grace so a fast second Escape continues to the panel/menu.
  - Tests: `tests/unit/escape-order.test.mjs` (Esc order + ABI field 21), `tests/ui/menu.test.tsx` (real router: stop before menu, double Escape, settings open mid-cast, idle order, bound key), Rust `stop_casting_cancels_the_active_cast_and_is_harmless_when_idle`; updated two tests that pin the ABI length. Docs: `docs/BROWSER.md` controls + scenario 12, `docs/FRONTEND.md` defaults.
- **Checks Observed:**
  - `.agents/skills/iterate-game/scripts/run-critics.sh --auto --base auto/smooth-cast-bars` (`smoke=false web_build=true wasm=true`): pass
    - `./scripts/check.sh`: pass (403 Rust tests, 1 pre-existing ignored; 52 Python; 46 Node; 41 Vitest files / 306 tests).
    - `cargo run -p arena-headless --locked`: pass. `npm run build`: pass (370 modules). WASM clippy: pass.
    - Server smoke: not applicable (only `arena-client` among crates changed).
  - `./scripts/build-web.sh`: pass (55 static resources; no browser launched).
- **Fresh-Eyes Review:** No Critical or Important findings. Addressed Minors: stop survives the customizer close-up, narrowed the pure helper to `escapeStopsCast` (the full close/open order is exercised by the UI test), double-Escape grace, binding group, docs (rebuild WASM and frontend together).
- **Learnings:** Node 24 (CI and local) imports erasable-syntax `.ts` modules from `tests/unit/*.mjs` directly, so pure web helpers without imports can be unit-tested there. Input ABI changes break `tests/unit/input.test.mjs` and `tests/ui/team-frames.test.tsx`, which pin the array.
- **Next Recommended Slice:** Playtest inbox — the player's cast bar as its own movable frame.

## 2026-11-22 — Player cast bar as its own movable frame (Lane B, playtest inbox)
- **Branch / Commit:** `auto/player-cast-bar`
- **Base / PR:** `auto/stop-casting` — https://github.com/kylek-dev/arena-prototype/pull/86 (stacked on PR #85)
- **Lane:** B (pending human browser review in `docs/UI-REVIEW.md`)
- **Task:** Playtest Inbox — "Make the player's cast bar its own movable frame"
- **Changes:**
  - `web/src/domain/preferences/schema.ts`: `castBar` frame role (default `layout(0.5, 0.78)`) and `castBarAttached` (default `false`), both defaulted for older/imported profiles.
  - `web/src/features/cast-bar/PlayerCastBar.tsx` + `cast-bar.css`: a `Placed` frame shown only while casting, with a layout-editor sample (move/scale/hide/lock/reset through the existing editor); the live bar is pointer-transparent. `UnitFrame` gains `showCast`; `Frames` shows the player's bar only when attached and previews it there in the editor; the **Attach player cast bar to the player frame** checkbox in layout settings clears a stale `castBar` selection. Target/focus/nameplate bars unchanged.
  - Tests: `tests/ui/combat-hud.test.tsx` (default separate frame, attach/detach, idle, editor sample, legacy profiles, live vs editing class, attached preview + selection), `tests/unit/hud-css.test.mjs` (pointer-events rule). Docs: `docs/FRONTEND.md`, `docs/BROWSER.md` scenario 13.
- **Checks Observed:**
  - `.agents/skills/iterate-game/scripts/run-critics.sh --auto --base auto/stop-casting` (`smoke=false web_build=true wasm=false`): pass
    - `./scripts/check.sh`: pass (403 Rust tests, 1 pre-existing ignored; 52 Python; 47 Node; 41 Vitest files / 311 tests).
    - `cargo run -p arena-headless --locked`: pass. `npm run build`: pass (371 modules).
    - Server smoke / WASM clippy: not applicable (no crate changes).
  - `./scripts/build-web.sh`: pass (55 static resources; no browser launched).
- **Fresh-Eyes Review:** One Important finding fixed: the live bar's `Placed` wrapper swallowed pointer input over the arena; it is now pointer-transparent outside layout editing (as the live unit tooltip is). Minors addressed: documented the default gap growing with screen height (computed ~10/22/73 px at 800×600/1280×720/1920×1080), stale selection on attach, attached preview, import order. Not done: friendly labels for raw layout ids ("castBar", "unitTooltip", "bar-1") — existing pattern, queue if wanted.
- **Learnings:** Vitest stubs CSS (`?raw` CSS is empty), so stylesheet rules are asserted from `tests/unit/*.mjs` with `node:fs`. Any new `Placed` HUD element over the arena needs `pointer-events: none` when not editing, since game mouse input listens on the canvas only.
- **Next Recommended Slice:** Playtest inbox — overlapping and non-clickable buttons (custom lobby first).
- **Playtest Follow-Up (2026-11-22 on `auto/player-cast-bar`):** User saw the player cast bar flash in a different spot before appearing at its final location on each cast. Root cause: `web/src/ui/useMovable.ts` measured frame size in a passive `useEffect` (after paint) from a 260×110 placeholder, so a frame that mounts mid-game was first painted ~67 px higher (1024×768). Fixed by measuring in `useLayoutEffect`; added a `flushSync` first-paint regression test in `tests/ui/combat-hud.test.tsx` (verified to fail without the fix). Merged up into `auto/button-styles`.

## 2026-11-22 — Overlapping and non-clickable buttons (Lane B, playtest inbox)
- **Branch / Commit:** `auto/button-styles`
- **Base / PR:** `auto/player-cast-bar` (stacked on PR #86)
- **Lane:** B (pending human browser review in `docs/UI-REVIEW.md`)
- **Task:** Playtest Inbox — "Fix overlapping buttons and buttons that don't look clickable" (last inbox item; the 2026-11-22 Playtest Inbox is now empty)
- **Changes:**
  - Root cause (verified in Radix Themes 3.3.0 CSS): `variant="ghost"` buttons have no border/fill and negative margins (−8 px/−4 px at size 1), so the lobby's slot actions overlapped by ~12 px at a 4 px gap and read as text.
  - Shared `.action-button` in `web/src/styles/base.css` (28×28 px minimum hit size, no label wrapping) used with Radix `surface` for every custom-lobby slot action (rows wrap with a 6 px gap) and for the small rows elsewhere (play-online Join/Cancel, map editor Create/Cancel and Open/Keep editing, customizer Randomize/Reset). Map editor Reload file and menu Toggle diagnostics moved from ghost to surface.
  - Audit of play online/party, scoreboard, settings, keybindings, customizer, map editor and spectator HUD: plain buttons already get border/fill/hover/focus from `base.css`; added a zero-specificity pressed fill for them and a hover colour for customizer tabs; sort headers, segmented controls and framed unit entries were already fine. Only × close/back icon buttons remain ghost.
  - Tests: `tests/unit/buttons.test.mjs` (ghost reserved for close/back, robust to arrow-function props and quoting; hit size; pressed rule can't override component backgrounds), lobby slot-action class/variant assertions in `tests/ui/custom-lobby.test.tsx`. Docs: `docs/FRONTEND.md` "Buttons".
- **Checks Observed:**
  - `.agents/skills/iterate-game/scripts/run-critics.sh --auto --base auto/player-cast-bar` (`smoke=false web_build=true wasm=false`): pass
    - `./scripts/check.sh`: pass (403 Rust tests, 1 pre-existing ignored; 52 Python; 50 Node; 41 Vitest files / 311 tests).
    - `cargo run -p arena-headless --locked`: pass. `npm run build`: pass (371 modules).
    - Server smoke / WASM clippy: not applicable (no crate changes).
  - `./scripts/build-web.sh`: pass (55 static resources; no browser launched).
- **Fresh-Eyes Review:** One Important finding fixed: the first pressed rule (`button:not(.rt-Button):active…`, specificity 0,3,1) would have greyed action-bar slots and unit entries while pressed; it is now wrapped in `:where()`. Minors addressed: guard-test bypasses, mixed 24/28 px heights within rows, dead tab-hover border, UI-REVIEW note that host rows wrap to two lines. Not built: a per-slot overflow menu (wrapping satisfies "wrap or collapse").
- **Learnings:** `base.css` element rules with `:not()`/pseudo-classes reach (0,3,1) specificity and beat feature classes; new global button states belong in `:where()`. Vitest cannot see CSS, so style invariants are guarded with node source scans in `tests/unit`.
- **Next Recommended Slice:** Playtest Inbox is empty — human browser review of the six stacked PRs (#82–#87), then the Lane A observability queue.

## 2026-11-25 — Observability Phase 1: structured logs, tick histograms, door metrics, local LGTM
- **Branch / Commit:** `auto/observability-logging`
- **Base / PR:** `feat/finished-human-male-kit` (stacked on PR #89) — PR #90
- **Lane:** A (machine-verified)
- **Task:** [observability-telemetry](issues/observability-telemetry.md) Phase 1 (first Lane A backlog item); [ADR 0054](../docs/decisions/0054-observability-logs-metrics-and-local-lgtm.md)
- **Changes:**
  - `arena-server`/`arena-door` log through `tracing` (`logging.rs`: `ARENA_LOG_FORMAT=pretty|json`, `RUST_LOG`, warnings for bad values, ANSI only on a terminal) with shared structured fields; `--debug-json` wire lines under `payload`, repeated-`Hello` tickets redacted; `tests/logging.rs` proves no ticket/key/link leaks (mutation-checked twice).
  - `metrics::Histogram<N>` → `arena_tick_duration_microseconds` (`_count`/`_sum` = `arena_ticks_total`/`arena_tick_microseconds_total`); door: queued by mode, queue connections, parties, lobbies, searches by party size, tickets, ready checks started/completed/failed, accepts, declines, `arena_door_queue_wait_milliseconds` (matchmaker `Stats` + bounded found-wait buffer); `process_*` RSS/CPU/FDs and `arena_draining`.
  - `deploy/compose.yaml` `obs` profile (`grafana/otel-lgtm:0.41.1`, loopback ports, 1 GiB cap), `deploy/observability/{prometheus.yaml,dashboards.yaml,dashboards/*.json}`; JSON logs in Docker; `docs/DEPLOYMENT.md` "Observability" + dated trial.
- **Checks Observed:**
  - `.agents/skills/iterate-game/scripts/run-critics.sh --auto --base feat/finished-human-male-kit` (`smoke=true web_build=false wasm=true`): pass
    - `./scripts/check.sh`: pass (417 Rust tests, 1 pre-existing ignored; 58 Python; 50 Node; 41 Vitest files / 320 tests).
    - `cargo run -p arena-headless --locked`: pass (scripted duel, winner 0 at tick 90).
    - `./scripts/server-smoke.sh`: pass (494 ticks, 0 overruns, mean 11.7 µs, drained cleanly).
    - WASM clippy: pass. `npm run build` / `build-web.sh`: not applicable (no `web/` changes).
  - Docker `--profile obs` (twice): both targets `up`, histogram count = ticks, all 28 dashboard panel queries returned data; `lgtm` 550 MiB.
  - Release A/B, 60×3v3 bot matches × 20 s, seed 7: mean tick 8.28/8.80 µs base vs 8.43/8.22 µs branch, 0 overruns.
- **Fresh-Eyes Review:** No Critical. Important fixed: tick buckets started at 50 µs while ticks take ~8 µs (added 1–20 µs buckets); wire log `message` field collided with the event text in JSON (renamed `payload`, test parses it); party-size metric and recorded trial evidence were missing (added `arena_door_searches_total{party_size}` + panel, dated trial in `docs/DEPLOYMENT.md`). Minors fixed: buffer comment, accept/decline HELP wording (counters cannot decrement), `RUST_LOG` parse warning, ANSI only on a TTY, redacted `Hello` keeps `appearance`, shared `write_metric`, histogram bounds out of the counter cache lines.
- **Learnings:** `grafana/otel-lgtm`'s bundled Prometheus has no scrape jobs (OTLP push only): mount a full `/otel-lgtm/prometheus.yaml`. Prometheus 3 normalizes `le` labels to floats (`le="20.0"`). `arena-bot` cannot use door-issued `wss://` links, so local obs checks need a loopback override and a small WebSocket script.
- **Next Recommended Slice:** Observability Phase 2 — opt-in OTLP traces, `trace_id` in JSON logs, log shipping and alert rules.

## 2026-11-25 — Observability Phase 2: OTLP traces, log↔trace ids, shipping and alerts
- **Branch / Commit:** `auto/observability-tracing`
- **Base / PR:** `auto/observability-logging` (stacked on PR #90) — PR #91
- **Lane:** A (machine-verified)
- **Task:** [observability-telemetry](issues/observability-telemetry.md) Phase 2; [ADR 0054](../docs/decisions/0054-observability-logs-metrics-and-local-lgtm.md) amended
- **Changes:**
  - `logging::init(service)` → `Telemetry`: OTLP/HTTP span exporter only when `OTEL_EXPORTER_OTLP_ENDPOINT` is non-blank, started before and flushed after the Tokio runtime (blocking client); `control_span!` records `trace_id`/`span_id`; JSON lines carry the current span.
  - Spans: `http.request` (tower-http, matched route or `unmatched`, never the query, probes/scrapes skipped, failures at debug), `party.create`/`party.join`, per-player `queue.search` + `queue.step`s (ends on matched/stopped/left/replaced; debug-asserted bound), `lobby.start`, `ws.hello`, one `match` span with seat/spectator children (never entered while ticking), `server.drain`. Throttled process-wide anomaly warnings (`anomaly.rs`). Tokio runtime gauges.
  - `deploy/`: Alloy (`alloy.alloy`): scrape + remote-write, Docker-socket logs for this project's server/door/caddy to Loki's push API (`service_name`, lowercase `level`, `trace_id` structured metadata), OTLP trace forwarding; destinations from env. Eight alert rules + webhook contact point; Prometheus 7-day retention; `OTEL_EXPORTER_OTLP_TIMEOUT=3000`. Production topology in `docs/DEPLOYMENT.md`.
- **Checks Observed:**
  - `.agents/skills/iterate-game/scripts/run-critics.sh --auto --base auto/observability-logging` (`smoke=true web_build=false wasm=true`): pass
    - `./scripts/check.sh`: pass (423 Rust tests, 1 pre-existing ignored; 58 Python; 50 Node; 41 Vitest files / 320 tests).
    - `cargo run -p arena-headless --locked`: pass. `./scripts/server-smoke.sh`: pass (492 ticks, 0 overruns, mean 7.4 µs, drained cleanly). WASM clippy: pass.
  - Docker `--profile obs` with `OTEL_EXPORTER_OTLP_ENDPOINT=http://alloy:4318` (final run on a fresh rebuild): metrics via Alloy remote write (`up`=1 both), Loki `service_name` server/door with `trace_id` structured metadata whose id opened its `match` trace in Tempo, door and server traces present, 8 rules healthy, retention `1w`, and a firing "Front door not scraped" alert delivered to a webhook catcher 3 min 33 s after stopping the door. Details in `docs/DEPLOYMENT.md` "Observability trial".
- **Fresh-Eyes Review:** No Critical. Important fixed: a reconnecting guest leaked its `queue.search` span forever (now ended as `replaced`, test + mutation check + debug assert); an unobserved "delivered alert" claim; party create/join spans and door-side refusal warnings were missing (added; exemplars explicitly deferred in the ADR); the log→trace link needed `trace_id` as Loki structured metadata (added in Alloy). Minors fixed: tower-http failure events at debug, per-code and process-wide throttles, `suppressed_reports` naming, matched-route paths, Alloy project filter and lowercase levels, per-job "not scraped" rules, query/unknown-path/invite-code leak checks, export timeout.
- **Also found live:** a queue client closing normally was answered as `bad_request` (now a Close frame just ends the connection; logging test + mutation check).
- **Learnings:** Loki's OTLP intake replaces a Loki `service_name` label with the resource's service name: push Docker logs through `loki.write` instead. Grafana's alert notification log persists on its data volume, so recreating the container does not resend an already-notified group. The `opentelemetry-otlp` blocking HTTP client must not be built or shut down inside an async context: start telemetry before the runtime.
- **Next Recommended Slice:** Observability Phase 3 — scrubbed browser telemetry and opt-in profiling.

## 2026-11-25 — Observability Phase 3: scrubbed browser reports and opt-in profiling
- **Branch / Commit:** `auto/observability-rum`
- **Base / PR:** `auto/observability-tracing` (stacked on PR #91) — PR #92
- **Lane:** A (machine-verified; a real-browser beacon check is pending and recorded as such)
- **Task:** [observability-telemetry](issues/observability-telemetry.md) Phase 3; [ADR 0054](../docs/decisions/0054-observability-logs-metrics-and-local-lgtm.md) amended (status: all three phases implemented)
- **Changes:**
  - Door `POST /api/telemetry` (`door/browser.rs`): origin and rate checks before parsing, closed `kind`-tagged schema, 2 KB limit, bounded numbers, server-side scrub, `info` log lines (`arena_door::browser`), `arena_door_browser_reports_total{kind}` / `_refused_total{reason}` / `_rejected_total`.
  - `web/src/domain/telemetry.ts` (scrub page/text incl. URL passwords, separate error and summary budgets, DNT/GPC/remembered `?telemetry=off`, WASM panic classification, reservoir-sampled `MatchSummaryCollector`) and `web/src/runtime/telemetry.ts` (uncaught + caught failures via `start.ts`, WebGL context loss, startup time since navigation, one summary per finished match, `sendBeacon`); shared `tests/fixtures/scrub-cases.json` run by both scrubbers.
  - `profiling` feature (`pyroscope` 2.1.5 pprof, no TLS) started by `ARENA_PYROSCOPE_URL`, bounded 11 s stop, `log` bridged into `tracing` with the agent at `warn`; image built with it; `check.sh` clippies it; `stop_grace_period` 1230 s; Front Door "Browser reports" panel.
- **Checks Observed:**
  - `.agents/skills/iterate-game/scripts/run-critics.sh --auto --base auto/observability-tracing` (`smoke=true web_build=true wasm=true`): pass (`./scripts/check.sh`: 430 Rust tests, 1 pre-existing ignored, incl. `--features profiling` clippy; 58 Python; 50 Node; 42 Vitest files / 334 tests; headless pass; server smoke 496 ticks, 0 overruns, mean 13.6 µs, drained cleanly; `npm run build` 373 modules; WASM clippy pass).
  - `./scripts/build-web.sh`: pass (28 static resources; no browser launched).
  - Docker: reports scrubbed in door logs/Loki with no ticket, 422/403 refusals, Pyroscope listing `arena-server` under load, bounded profiler stop (5.7 s).
- **Fresh-Eyes Review:** No Critical. Important fixed: failures caught by `start.ts` (engine load, `onState`) never reached telemetry, and the startup timer polled forever; match summaries shared the 20-report budget. Minors fixed: scrubbers aligned on whitespace and URL passwords with shared fixtures; parse after origin/rate checks; separate refusal counters; client reports at `info`; reservoir sampling; lazy frame-time read; startup measured from navigation; remembered opt-out; compose `ARENA_PYROSCOPE_HZ`; bounded profiler stop; `log` bridge; profiling clippy in `check.sh`; counters sized by kinds; new runtime tests. Documented rather than changed: `frame_ms` is the median of recent engine frames, and a rejoin after the link ended starts a new summary.
- **Learnings:** `pyroscope` 2.x bundles its pprof backend (the separate `pyroscope_pprofrs` crate is the old API) and with default features pulls rustls/aws-lc; `default-features = false` keeps it plain HTTP. Its `stop()` waits for the current 10 s cycle. With no CPU load it uploads nothing, so Pyroscope lists no service until there is traffic.
- **Next Recommended Slice:** Accounts research & login decision (social-identity step 1).

## 2026-11-25 — Accounts research and login decision
- **Branch / Commit:** `auto/accounts-decision`
- **Base / PR:** `auto/observability-rum` (stacked on PR #92) — PR #93
- **Lane:** A (documentation only)
- **Task:** [social-identity](issues/social-identity.md) step 1
- **Changes:**
  - `docs/research/accounts-2026-11-25.md`: own OAuth/OIDC vs WorkOS, Clerk, Supabase Auth, Auth0, Ory Kratos, Keycloak and own passwords, on price at playtest and modest scale, lock-in, stored data, deletion and single-host load, with sources (third-party 2026 pricing summaries and vendor docs; vendor pages not opened directly, said so).
  - `docs/decisions/0055-accounts-oauth-login-in-arena-door.md` (Proposed): Discord/Google/GitHub via OAuth code + PKCE in an identity module beside `arena-door`; no passwords; minimal SQLite store (account id, provider subject, handle, timestamps, admin flag); own signed session cookie keyed by `ARENA_SESSION_KEY`; account id in tickets; guests first-class; deletion removes our rows; WorkOS AuthKit as the hosted fallback.
- **Checks Observed:** `./scripts/check.sh`: pass (430 Rust tests, 1 pre-existing ignored; 58 Python; 50 Node; 42 Vitest files / 334 tests). `cargo run -p arena-headless --locked`: pass (winner 0). Server smoke, web build and WASM: not applicable (docs only).
- **Fresh-Eyes Review:** No Critical. Important fixed in the ADR and the note: the door was not stateless and had no volume (a `door-data` volume, online backups and doc follow-ups are now consequences); a deleted account could keep using its cookie (sessions are checked against the database with a generation counter; ticket, backup and log retention are stated); the CSRF design was underspecified (`__Host-` cookies, required `Origin` on cookie POSTs and the queue handshake, cookie auth off under `--any-origin`, state and PKCE in a signed short-lived cookie, fixed redirects); identity keys and linking were not decided (permanent numeric ids, no auto-merge, linking out of scope, the WorkOS migration caveat); privacy and provider limits were understated (a privacy notice, Google's 100-test-user Testing mode). Minors: admin from config only, minimal scopes, sources for Google and GitHub, community-report labels, wording.
- **Learnings:** At our scale every hosted option is free, so the choice turns on lock-in, privacy and what runs next to the match loop. Supabase free projects pause after a week idle, which would lock players out between playtests.
- **Next Recommended Slice:** Identity service & signed account ids (social-identity step 2).

## 2026-11-25 — Identity service and signed account ids
- **Branch / Commit:** `auto/identity-service`
- **Base / PR:** `auto/accounts-decision` (stacked on PR #93) — PR #94
- **Lane:** A (machine-verified; the login UI is queued as Lane B)
- **Task:** [social-identity](issues/social-identity.md) step 2; [ADR 0055](../docs/decisions/0055-accounts-oauth-login-in-arena-door.md)
- **Changes:**
  - `crates/arena-server/src/identity/`: SQLite `Store` (one account per provider + permanent subject, random non-zero ids, session generation, delete), `cookie` (domain-separated HMAC session and login cookies, `__Host-` names, PKCE S256, base64url), `provider` (Discord `identify`, Google `openid`, GitHub no scope; Discord id / Google `sub` / GitHub numeric id; HTTPS via rustls + `ring`), `Identity` (store checks on every session use, `Origin` rules, admins from `ARENA_ADMIN_ACCOUNTS`).
  - `door/auth.rs`: `/api/auth/me`, `<provider>/login`, `<provider>/callback`, `logout`, `logout_everywhere`, `delete`; per-address limit; fixed redirect to the game page; login/failure/deletion metrics.
  - `ticket::Claims.account` and `ticket::issue_seats`; the queue resolves the session at the WebSocket handshake (allowed `Origin` required) and signs each seat's or spectator's account into their ticket.
  - `arena-door --identity-db`, `ARENA_SESSION_KEY`, `ARENA_OAUTH_*`; Dockerfile CA certificates and `/var/lib/arena`; Compose `door-data` volume and variables; PROTOCOL "Accounts", DEPLOYMENT "Accounts" (setup, privacy notice, backups), ARCHITECTURE.
- **Checks Observed:**
  - `.agents/skills/iterate-game/scripts/run-critics.sh --auto --base auto/accounts-decision` (`smoke=true web_build=false wasm=true`): pass (`./scripts/check.sh`: 445 Rust tests, 1 pre-existing ignored; 58 Python; 50 Node; 42 Vitest files / 334 tests; headless pass; server smoke 496 ticks, 0 overruns, mean 14.2 µs, drained cleanly; WASM clippy pass).
  - `tests/identity.rs` against a fake provider; the forged-state case was mutation-checked (it first passed for the wrong reason, because the fake's PKCE check refused it; the test now lets only the state check stop it).
  - Docker image: accounts on with a key (SQLite on the volume as `arena`, Discord redirect with the right callback), guest-only without a key despite `--identity-db`, CA bundle present, the WAL folded into the file after `docker stop`.
- **Fresh-Eyes Review:** No Critical. Important fixed:
  - Open queue connections kept signing a deleted or logged-out-everywhere account into new tickets. They now get `Command::Revoke`, with an integration test and a mutation check.
  - Backups copied only the main file of a WAL-mode database. They now copy every `identity.sqlite3*` file; the clean-stop fold was checked live.

  Minors fixed: negative tests (no `Origin` on the queue handshake, a login replayed at another provider's callback, delete without `Origin`, the login rate limit, `issue_seats` and account 0); the upgrade note for the ticket format; a Caddy log filter dropping `code` and `state` (validated with `caddy validate` and a live request); SQLite subject conflicts now error instead of looping; a session key equal to the ticket key is refused; every `Cookie` header is read; the account lookup moved after the connection limit. Not done: capping provider response bodies (fixed, trusted endpoints with a 10 s timeout).
- **Learnings:** A forged-state test can pass for the wrong reason when a later check (here the fake provider's PKCE check) also refuses; make only the check under test able to stop it, then mutation-check. A clean `docker stop` closes SQLite and folds its WAL. Caddy's `format filter` can delete query keys from `request>uri` and still writes JSON.
- **Next Recommended Slice:** `Name#1234` display names (social-identity step 3).

## 2026-11-25 — `Name#1234` display names
- **Branch / Commit:** `auto/display-names`
- **Base / PR:** `auto/identity-service` (stacked on PR #94) — PR #95
- **Lane:** A (machine-verified; HUD display and the name picker queued as Lane B)
- **Task:** [social-identity](issues/social-identity.md) step 3
- **Changes:**
  - `arena_protocol::Handle` (fixed 16-byte zero-padded name, 3–16 ASCII letters/digits starting with a letter, number 1–9999, `Name#0042` display) and `ServerMessage::Handles` (variant 5, even 2–6 seats, validated at decode); `PROTOCOL_VERSION = 9` in Rust and web, golden frames and fuzz seeds updated.
  - Tickets: `Claims.handle`, `ticket::Holder`; door naming policy (`identity/names.rs` + `blocked-names.txt`, digit-swap folding, `ARENA_BLOCKED_NAMES`), store handles (case-insensitive unique index per number, random free number with a lowest-free fallback, one rename per 7 days, free first pick and case changes, migration adding columns to older stores), `POST /api/auth/handle`, `handle`/`rename_at` in `/api/auth/me`.
  - Queue `Holder`s: account handle or a temporary `Guest#NNNN`; `Command::Rename` and `Revoke` update open searches. Match server locks a seat's handle from its first naming ticket and relays `Handles` to players and spectators. `arena-client` keeps them in local order and publishes `onHandles` → `remoteHandles` (web), with strict parsing; bot ignores them; the web queue protects variant 5 from eviction.
- **Checks Observed:**
  - `.agents/skills/iterate-game/scripts/run-critics.sh --auto --base auto/identity-service` (`smoke=true web_build=true wasm=true`): pass (`./scripts/check.sh`: 457 Rust tests, 1 pre-existing ignored; 58 Python; 50 Node; 43 Vitest files / 337 tests; headless pass; server smoke 496 ticks, 0 overruns, mean 15.0 µs, drained cleanly; `npm run build` 374 modules; WASM clippy pass). `./scripts/build-web.sh`: pass (28 static resources; no browser launched).
  - Found on the way: the "tampered session" test I wrote in the identity slice was flaky (replacing a final `0` with `0`), now always changes the digit; the palette guard reads `#1234` in comments as a colour, so web comments say `Name#number`.
- **Fresh-Eyes Review:** No Critical. Important fixed: `arena-client` never cleared relayed handles on a new connection or match, so a rejoin or a lobby-link match could show stale or no handles; they are now cleared on every new generation and Welcome for another match (unconditionally, not only when looks were relayed), with a test. Minors fixed: misplaced doc comment; docs now say the first pick starts the 7-day clock, that `None` also means "not joined yet", and that guest handles repeat; `1` is folded as both `i` and `l`; tests document the substring trade-off (`Badminton`, `Quarterstaff`, `Scunthorpe` are refused); handle endpoint tests for a missing `Origin` (403) and malformed bodies (422). Noted for the admin step: no way yet to clear a stored handle that a later blocklist covers.
- **Learnings:** The palette guard (`tests/unit/palette.test.mjs`) treats any `#` followed by 3, 4, 6 or 8 hex digits in web sources as a colour, comments included. Clear per-match client state outside "only if non-empty" guards that belong to a sibling field.
- **Next Recommended Slice:** Friends & friend invites (social-identity step 4).

## 2026-11-25 — Friends and friend invites
- **Branch / Commit:** `auto/friends`
- **Base / PR:** `auto/display-names` (stacked on PR #95) — PR #96
- **Lane:** A (machine-verified; friends UI queued as Lane B)
- **Task:** [social-identity](issues/social-identity.md) step 4
- **Changes:**
  - `identity/friends.rs`: `friends` (unordered pair, requester, accepted) and `blocks` tables beside the accounts; request (crossed requests become friendships), accept, remove (decline/withdraw/unfriend), block (ends the edge), unblock, `friends_of` with handles; caps of 50 waiting requests and 200 friends; blocks look like missing players; account deletion forgets everything. `Store::find_handle` (exactly `Name#NNNN`, case-insensitive).
  - `door/friends.rs`: `GET /api/friends` (relations, presence for friends, blocked handles) and `POST /api/friends/<action>`; `Origin` and login rules as for accounts; `pick_a_name_first`; `friends_changed` pushes (a block tells only the blocker).
  - Queue: `invite` checked in the connection task (logged in, per-account limit, friendship in the store) then delivered by the matchmaker task with the inviter's lobby or party code (`Matchmaker::invite_code`, creating a party for a solo player), `invite_sent`, `offline`/`no_such_friend` refusals; presence computed from holders, searches, lobbies and recent match tickets; `ingress::RateLimit<K>` with `RatePerAccount`; `DoorConfig::auth_burst`.
- **Checks Observed:**
  - `.agents/skills/iterate-game/scripts/run-critics.sh --auto --base auto/display-names` (`smoke=true web_build=false wasm=true`): pass (`./scripts/check.sh`: 463 Rust tests, 1 pre-existing ignored; 58 Python; 50 Node; 43 Vitest files / 337 tests; headless pass; server smoke 496 ticks, 0 overruns, mean 16.5 µs, drained cleanly; WASM clippy pass).
  - End-to-end test: request, `friends_changed` push, accept, presence online/offline/searching, one-click invite and join by code, offline/stranger/self refusals, nameless requester, blocks, request and invite spam limits, login and Origin rules.
- **Fresh-Eyes Review:** No Critical. Important fixed:
  - A blocked player could detect the block by comparing `request` (404) with `unblock`/`accept` (409) answers. Every action now answers unknown handles exactly as it would a blocker, and an unblock notifies nobody (tested pairwise).
  - A connection logged out everywhere could still invite friends from its stale copy of the account. `Command::Invite` now carries the account and the matchmaker task refuses `not_logged_in` unless the connection still holds it (test, mutation-checked).
  - Polling `GET /api/friends` could flood the single matchmaker task with O(friends × connections) scans. It now has a per-account read limit, and the task keeps an account → connections index.

  Minors fixed: the friend cap is counted for both sides; friend actions have their own per-account limit; unreadable bodies answer `bad_request`; the presence comment; spectators of lobbies that forbid them no longer count as in a match; plain-logout behaviour documented; the rate limiter comment. Tests added: a stranger never receives the code, requests carry no presence, a block sends nothing. Not done: `in_lobby`/`in_match` presence and lobby invites end to end (covered by the presence ordering code but not by a test), blocks of accounts without a handle cannot be listed or undone by handle.
- **Learnings:** "Indistinguishable" privacy claims have to hold across every endpoint that takes the same input, not just the obvious one. A connection task's copy of the account goes stale; authority belongs to the task that sees `Revoke`.
- **Next Recommended Slice:** Admin role & tag (social-identity step 5).

## 2026-11-25 — Admin role and tag
- **Branch / Commit:** `auto/admin-role`
- **Base / PR:** `auto/friends` (stacked on PR #96) — PR #97
- **Lane:** A (machine-verified; the tag display and the editor's server save queued as Lane B)
- **Task:** [social-identity](issues/social-identity.md) step 5
- **Changes:**
  - Role from `ARENA_ADMIN_ACCOUNTS` (identity step), now carried as `Claims.admin` / `Holder.admin` (never without an account; cleared on revoke) and relayed by the match server as `ServerMessage::Tags` (variant 6) with the seat's handle; `PROTOCOL_VERSION = 10` in Rust and web, golden frames, fuzz seeds, decode limits; `arena-client` keeps tags in local order, clears them per match and publishes `onTags` → `remoteTags` (strict `parseTags`); the web queue protects variant 6 from eviction.
  - `door/admin.rs`: shared `require_admin` (accounts on, allowed `Origin` for writes, logged in, admin from config) and the first tools: `GET /api/admin/maps`, `GET`/`PUT /api/admin/maps/<id>.json` (id allowlist, 256 KiB, `arena_map::parse`, canonical JSON, atomic write into `maps/` beside the account store) and `POST /api/admin/handles/clear` (`Store::clear_handle`, open searches renamed to a guest handle).
- **Checks Observed:**
  - `.agents/skills/iterate-game/scripts/run-critics.sh --auto --base auto/friends` (`smoke=true web_build=true wasm=true`): pass (`./scripts/check.sh`: 469 Rust tests, 1 pre-existing ignored; 58 Python; 50 Node; 43 Vitest files / 338 tests; headless pass; server smoke 496 ticks, 0 overruns, mean 14.2 µs, drained cleanly; `npm run build` 374 modules; WASM clippy pass). `./scripts/build-web.sh`: pass (no browser launched).
  - Door test with two door processes over one file store (accounts first, then the operator names an admin): `/me` admin flag, maps GET/PUT for admin vs non-admin vs guest vs missing `Origin`, invalid map problems, bad id, the file on disk, clearing a handle and picking again, `admin` in the admin's queue ticket only. Server test: `Tags` follows `Handles`. Client test: tags in local order.
- **Fresh-Eyes Review:** No Critical or Important findings (server gate complete, tag unforgeable, revocation clears it). Minors fixed:
  - Two saves of one map shared a temporary file and could leave corrupt JSON; each save now uses its own synced temporary file.
  - One unreadable or special file no longer fails or hangs the map list.
  - The docs say tickets already issued keep their tag until they expire, and that the tools are off under `--any-origin`.
  - Tests added: a signed admin claim without an account is refused, traversal attempts over HTTP are refused, oversize bodies get 413, and `Tags` reaches later players and spectators.

  Not done: reading PUT bodies only after the admin check (other routes allow similar sizes), and an audit trail of which admin acted (needs a store table; noted for when there are several admins).
- **Learnings:** `#[serde(default)]` is no help for postcard tickets; new claims change the format, which ships with the door and server together. Box a `Response` error once a handler returns `Result<_, Response>` (Clippy `result_large_err`).
- **Next Recommended Slice:** Chat server (the last Lane A item).

## 2026-11-25 — Chat server
- **Branch / Commit:** `auto/chat-server`
- **Base / PR:** `auto/admin-role` (stacked on PR #97) — PR #98
- **Lane:** A (machine-verified; chat box, system messages and slash commands queued as Lane B)
- **Task:** [chat](issues/chat.md), server side — the last Lane A backlog item
- **Changes:**
  - Chat over the queue WebSocket: client `chat {channel, text}`, `whisper {to, text}`, `ignore`/`unignore {handle}`; door `chat {channel, from, to?, admin, text}` with the sender and tag set by the door.
  - Routing in the matchmaker task (`door/queue.rs` `Chat`, `ChatState`): `public` (logged-in posters); `match` and `team` from the tickets the door issued (team falls back to the custom lobby, then the party); `spectator` for spectators only, who may post or whisper nowhere else; whispers to friends (checked like invites, re-checked against the connection's account). Anti-ghosting invariant: a player in a live match hears only `match` and `team` from players of that match, so second tabs, second accounts and lobby-mates cannot feed them.
  - Rules (`door/chat.rs`): cleaning and a 200-character cap, a per-connection budget (5, then one a second), abuse words masked (the word list now separates `[reserved]` names from `[abuse]`), per-connection ignore (200), account blocks loaded at hello and updated live, admin mutes (`POST /api/admin/mute`). `system` and sender fields cannot be posted.
  - Review fixes: spectator whispers and second-tab/lobby ghosting closed by the invariant; ignores and mutes keyed on account or guest id (mutes survive logout); chat keeps 8 outbox slots free for tickets; per-account public budget; 4 KiB queue frames so 200 characters fit in any script; invisible, bidi and line-separator characters stripped and combining marks capped; blocks read after hello (no lost block); seats indexed per match.
- **Checks Observed:**
  - `.agents/skills/iterate-game/scripts/run-critics.sh --auto --base auto/admin-role` (`smoke=true web_build=false wasm=true`): pass (`./scripts/check.sh`: 480 Rust tests, 1 pre-existing ignored; 58 Python; 50 Node; 43 Vitest files / 338 tests; headless pass; server smoke 496 ticks, 0 overruns, mean 15.3 µs, drained cleanly; WASM clippy pass).
  - `tests/chat.rs` (guests): door-set sender, masking, oversized/empty/`system`/guest-`public` refusals, 200 CJK and emoji characters, spoofed sender closes the connection, flood limit, ignore/unignore and unknown-handle refusals, party and lobby team chat, match/team/spectator isolation, a mid-match lobby joiner never reaching players, 2v2 team chat. `tests/identity.rs`: whispers, stranger and guest refusals, admin mute and unmute, a block silencing chat live, a spectating friend's whisper refused, a second tab's public chat and whispers never reaching live players. Spectator confinement, spectator whispers, the public filter and the lobby filter each mutation-checked.

## 2026-11-25 — Accounts on WorkOS and PostgreSQL
- **Branch / Commit:** `auto/accounts-workos`
- **Base / PR:** `auto/chat-server` (stacked on PR #98)
- **Lane:** A (machine-verified; requested by the user after the Lane A queue emptied)
- **Task:** revise [ADR 0055](../docs/decisions/0055-accounts-oauth-login-in-arena-door.md): login without a social account, PostgreSQL in its own database
- **Changes:**
  - `identity/workos.rs` replaces the three direct OAuth providers:
    - AuthKit authorize link with state and PKCE (and `?sign_up`);
    - code exchange at `/user_management/authenticate`;
    - logout link from the access token's `sid`;
    - user deletion.
  - Routes are now `/api/auth/login` and `/api/auth/callback`. `me` reports `login` instead of `providers`.
  - `identity/store.rs` and `friends.rs` moved from SQLite to PostgreSQL:
    - `tokio-postgres` with a `deadpool-postgres` pool, and numbered migrations under an advisory lock;
    - friends and blocks are deleted with their account (`ON DELETE CASCADE`);
    - picking a handle takes a per-name advisory lock, and friend actions lock both accounts' rows;
    - `rusqlite` is gone, and blocking store calls became plain `await`s.
  - The session cookie (v2) carries the WorkOS session id. Logout answers `{"logout_url"}`, and deletion removes the WorkOS user first (502 and no change if WorkOS cannot be reached).
  - Compose:
    - new `identity-db` container (PostgreSQL 18.7, private network, 256 MB, health check) that the door waits for;
    - `ARENA_DATABASE_URL`, `ARENA_WORKOS_*` and `ARENA_IDENTITY_DB_PASSWORD`;
    - admin maps move to `--maps-dir`.
  - `check.sh` starts a throwaway PostgreSQL when `ARENA_TEST_DATABASE_URL` is unset, and CI gains a PostgreSQL service.
  - Docs: ADR 0055 rewritten, research addendum (WorkOS pricing and API, read 2026-11-25), PROTOCOL, DEPLOYMENT (setup, `pg_dump` backups), ARCHITECTURE and AGENTS.
- **Checks Observed:**
  - `.agents/skills/iterate-game/scripts/run-critics.sh --auto --base auto/chat-server` with `ARENA_TEST_DATABASE_URL` unset (`smoke=true web_build=false wasm=true`): pass. `check.sh` started and removed its own `postgres:18.7-alpine`, and no test reported skipping. Results:
    - 482 Rust tests, 1 pre-existing ignored;
    - 58 Python, 50 Node, 43 Vitest files / 338 tests;
    - headless pass;
    - server smoke: 495 ticks, 0 overruns, mean 19.4 µs, drained cleanly;
    - WASM clippy pass.
  - Account tests against PostgreSQL: 25 identity unit tests (including 8 players racing for one handle) and 7 end-to-end tests with a fake WorkOS, which cover:
    - PKCE and the API key at code exchange;
    - forged, unstarted and WorkOS-refused callbacks;
    - logout links carrying the session id;
    - deletion at WorkOS first, with a 502 and no change when WorkOS refuses;
    - restarts sharing one database.
  - Live Docker Compose check (rebuilt image; `identity-db`, `server`, `door`, `caddy`; placeholder WorkOS ids):
    - `identity-db` healthy, and the door logged "accounts are on (WorkOS login, PostgreSQL store)";
    - `GET https://localhost:8443/api/auth/me` answered `{"account":null,"admin":false,"login":true,...}`;
    - `/api/auth/login` answered 303 to `api.workos.com/user_management/authorize` with `provider=authkit`, the callback `https://localhost:8443/api/auth/callback` and S256 PKCE, and set a `__Host-arena_oauth` cookie (`Max-Age=600; HttpOnly; Secure; SameSite=Lax`);
    - `psql` listed `accounts`, `blocks`, `friends` and `schema_migrations` (version 1).

    Then `docker compose down`.
  - Real WorkOS login, reported by the user on 2026-11-25. They set up a WorkOS staging application, put its client id and API key in `deploy/.env`, and logged in through the local Compose stack at `https://localhost:8443`. Before that, the door's login link got a 302 from WorkOS to the application's hosted AuthKit page. The user approved the slice. A Grafana login (anonymous access is admin, on loopback only) was discussed and deferred.


## 2026-11-25 — Account login in Play online
- **Branch / Commit:** `auto/account-ui`
- **Base / PR:** `main` (base of the Lane B social/chat stack)
- **Lane:** B (pending human review)
- **Task:** [BACKLOG](BACKLOG.md) Lane B "Log in and account status in Play online" ([social-identity](issues/social-identity.md), [ADR 0055](../docs/decisions/0055-accounts-oauth-login-in-arena-door.md))
- **Changes:**
  - `domain/account.ts` (strict `/api/auth/me` parsing; a guest is never admin or named; `logoutTarget` opens only https, or http from an http page; refusal wording), `runtime/account.ts` (fetch-injected service: refresh, logout, logout everywhere, delete; 404 re-checks login; re-check on tab visible and back/forward restore), `state/account.ts`, started in `runtime/start.ts`.
  - `features/account/AccountPanel.tsx` at the top of Play online: Log in / Sign up links, handle or "no name yet", Log out, Log out everywhere, Delete account… with in-panel confirmation and the 502 `unavailable` retry text, Privacy notice link.
  - `web/privacy.html` as a second Vite input (WorkOS holds email and login methods; the game stores the WorkOS id, name, account/login/rename times, friends and blocks; access logs and opt-out browser reports; how to delete). DEPLOYMENT tells operators to add their contact details.
- **Checks Observed:**
  - `.agents/skills/iterate-game/scripts/run-critics.sh --auto --base main` (`smoke=false web_build=true wasm=false`): pass — 482 Rust tests (1 pre-existing ignored), 58 Python, 50 Node, 45 Vitest files / 356 tests, headless pass, `npm run build` pass (emits `dist/privacy.html`).
  - Server smoke and WASM: not applicable (no crate, script or asset change).
- **Fresh-Eyes Review:** no Critical. Important: privacy notice omitted access logs, browser reports and the account/rename times (fixed); missing UI-REVIEW entry (added). Minor fixes taken: bare 502 no longer says "nothing was deleted", 404 re-checks login, http logout links only from http pages, panel kept when a follow-up refresh fails, back/forward restore and tab focus re-check, confirmation stays open after a refusal, six more negative tests.
- **Learnings:** `web/public/` is generated and gitignored (build-web.sh), so static pages go in `web/` as Vite inputs. `#` followed by 3–8 hex digits anywhere in `web/src` (even `Name#1234` in a comment) fails the palette literal test; write `Name#number`. `vite.config.ts` is type-checked without Node types, so it cannot import `node:*`.
- **Next Recommended Slice:** `Name#number` handles in the HUD and "Choose your name" (stacked on this branch).

## 2026-11-25 — `Name#number` handles everywhere
- **Branch / Commit:** `auto/handles-ui`
- **Base / PR:** `auto/account-ui` (stacked on PR #100)
- **Lane:** B (pending human review)
- **Task:** [BACKLOG](BACKLOG.md) Lane B "Show `Name#1234` handles everywhere" ([social-identity](issues/social-identity.md) step 3 display)
- **Changes:**
  - Web: `withHandles` (`domain/handles.ts`) names network fighters from `remoteHandles` in `runtime/start.ts` `onState`, so frames, tooltip, scoreboard and spectator HUD show the full handle and nameplates `shortName` (via `handleName`); seats without one keep their role name. Unit-frame names ellipsize.
  - Door: `LobbyMemberView` gains `handle` (filled from the queue's holders at send; a `#[serde(skip)] conn` carries the connection) and `Matchmaker::announce_lobbies_of` re-sends lobby views on `Rename` and `Revoke`; the web parses it and names lobby members (`memberName`, `memberLabel` for repeated guest names).
  - "Choose your name" / "Change name" in the account panel (`chooseName` posts `/api/auth/handle`; `invalid_name`, `blocked_name`, `name_full`, `too_soon` with the wait; 401/404 refresh).
- **Checks Observed:**
  - `.agents/skills/iterate-game/scripts/run-critics.sh --auto --base auto/account-ui` (`smoke=true web_build=true wasm=true`): pass — 483 Rust tests (1 pre-existing ignored; account tests ran against `check.sh`'s throwaway PostgreSQL), 58 Python, 50 Node, 45 Vitest files / 366 tests, headless pass, server smoke 496 ticks / 0 overruns / drained cleanly, `npm run build` pass, WASM clippy pass.
  - `./scripts/build-web.sh`: pass (28 static resources, WASM MIME correct, no browser).
- **Fresh-Eyes Review:** no Critical. Important: renames and revokes did not reach open lobby views (fixed with re-announce and an end-to-end test); long handles overflowed unit frames (ellipsis + title). Minor taken: `handleName` reuse, role names instead of per-viewer `Player N`, unique lobby button labels, name-form edge cases, `conn` never serialized (unit test), spectator naming test.
- **Learnings:** the matchmaker knows no names; the door's `holders` do, so name-bearing views are completed at the `Out` send site in `door/queue.rs`, and anything that changes a holder must re-announce views that show it.
- **Next Recommended Slice:** friends list and invite prompts (stacked on this branch).

## 2026-11-25 — Friends list and invite prompts
- **Branch / Commit:** `auto/friends-ui`
- **Base / PR:** `auto/handles-ui` (stacked on PR #101)
- **Lane:** B (pending human review)
- **Task:** [BACKLOG](BACKLOG.md) Lane B "Friends list and invite prompts" ([social-identity](issues/social-identity.md) step 4 UI)
- **Changes:**
  - `domain/friends.ts` (strict list parsing, grouping, presence words, refusal words), `runtime/friends.ts` (list reads spaced 3 s apart with folding and a 429 retry; request/accept/decline/remove/block/unblock), `features/friends/Friends.tsx` in Play online (Add by `Name#number`, Accept/Decline, Withdraw, Remove/Block on a second click, Unblock, Invite per friend who is on, 30 s presence poll while shown).
  - Queue: `invite`, `invite_sent`, `friends_changed` parsed; `connect()` opens a listening connection that ends quietly; `invite` (one at a time), `acceptInvite` (refusals shown at the prompt), `declineInvite`; `InvitePrompt` (right edge, non-modal, three at most, newest per friend).
  - `runtime/social.ts`: one tab (Web Lock) listens while logged in, reconnects with a backoff (2 s … 60 s) but never after `replaced`, and the friends list follows login and pushes.
- **Checks Observed:**
  - `.agents/skills/iterate-game/scripts/run-critics.sh --auto --base auto/handles-ui` (`smoke=false web_build=true wasm=false`): pass — 483 Rust tests (1 pre-existing ignored), 58 Python, 50 Node, 47 Vitest files / 382 tests, headless pass, `npm run build` pass.
  - Server smoke and WASM: not applicable (web only).
- **Fresh-Eyes Review:** no Critical. Important (all fixed): opening a second tab replaced the first tab's connection (one listening tab via a Web Lock, quiet background ends); dropped listening connections never came back (backoff reconnect, visibility retry, not after `replaced`); failed invite joins were silent (shown at the prompt); `rate_limited`/`unavailable` invite refusals had no words; presence is not pushed (30 s poll, scenario reworded). Minor taken: read spacing and 429 retry, Invite pending state, dismissible "invite sent", prompt placement and roles, PROTOCOL `refused` codes for invites.
- **Learnings:** the door keeps one queue connection per guest id and every tab shares it, so anything that connects without a player action must elect a single tab and must not show its end as an error.
- **Next Recommended Slice:** admin tag and server-backed map editor (stacked on this branch).

## 2026-11-25 — Admin tag and server-backed map editor
- **Branch / Commit:** `auto/admin-ui`
- **Base / PR:** `auto/friends-ui` (stacked on PR #102)
- **Lane:** B (pending human review)
- **Task:** [BACKLOG](BACKLOG.md) Lane B "Admin tag and server-backed map editor" ([social-identity](issues/social-identity.md) step 5 UI)
- **Changes:**
  - `withTags` (`domain/handles.ts`) marks network fighters from `remoteTags`; `ui/AdminTag.tsx` shows a gold **Admin** badge in unit frames (so the spectator HUD), on nameplates (compact, "(admin)" in the label) and on scoreboard rows (`ScoreboardRow.admin`, compared in `rowsEqual`).
  - Map editor, admins only (per `/api/auth/me`): **Save … to server** (asks before replacing a server map this one did not come from; lists `invalid_map` problems; worded 401/403/404/413/429), **Open from server…**, **Reload from server**. `mapSource` and `serverSavedText` keep server copies apart from map files, so the local **Save as …json** keeps working and the discard prompts count either save. Shared `parseMapList` and `DiscardPrompt`.
- **Checks Observed:**
  - `.agents/skills/iterate-game/scripts/run-critics.sh --auto --base auto/friends-ui` (`smoke=false web_build=true wasm=false`): pass — 483 Rust tests (1 pre-existing ignored), 58 Python, 50 Node, 47 Vitest files / 386 tests, headless pass, `npm run build` pass.
  - Server smoke and WASM: not applicable (web only).
- **Fresh-Eyes Review:** no Critical. Important (fixed): a server-opened map was treated as a map file (Reload and Open map fetched `assets/maps/<id>.json` and failed silently, or loaded a bundled copy); one `mapSavedText` meant both saves, disabling the local save after a server open. Minor taken: empty-problem 422, 413 and origin-403 wording, non-JSON list, replace confirmation, shared parsers, scoreboard `rowsEqual`, merged imports, tests.
- **Learnings:** the map editor's `mapId` names a file under `assets/maps`; any other source needs its own source marker and saved-text, or reload, open and dirty tracking quietly assume a file.
- **Next Recommended Slice:** the chat box (stacked on this branch).

## 2026-11-25 — Chat box
- **Branch / Commit:** `auto/chat-box`
- **Base / PR:** `auto/admin-ui` (stacked on PR #103)
- **Lane:** B (pending human review)
- **Task:** [BACKLOG](BACKLOG.md) Lane B "Chat box" ([chat](issues/chat.md))
- **Changes:**
  - `domain/chat.ts` (strict `chat` lines, tabs, character counting, unread/whisper/ignore helpers, refusal words and routing sets); queue requests `chat`, `whisper`, `ignore`/`unignore`.
  - Queue runtime: a separate `chat` store (200 lines); refusals go to chat for chat-only codes, or shared codes right after a chat request; a refused ignore is undone; the newest five ignores are re-sent quietly on each new connection.
  - `features/chat/ChatBox.tsx`: a `chat` layout frame (left side, above the action bars); Enter (unmodified, not repeated) opens it through the input router, releasing held keys; Enter sends, Escape leaves keeping the draft; tabs with unread counts that follow seated/spectating; whisper and ignore from a name (not one's own); admin tag on senders.
- **Checks Observed:**
  - `.agents/skills/iterate-game/scripts/run-critics.sh --auto --base auto/admin-ui` (`smoke=false web_build=true wasm=false`): pass — 483 Rust tests (1 pre-existing ignored), 58 Python, 50 Node, 49 Vitest files / 399 tests, headless pass, `npm run build` pass.
  - Server smoke and WASM: not applicable (web only).
- **Fresh-Eyes Review:** no Critical. Important (fixed): a remounted box took focus after any earlier Enter; re-sending every ignore on reconnect could trip the door's 10/s limit, showed false `no_such_player` errors and could undo the wrong name; the default position covered the action bars below ~1500 px. Minor taken: scroll pinning by newest id, Team tab when seated, whisper target hint, no menu on own lines, narrower shared-code routing, IME and Enter-repeat guards, PROTOCOL's 4 KiB message size, more negative tests.
- **Learnings:** the door answers queue requests from two places (socket-task chat checks, matchmaker-outbox refusals) and successful ignores get no reply, so a refusal can only be matched to a request by its code plus what was asked last; anything replayed on reconnect must stay inside the 10 messages/s budget.
- **Next Recommended Slice:** system messages and slash commands (stacked on this branch).

## 2026-11-25 — Slash commands and system messages
- **Branch / Commit:** `auto/chat-commands`
- **Base / PR:** `auto/chat-box` (stacked on PR #104); last of the Lane B stack
- **Lane:** B (pending human review)
- **Task:** [BACKLOG](BACKLOG.md) Lane B "System messages & slash commands" ([chat](issues/chat.md))
- **Changes:**
  - `domain/commands.ts` (parse, lookup, help from the registry) and `runtime/chat-commands.ts` (the registry: every panel, chat commands, admin-only `/mute`, `/unmute`, `/clearname` through the door's admin routes); `/queue` and `/lobby` apply the buttons' guards; `/scoreboard` reopens the last match's board (`reopenPostMatchScoreboard`).
  - A page-only `system` chat kind (never parsed from the wire), gold italic in every tab; `domain/system-events.ts` + `runtime/system-events.ts` for match found/joining, lobby and party changes, friends online/requests/accepts, disconnects/reconnects, round start and result (quiet across dropped connections and rejoins).
  - `/` (by character or numpad, when unbound) opens chat with a slash, keeping a draft; `//` sends a literal slash; a bare `/` sends nothing.
- **Checks Observed:**
  - `.agents/skills/iterate-game/scripts/run-critics.sh --auto --base auto/chat-box` (`smoke=false web_build=true wasm=false`): pass — 483 Rust tests (1 pre-existing ignored), 58 Python, 50 Node, 52 Vitest files / 415 tests, headless pass, `npm run build` pass.
  - Server smoke and WASM: not applicable (web only).
- **Fresh-Eyes Review:** no Critical. Important (fixed): `/lobby` opened over a live match and beside the customizer; `/queue <mode>` skipped `queueBlocked` and the phase check. Minor taken: no false lobby/party lines on a dropped connection, no repeated round start on reconnect, draft kept on `/`, bare `/`, layout-independent `/`, the menu's in-match/spectating definitions, admin errors worded from the body, party-leave lines, doc notes. Open for the user: `/mapeditor` is not admin-only (the menu's editor is open to all).
- **Learnings:** commands must reuse the guards of the buttons they stand for (`queueBlocked`, the menu's in-match test), or they open windows the UI never allows together.
- **Next Recommended Slice:** human browser review of the six Lane B entries (scenarios 14–19), or a critic audit to refill Lane A.

## 2026-11-25 — Resizable chat box (playtest)
- **Branch / Commit:** `auto/chat-resize`
- **Base / PR:** `auto/chat-commands` (stacked on PR #105)
- **Lane:** B (pending human review)
- **Task:** [BACKLOG](BACKLOG.md) Playtest Inbox "Chat box needs to be resizable with a minimum size"
- **Changes:**
  - `chatSize` in each profile (`CHAT_SIZE` 260×150 … 960×720, `clampChatSize`, old profiles get 360×210); `anchorAt` keeps a resized frame's top-left corner.
  - Chat box: a bottom-right grip (drag, or arrow keys) with a draft that never outgrows the screen at the frame's scale, cancels on lost capture and when layout editing starts; the log fills the height and gives way first. Width/Height sliders in the layout editor.
- **Checks Observed:**
  - `.agents/skills/iterate-game/scripts/run-critics.sh --auto --base auto/chat-commands` (`smoke=false web_build=true wasm=false`): pass — 483 Rust tests (1 pre-existing ignored), 58 Python, 50 Node, 53 Vitest files / 421 tests, headless pass, `npm run build` pass.
- **Fresh-Eyes Review:** no Critical. Important (fixed): on small windows the CSS width cap, the saved size and the effective scale disagreed, so drags slipped off the pointer. Minor taken: lost-capture cancel, draft cleared on layout editing, clipping at the minimum height, grip clear of the field.
- **Learnings:** frames are positioned by fractions of their free travel, so anything that changes a frame's size must re-anchor its layout, or it grows in both directions.
- **Next Recommended Slice:** the in-game P/F/S/C launcher and the simpler game menu (stacked on this branch).

## 2026-11-25 — In-game launcher and a simpler game menu (playtest)
- **Branch / Commit:** `auto/game-launcher`
- **Base / PR:** `auto/chat-resize` (stacked on PR #106)
- **Lane:** B (pending human review)
- **Task:** [BACKLOG](BACKLOG.md) Playtest Inbox: launcher P/F/S/C, remove lighting tweaks, no invite/join-party section and no account management in game
- **Changes:**
  - `features/launcher`: a movable `launcher` frame (P Play, F Friends, S Spells, C Customize; full tooltips, disabled states with reasons), a movable Play window (queues, custom games, practice mode and reset) and Friends window; `state/windows.ts`; Escape closes them before the menu; full-screen overlays close them.
  - Game menu: Resume, Leave/Rejoin, Home screen (outside a match), Settings, Map editor, Edit layout, Quick binding, debug diagnostics. `PlayOnline inGame` hides account, friends and party invite/join (a formed party shows with Leave party). `/queue` (`/play`) and `/friends` open the windows.
  - Lighting tweaks removed (window, menu/Graphics entries, `/lighting`, `lightingVisible`); the preset still feeds the renderer; ADR 0020 superseded; docs updated.
- **Checks Observed:**
  - `.agents/skills/iterate-game/scripts/run-critics.sh --auto --base auto/chat-resize` (`smoke=false web_build=true wasm=false`): pass — 483 Rust tests (1 pre-existing ignored), 58 Python, 50 Node, 54 Vitest files / 429 tests, headless pass, `npm run build` pass.
- **Fresh-Eyes Review:** no Critical. Important (fixed): practice had no way back to the home screen (account, name, party) — Home screen in the menu; a party member had no Leave party in game — read-only party line; Play/Friends windows stayed hidden under overlays and swallowed Escape — overlays close them and Escape skips them there. Minor taken: default positions, Friends loading state, stale docs and comments, assets/README on the preset.
- **Learnings:** moving controls out of a menu means auditing every flow that reached them through it (home screen, party leave, Escape order) — the menu was also the only route back to some places.
- **Next Recommended Slice:** human review of scenarios 14–20; decide whether friends' Invite buttons stay in game.
