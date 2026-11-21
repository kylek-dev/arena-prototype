# Two-Stage Fresh-Eyes Review Rubric

Use this rubric in **Step 7** of `iterate-game` by invoking a read-only `research`
subagent with fresh context. Passing a separate review pass catches scope drift,
duplicated logic, missing negative tests, and invariant violations before a PR opens.

---

## Subagent Prompt Template

Fill in the bracketed fields and send to a `research` subagent:

```text
Perform a strict two-stage fresh-eyes review of the current working tree and/or
branch diff (`git diff <base-branch>...HEAD` plus `git status --short` and
uncommitted `git diff`) for task: <task title & issue path>.

Stage 1 — Spec Compliance & Pitborn Invariants:
1. Check every requirement in <issue/backlog item>. Are any acceptance criteria
   partially implemented, silently deferred without a tracked follow-up, or
   out of scope?
2. Verify all AGENTS.md invariants:
   - `arena-sim` remains pure, fixed-tick, integer-based, deterministic, and free
     of graphics, OS clocks, network I/O, and runtime LLM calls.
   - Server authority: clients submit intent only; never trust client health,
     damage, cooldown, or position claims.
   - Protocol/rules version bumps and decode validation for any wire change.
   - Closed color palette (`docs/palette.json` via `scripts/palette.mjs`) for
     non-VFX colors; original IP & provenance; no secrets or raw captures.
   - Honest verification claims (no unobserved browser/network/perf claims).

Stage 2 — Code Quality, Exploit Boundaries & Regression Coverage:
1. Hunt for edge cases, integer overflow/underflow, saturating arithmetic near
   bounds, malformed wire payloads, DoS vectors, state-hash divergence, and
   reconnect/disconnect races.
2. Check for duplicated logic where an existing canonical helper already exists.
3. Verify behavioral and negative tests exist for every new rule, boundary, or
   rejected input.
4. Check docs, backlog, and iteration log consistency (no stale numbers or paths).

Report findings grouped by severity:
- Critical (must fix before commit/PR)
- Important (should fix before commit/PR)
- Minor (optional cleanup or follow-up)
Include exact file paths, line numbers, and concrete remediation for each item.
If no blocking issues exist, state "No Critical or Important findings."
```

---

## Severity Policy

| Severity | Meaning | Required Action |
| --- | --- | --- |
| **Critical** | Correctness bug, exploit/authority bypass, determinism break, missing hard invariant, or broken check | Must fix and re-run affected verification gates fresh. |
| **Important** | Missing boundary/negative test, duplicated canonical helper, misleading docs, or scope creep | Fix before commit unless explicitly out of scope (in which case queue in `planning/BACKLOG.md`). |
| **Minor** | Style polish, naming tweak, or nice-to-have cleanup | Fix if trivial and safe; otherwise ignore to avoid churn. |

---

## Verification Red Flags & Rationalization Table

Adapted from proven verification-before-completion practices. If any thought below
appears, **STOP** and run the full verification command fresh.

| Rationalization / Red Flag | Reality |
| --- | --- |
| "Should pass now after that tiny tweak." | Confidence is not evidence. Re-run the full gate fresh. |
| "It's only a comment, doc, or log edit." | Still check `git diff` and run applicable checks (lint/format/unit tests can fail on markdown/JSON/Python). |
| "Passed before I addressed the review finding." | Prior output is stale after any edit. Re-run affected gates. |
| "The subagent said everything looks clean." | Review subagents do not replace executable verification gates. |
| "That test looks flaky; skip it for now." | Forbidden shortcut. Diagnose root cause or trip the circuit breaker. |
| "CI will catch WASM or web build issues." | Run local `wasm32` clippy and `npm run build` / `./scripts/build-web.sh` whenever the toolchain is installed. |
