# Autonomous iteration skills survey

Investigated 2026-11-21 to design and harden Pitborn's `.agents/skills/iterate-game/` workflow and `.agents/hooks/` guardrails. Findings below synthesize public open-source agent skills and engineering harnesses; no upstream code was copied verbatim.

## Public sources reviewed

| Project / Reference | Core focus | Key mechanisms |
| --- | --- | --- |
| `obra/superpowers` (`verification-before-completion`, `systematic-debugging`, `subagent-driven-development`, `finishing-a-development-branch`, `requesting-code-review`) | Disciplined engineering skills & verification gates | Explicit "Iron Laws", 5-step verification gate function, rationalization/red-flag tables, root-cause-first debugging, two-stage review (spec compliance first, then code quality), clean branch completion checklist. |
| `snarktank/ralph` & Geoffrey Huntley's Ralph Wiggum methodology (`how-to-ralph-wiggum`) | Stateless iterative loops ("disk is memory") | Fresh context per slice, top-of-file `## Codebase Patterns` section in the progress log so learnings compound across sessions, "don't assume not implemented" search-first rule, right-sized single-window tasks, and sequential single-writer backpressure for heavy builds/tests. |
| `frankbria/ralph-claude-code` | Autonomous loop safety & stall detection | Multi-signal circuit breaker (no-progress streak, repeated identical error threshold, permission denial threshold) with clean halt instead of infinite retry loops. |
| `karpathy/autoresearch` & public autoresearch `SKILL.md` skills | Autonomous optimization loops | Hypothesis -> bounded edit -> immutable evaluation metric -> keep on improvement / discard on regression -> append-only experiment log. |
| Anthropic — *Effective Harnesses for Long-Running Agents* | Multi-session continuity | Initializer/orientation script (`init.sh`), structured on-disk progress artifacts, one bounded feature per session, and **Clean State Handoff** (every session ends merge-ready with no stray edits). |
| `mgechev/skills-best-practices` & Antigravity `agy-customizations` | `SKILL.md` token budget & routing | Three-tier progressive disclosure (frontmatter metadata -> concise `<500`-line `SKILL.md` -> on-demand `references/` & executable `scripts/`), third-person descriptions with explicit positive and negative triggers. |

## What we adopted for Pitborn

- **Progressive disclosure:** Kept `.agents/skills/iterate-game/SKILL.md` concise (~185 lines) and moved detailed subagent review prompts/rubrics and PR/stacked-PR/log templates into `references/review-rubric.md` and `references/pr-and-log-templates.md`.
- **Verification & debugging Iron Laws:** Embedded the 5-step Verification Gate Function and Red Flags / Rationalization table so fresh command evidence is mandatory after the final edit, plus root-cause-first debugging before retrying any failing gate.
- **Compounding disk memory & deterministic orientation:** Added `## Codebase Patterns` at the top of `planning/ITERATION-LOG.md` and created `.agents/skills/iterate-game/scripts/orient.sh` to report git hygiene, branch sync, open `auto/*` PRs, patterns, and backlog queues in one read-only pass.
- **CI-aligned sequential critics:** Upgraded `scripts/run-critics.sh` with `--auto --base <ref>` to run `./scripts/check.sh`, `arena-headless`, and automatically trigger `--smoke`, `--web-build` (`npm run build`, matching CI's `browser` job), and `--wasm` based on touched paths.
- **Two-stage fresh-eyes review:** Split read-only `research` subagent review into Stage 1 (Spec Compliance & Pitborn Invariants) and Stage 2 (Code Quality, Exploit Boundaries & Regression Coverage).
- **Multi-signal circuit breaker & Experiment mode:** Combined 3-attempt root-cause cap, 2-identical-error early stop, 2-round review cap, and optional keep/discard metric discipline for sim/server/bot performance tasks.
- **Tested hook guardrails:** Hardened `.agents/hooks/guard-commands.py` against `--no-verify` hook bypasses and `gh pr merge --admin` protection bypasses while keeping clean `auto/*` branch pushes allowed, and added `tests/unit/test_agent_hooks.py` so `./scripts/check.sh` verifies both hooks.

## What we left behind (Pitborn constraints)

- **No blind auto-merge or direct pushes to `main`:** `AGENTS.md` and GitHub branch protection require PRs, human review, and passing `rust`, `browser`, and `client-build` checks. Autonomous runs stop after opening/updating the PR.
- **No unconditional `git reset --hard`:** Unattended hard resets risk destroying unrelated local work; destructive resets remain gated by `guard-commands.py`.
- **No automated Playwright browser gate:** Per `AGENTS.md` and ADR 0004, rendered browser/UI checks remain a human review gate tracked in `docs/UI-REVIEW.md`.
- **No runtime LLM dependencies in combat:** AI tooling stays strictly offline/development-side; `arena-sim` remains pure, deterministic, integer-based Rust.

## Primary sources

- [obra/superpowers](https://github.com/obra/superpowers)
- [snarktank/ralph](https://github.com/snarktank/ralph)
- [frankbria/ralph-claude-code](https://github.com/frankbria/ralph-claude-code)
- [karpathy/autoresearch](https://github.com/karpathy/autoresearch)
- [mgechev/skills-best-practices](https://github.com/mgechev/skills-best-practices)
- [Anthropic Engineering — Effective Harnesses for Long-Running Agents](https://www.anthropic.com/engineering/effective-harnesses-for-long-running-agents)
- [Geoffrey Huntley — Ralph Wiggum Methodology](https://ghuntley.com/ralph/)
