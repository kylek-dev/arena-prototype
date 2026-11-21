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
