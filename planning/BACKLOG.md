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

- [ ] **Round clock & time-limit result (Lane A)** ([round-clock](issues/round-clock.md)) — authoritative `arena-sim` round-limit draw, protocol clock metadata, server limit & match-result metrics, bot verification, and pure TS round-clock domain helpers.
- [ ] **Server-authoritative casting, cooldowns, resources & interrupts** ([server-casting-resources](issues/server-casting-resources.md)) — `arena-sim`, `arena-protocol`, `arena-server`, `arena-bot` tests.
- [ ] **Netcode stress & exploit hardening** ([netcode-hardening](issues/netcode-hardening.md)) — expand `arena-bot` jitter/drop/reconnect matrices and `arena-protocol` malformed-packet fuzz tests.
- [ ] **Headless combat telemetry** — extend `arena-headless` / `arena-bot` to emit structured JSON match metrics (duration, draw rate, LOS blocks, tick compute time) for objective balance tracking.

## Lane B — Human-Reviewed Queue (Visual / UI / Feel)

Agent implements on a branch, runs automated checks + WASM build, and appends a `pending human review` entry to [docs/UI-REVIEW.md](../docs/UI-REVIEW.md):

- [ ] **Networked character appearance** ([networked-appearance](issues/networked-appearance.md))
- [ ] **End-of-match scoreboard** ([scoreboard](issues/scoreboard.md))
- [ ] **Private custom matches** ([custom-matches](issues/custom-matches.md))
- [ ] **Spectating** ([spectating](issues/spectating.md))

## Completed by Loop

*(None yet.)*
