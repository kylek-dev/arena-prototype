# Improvement Backlog

Low-friction inbox for the self-improving iteration loop (`.agents/skills/iterate-game/SKILL.md`).
Drop one-line notes here while playtesting instead of nitpicking in chat; the loop triages
and works through items onto isolated branches (`auto/<slug>`) and opens PRs targeting
`main` (or stacked parent `auto/*` branches).

Source roadmap and full issue definitions remain in [ROADMAP.md](ROADMAP.md) and `planning/issues/`.

## Playtest Inbox (User Notes — Highest Priority)

<!-- Add raw one-line observations below while playing. Example:
- [ ] (Lane B) Camera feels too close after jumping near pillars.
- [ ] (Lane A) Reconnecting twice within 5s leaves opponent nameplate stuck greyed out.
-->

*(Empty — drop notes here anytime you playtest.)*

## Lane A — Autonomous Queue (Machine-Verified)

Ordered from [ROADMAP.md](ROADMAP.md) and automated critic opportunities:

- [ ] **Netcode measurement follow-ups** ([netcode-hardening](issues/netcode-hardening.md)) — profile `arena-server` CPU scaling, measure targeting-fairness / lag-comp disagreement rates, and evaluate snapshot bandwidth deltas/rate reductions.

## Lane B — Human-Reviewed Queue (Visual / UI / Feel)

Agent implements on a branch, runs automated checks + WASM build, and appends a `pending human review` entry to [docs/UI-REVIEW.md](../docs/UI-REVIEW.md):

- [ ] **Round clock HUD timer & time-limit result banner** ([round-clock](issues/round-clock.md)) — render remaining `MM:SS` countdown and `"Time's up · draw"` outcome using `web/src/domain/round-clock.ts` and `Snapshot.{maxRoundTicks,tickRate,roundResult}`.
- [ ] **Networked character appearance** ([networked-appearance](issues/networked-appearance.md))
- [ ] **End-of-match scoreboard** ([scoreboard](issues/scoreboard.md))
- [ ] **Private custom matches** ([custom-matches](issues/custom-matches.md))
- [ ] **Spectating** ([spectating](issues/spectating.md))

## Completed by Loop

- [x] **Round clock & time-limit result (Lane A)** ([round-clock](issues/round-clock.md)) — branch `auto/round-clock` (stacked on `auto/iterate-game-skill`), commit `c6ddc7a` (sim `RoundResult::TimeLimitDraw` + configurable `1..=MAX_ROUND_TICKS` limit & `remaining_ticks()`, protocol v2 `Welcome.round_ticks` & `WireResult::TimeLimitDraw`, server `--round-seconds` + `arena_matches_finished_total` metrics + WS e2e test, bot strict `result_matches`, client local-parity result translation & snapshot clock metadata, and pure TS `web/src/domain/round-clock.ts`).
- [x] **Server-authoritative casting, cooldowns, resources & interrupts (Lane A)** ([server-casting-resources](issues/server-casting-resources.md), [ADR 0048](../docs/decisions/0048-server-authoritative-casting-and-resources.md)) — branch `auto/server-casting-resources` (compiled & tested `casting`, `combat_state`, `talents`, `spells`, `classes`, `specs` with deterministic `BTreeMap` and removed duplicate `TICKS_PER_SECOND`; integrated 4-spell kit, GCD, per-spell cooldowns, passive resource regen, interrupts/lockout, movement/cancel fake-casting, and completion range/LoS re-checks into `Arena::advance`; bumped `PROTOCOL_VERSION = 3` / `RULES_VERSION = 4` with `WireAction`, cast/interrupt/heal events, decode invariant checks, and no-cast-prediction in `Predictor`; added server session + live WS tests, bot cast/interrupt AI, client bridge/snapshot publication, and pure TS `web/src/domain/casting.ts`).
- [x] **Netcode stress & exploit hardening (Lane A)** ([netcode-hardening](issues/netcode-hardening.md)) — branch `auto/netcode-hardening` (hardened `arena-protocol` `Welcome`/`Snapshot` decode invariants without breaking wire compatibility and added 24,000-round dual client/server mutation fuzzing + adversarial postcard OOM/varint corpus; closed pre- and post-hello WebSocket control `Ping`/`Pong` rate-limit bypasses on `/play` and `/api/queue` with live exploit/takeover tests; added simulated TCP head-of-line packet loss (`--loss-pct`), relay clumping (`--clump-ms`), browser-parity `Pacer` + `TickRate` adaptive pacing, expanded server metrics parsing, and 3 new jitter/loss/clump/reconnect stress matrix tests in `arena-bot`).
- [x] **Headless combat telemetry (Lane A)** — branch `auto/headless-combat-telemetry` (added `arena_headless::{policy, telemetry, runner}` shared deterministic bot policy, zero-network headless Arena batch runner with `--simulate`/`--matches`/`--team-size`/`--round-seconds`/`--round-ticks`/`--seed`/`--map`/`--per-match`/`--json` while preserving no-arg scripted 1D `Duel`, structured JSON `OutcomeSummary`/`DurationSummary`/`CombatMetrics`/`TickComputeSummary` with sub-µs nanosecond timing and LOS block breakdown; wired `arena-bot` to reuse canonical policy/RNG/telemetry with reconnect-safe multi-seat event deduplication and per-seat rejection counting; added `arena_tick_microseconds_total` on `arena-server` running ticks and `tick_mean_microseconds` + finished-outcome counters in `arena-bot`).
