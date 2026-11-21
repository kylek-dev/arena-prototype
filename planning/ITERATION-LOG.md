# Autonomous Iteration Log

Append-only record of iterations run via `.agents/skills/iterate-game/SKILL.md`.
Each fresh agent session reads `## Codebase Patterns` and the latest entries here
before editing code so context compounds on disk instead of decaying in long chats.

## Codebase Patterns

Reusable repo invariants, canonical helpers, and CI traps promoted across iterations:

- **Simulation purity:** `arena-sim` must stay pure, fixed-tick, integer-based, and deterministic (no graphics, OS clocks, network I/O, or runtime LLM calls).
- **CI `browser` vs `scripts/check.sh`:** `./scripts/check.sh` checks `typecheck`, `lint`, `format:check`, and `npm test`, while `.github/workflows/ci.yml`'s `browser` job also runs `npm run build`. Run `.agents/skills/iterate-game/scripts/run-critics.sh --auto` (or `--web-build`) whenever `web/` changes.
- **CI `client-build` trigger:** `.github/workflows/client-build.yml` runs full native + WASM builds only when `crates/**`, `Cargo.toml`, `Cargo.lock`, `rust-toolchain*`, `.cargo/**`, `assets/**`, `scripts/**`, or `.github/workflows/**` change; doc/skill/planning-only PRs skip them cleanly.
- **Python unit tests in `check.sh`:** `./scripts/check.sh` automatically runs `python3 -m unittest discover -s tests/unit -p 'test_*.py'`, so Python harness/hook tests belong in `tests/unit/test_*.py`.
- **Closed color palette:** Non-VFX colors must come from `docs/palette.json` and pass `node scripts/palette.mjs --check`.
- **Canonical state hashing:** Always reuse `Arena::hash_state` instead of duplicating state-hash logic in `arena-client`.
- **Wire & snapshot boundaries:** Validate protocol limits at decode time (for example, `Welcome.round_ticks` in `1..=MAX_ROUND_TICKS`), use saturating tick/cooldown math near `u64::MAX`, and validate JS bridge integers with `Number.isSafeInteger` in `adaptSnapshot`.

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
