---
name: iterate-game
description: >-
  Runs one bounded, self-verifying autonomous iteration or critic audit on Pitborn
  (Arena Prototype). Use when the user asks to self-improve the game, run an
  autonomous iteration loop, pick up the next roadmap or backlog slice, run a
  critic audit, or optimize a measurable sim/server metric. Not for one-off Q&A,
  ad-hoc file explanations, direct merges, or open-ended multi-epic rewrites.
---

# Pitborn Self-Improving Iteration Loop

Run **one small, bounded, revertible slice** per iteration. Never attempt open-ended
"improve everything" refactors, never weaken tests or linters to pass a gate, and
never claim an unobserved browser, network, or performance pass.

All project rules in [AGENTS.md](../../../AGENTS.md) remain binding.

## Iron Laws

1. **Verification before completion:** `NO COMPLETION CLAIMS WITHOUT FRESH VERIFICATION EVIDENCE.`
   Run the full verification command after the final edit, read full output and exit
   code, and report exact observed counts. Never extrapolate from an earlier run or a
   subagent's summary.
2. **Root cause before patching:** When any check fails, reproduce and trace the root
   cause before editing. Never apply blind symptom patches or relax assertions.
3. **Search before assuming missing:** Inspect existing crates, helpers, and tests
   before writing new abstractions. Reuse canonical sources of truth (for example,
   `Arena::hash_state`) rather than duplicating logic.
4. **Clean state handoff:** Every completed or circuit-broken iteration must leave a
   clean, merge-ready working tree and an up-to-date on-disk record in
   `planning/ITERATION-LOG.md` so the next stateless session can resume immediately.

## Iteration Modes

- **Build iteration (default):** Pick 1 bounded task, implement on an isolated
  `auto/<slug>` branch, verify all applicable gates, run a two-stage fresh-eyes
  review, commit, push, open a PR (targeting `main` or a stacked parent `auto/*`
  branch), and log.
- **Critic audit mode:** When asked to audit or when no clear task is queued, run
  [run-critics.sh](./scripts/run-critics.sh), inspect codebase/roadmap gaps, and add
  concrete, actionable items to [planning/BACKLOG.md](../../../planning/BACKLOG.md)
  without editing production code.
- **Experiment / Perf mode:** For measurable sim/server/bot optimizations, define an
  immutable baseline command before editing, measure before and after on the same
  workload, and **keep** only when the metric measurably improves with all
  correctness gates green; otherwise **discard** cleanly and log the hypothesis.

---

## Step-by-Step Runbook

### 1. Orient & Enforce Budgets
1. Run [orient.sh](./scripts/orient.sh) to inspect working-tree cleanliness, branch
   state, open `auto/*` PRs, `## Codebase Patterns` at the top of
   [planning/ITERATION-LOG.md](../../../planning/ITERATION-LOG.md), and the top of
   [planning/BACKLOG.md](../../../planning/BACKLOG.md).
2. If uncommitted edits from an unrelated task exist, stop and report them rather
   than mixing changes.
3. Read [planning/ROADMAP.md](../../../planning/ROADMAP.md) when picking roadmap work.

### 2. Select & Classify Exactly One Bounded Task
Pick the highest-priority task in this order:
1. **User playtest notes** in `planning/BACKLOG.md` under `## Playtest Inbox`.
2. **Next roadmap issue** in `planning/ROADMAP.md` (`planning/issues/*.md`).
3. **Critic discovery:** Run [run-critics.sh](./scripts/run-critics.sh) and pick the
   highest-severity defect, missing exploit/boundary test (`arena-sim` /
   `arena-protocol`), netcode stress edge case (`arena-world` / `arena-bot`), or
   architecture invariant gap.

Classify and right-size before editing:
- **Lane A (Autonomous / Machine-Verified):** `arena-sim`, `arena-protocol`,
  `arena-world`, `arena-bot`, `arena-headless`, deterministic JS/TS domain & unit
  tests, static asset/palette checks, harness/CI hardening.
- **Lane B (Human-Reviewed Output):** `arena-client` rendering, camera, animations,
  lighting, HUD layout/visuals, combat feel. Build and verify locally, then append a
  dated **pending human review** entry to [docs/UI-REVIEW.md](../../../docs/UI-REVIEW.md).
- **Right-sizing check:** The slice must fit comfortably in one focused PR. If an
  issue spans both core rules/protocol and rendered UI, ship Lane A first and queue
  the Lane B follow-up in `planning/BACKLOG.md`.

### 3. Branch Isolation (Standalone or Stacked)
- Never commit or push directly to `main`.
- **Standalone PR (default):** Start on a fresh branch off `main`:
  `git checkout -b auto/<short-slug> main` (or `origin/main` when synced).
- **Stacked PR:** When intentionally building on top of an unmerged `auto/<parent>`
  branch, branch from that parent (`git checkout -b auto/<short-slug> auto/<parent>`)
  and target `--base auto/<parent>` when opening the PR. Cross-link the stack in
  both PR descriptions (see [pr-and-log-templates.md](./references/pr-and-log-templates.md)).
- Only reuse an existing `auto/<slug>` branch when continuing an open PR for the
  same task. Never force-push shared history or bypass required reviews, status
  checks, or branch protections.

### 4. Spec, Existing Code & Invariant Check
Before editing code:
1. Read the relevant issue file and affected sections of
   [docs/GAME.md](../../../docs/GAME.md), [docs/ARCHITECTURE.md](../../../docs/ARCHITECTURE.md),
   [docs/PROTOCOL.md](../../../docs/PROTOCOL.md), and any referenced ADR in
   `docs/decisions/`.
2. Search the target crates/modules thoroughly (**don't assume not implemented**) so
   existing helpers, wire types, and tests are reused rather than duplicated.
3. Verify the planned change preserves all core Pitborn invariants:
   - `arena-sim` stays pure, fixed-tick, integer-based, and independent of graphics,
     OS clocks, network I/O, and runtime LLM calls.
   - Server authority: clients submit intent only; never trust client health, damage,
     cooldown, or position claims.
   - Closed color palette: all non-VFX colors come from `docs/palette.json` via
     `scripts/palette.mjs`.
   - Original IP & recorded provenance for any art/assets; no secrets or raw captures.
   - Significant architectural or rule choices get a numbered `Proposed` ADR under
     `docs/decisions/`.

### 5. Implement + Behavioral Tests + Circuit Breaker
- Keep diffs tight and single-purpose.
- Add behavioral tests covering game rules, exploit boundaries, replay determinism,
  malformed wire inputs, or frontend domain/runtime behavior.
- **Forbidden shortcuts:** Never delete/skip existing tests, relax `clippy -D warnings`,
  pass `--no-verify`, bypass `scripts/palette.mjs --check`, or disable validation.
- **Multi-signal circuit breaker:**
  - Cap targeted root-cause fixes per failing verification gate at **3 attempts**.
  - Stop early after **2 identical repeated errors** with no measurable diff progress.
  - Cap review-fix cycles after Step 7 at **2 rounds**.
  - On trip: revert or stash only the broken uncommitted edits cleanly (never run
    destructive resets over unrelated work), confirm a clean tree, record the exact
    error, hypotheses tried, and blocker in `planning/ITERATION-LOG.md`, and halt.

### 6. Run Hard Verification Gates (Sequential Backpressure)
Run heavy build/test gates **sequentially from one executor** (parallel read-only
searches are fine; parallel Cargo builds cause lock and port contention).

Run the auto-detected gate suite against the PR base branch:
```sh
.agents/skills/iterate-game/scripts/run-critics.sh --auto --base <base-branch>
```
Exact gate matrix (matches `.github/workflows/ci.yml` & `client-build.yml`):
- **Always:** `./scripts/check.sh` and `cargo run -p arena-headless --locked`.
- **When sim, protocol, server, bot, map, palette, or root Cargo manifests changed:**
  `cargo build -p arena-world -p arena-bot --locked && ./scripts/server-smoke.sh` (`--smoke`).
- **When `web/`, `package*.json`, `vite.config.ts`, `tsconfig.json`, or `docs/palette.json` changed:**
  `npm run build` (`--web-build`).
- **When any `scripts/client-build-needed.sh` input (`crates/**`, `**/Cargo.*`, `rust-toolchain*`, `.cargo/**`, `assets/**`, `scripts/**`, `.github/workflows/**`) changed:**
  `cargo clippy -p arena-client --target wasm32-unknown-unknown --locked -- -D warnings` (`--wasm`),
  and `./scripts/build-web.sh` when `arena-client` or `web/` changed. If the WASM
  toolchain is missing in the current environment, record that explicitly rather
  than claiming a pass.
- Do **not** run Playwright automatically as a dev/CI gate (per `AGENTS.md`).

**Verification Gate Function (mandatory before any pass claim):**
1. **Identify** the exact command proving the claim.
2. **Run** it fresh after the final edit.
3. **Read** the full output and exit code.
4. **Confirm** zero failures/regressions (and check the Red Flags table in
   [review-rubric.md](./references/review-rubric.md)).
5. **Only then** state the claim with exact observed counts.

### 7. Two-Stage Fresh-Eyes Adversarial Review
Before committing, invoke a read-only `research` subagent (fresh context) using the
prompt and checklist in [references/review-rubric.md](./references/review-rubric.md):
- **Stage 1 — Spec Compliance & Pitborn Invariants:** Verify every acceptance criterion
  is met with no silent shortcuts or scope creep, and all `AGENTS.md` invariants hold.
- **Stage 2 — Code Quality, Exploit Boundaries & Regression Tests:** Check edge cases,
  integer overflow/DoS/malformed-wire vectors, replay determinism, and negative tests.

Address all valid Critical and Important findings, then **re-run all affected
verification gates fresh** (Iron Law 1).

### 8. Checkpoint, Open PR & Log (Clean State Handoff)
Use the templates in [references/pr-and-log-templates.md](./references/pr-and-log-templates.md):
1. Update `planning/BACKLOG.md` (check off completed item, add discovered follow-ups)
   and `planning/ROADMAP.md` if an issue status changed.
2. If Lane B, add a dated `pending human review` note to `docs/UI-REVIEW.md`.
3. Append a structured entry to `planning/ITERATION-LOG.md` and promote any
   generally reusable repo discovery into `## Codebase Patterns` at the top.
4. Stage only task-scoped files and commit atomically on `auto/<slug>`.
5. Push the branch and open (or update) the PR targeting `main` (or the parent
   `auto/<parent>` branch for stacked PRs):
   ```sh
   git push -u origin auto/<slug>
   gh pr create --base <base-branch> --head auto/<slug> --title "<type>(<scope>): <summary>" --body "..."
   ```
   - Leave the PR open for required human review and GitHub Actions checks (`rust`,
     `browser`, `client-build`); never merge directly or bypass branch protections.
   - If push or `gh pr create` fails (offline/auth), keep the local commit intact,
     record the blocker honestly in `planning/ITERATION-LOG.md`, and report it.
6. Confirm `git status --short` is clean before ending the iteration.
