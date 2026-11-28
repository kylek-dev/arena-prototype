# PR, Stacked PR, UI Review & Iteration Log Templates

Use these templates in **Step 8** of `iterate-game` to keep PRs, stacked branches,
UI review gates, and cross-session memory consistent.

---

## 1. Standalone PR Description Template

Title style: Conventional Commit (`feat(scope): summary`, `fix(scope): summary`,
`perf(scope): summary`, `chore(scope): summary`, `docs(scope): summary`).

```markdown
## Summary

<1-3 sentences explaining what changed and why.>

- **Task / Issue:** <link to `planning/issues/<name>.md` or `planning/BACKLOG.md` item>
- **Lane:** `A (machine-verified)` | `B (pending human browser/UI review)`

### Changes

- **`<crate or path>`:** <concise bullet of concrete behavioral changes>

### Verification

All commands below were executed fresh after the final commit on this branch:

- `./scripts/check.sh`: pass (<exact key counts, e.g., Rust tests, Python tests, Vitest>)
- `cargo run -p arena-headless --locked`: pass (<observed output>)
- `cargo build -p arena-world -p arena-bot --locked && ./scripts/server-smoke.sh`: pass | not applicable (unchanged crates)
- `npm run build`: pass | not applicable (unchanged `web/`)
- `cargo clippy -p arena-client --target wasm32-unknown-unknown --locked -- -D warnings`: pass | not applicable | toolchain unavailable
- `./scripts/build-web.sh`: pass | not applicable | toolchain unavailable

### Fresh-Eyes Review

- <Key Critical/Important findings addressed, or "Read-only two-stage review found no Critical or Important issues.">

<!-- Include for Lane B PRs only: -->
### Human Review Gate

- **Pending human browser/UI review:** Recorded in `docs/UI-REVIEW.md`. No browser was launched and no rendered/visual pass is claimed from automated builds alone.
```

---

## 2. Stacked PR Additions

When stacking `auto/<child>` on top of an unmerged `auto/<parent>`:

1. Create the child branch directly from the parent branch:
   ```sh
   git checkout -b auto/<child> auto/<parent>
   ```
2. Open the child PR targeting the parent branch:
   ```sh
   gh pr create --base auto/<parent> --head auto/<child> --title "..." --body "..."
   ```
3. Include a **Stack** block at the top of **both** PR descriptions:
   ```markdown
   ### Stack

   1. Base PR: #<base-pr-number> (`auto/<parent>` -> `main`) — <short title>
   2. Child PR: #<child-pr-number> (`auto/<child>` -> `auto/<parent>`) — <short title>

   Merge **#<base-pr-number>** first, then retarget **#<child-pr-number>** to `main`
   (GitHub does this automatically when the base branch is deleted on merge) and
   rebase if `main` advanced before merging.
   ```

---

## 3. `planning/ITERATION-LOG.md` Entry & Pattern Promotion

Append new entries to the bottom of `planning/ITERATION-LOG.md` (never overwrite
history):

```markdown
## YYYY-MM-DD — <short title>
- **Branch / Commit:** `auto/<slug>` (`<short-sha>`)
- **Base / PR:** `<main or auto/parent>` — <PR URL or explicit blocker>
- **Lane:** A (machine-verified) | B (pending human review)
- **Task:** <link to issue or backlog item>
- **Changes:**
  - <1-3 concise bullets>
- **Checks Observed:**
  - `./scripts/check.sh`: pass/fail (<brief observed evidence>)
  - `cargo run -p arena-headless --locked`: pass/fail
  - `./scripts/server-smoke.sh`: pass/fail/not-applicable
  - `npm run build`: pass/fail/not-applicable
  - WASM clippy & `./scripts/build-web.sh`: pass/fail/not-applicable/unavailable
- **Fresh-Eyes Review:** <key findings addressed, or "no Critical/Important findings">
- **Learnings:** <gotchas or reusable invariants discovered>
- **Next Recommended Slice:** <single bounded follow-up>
```

**Promoting to `## Codebase Patterns`:**
If an iteration uncovers a generalizable trap, canonical helper, or CI requirement
that future stateless sessions should know before editing (for example, a shared hash
function, protocol decode rule, or CI gate mismatch), add a concise 1-line bullet to
`## Codebase Patterns` at the **top** of `planning/ITERATION-LOG.md`.

---

## 4. `docs/UI-REVIEW.md` Lane B Entry Template

Append under `## Subsequent changes` when touching rendered visuals, camera, lighting,
animations, HUD layout, or combat feel:

```markdown
YYYY-MM-DD: **pending human review.** <Concise summary of rendered/UI changes and
issue/ADR link.> Automated checks passed locally (<exact commands run>); no browser
was launched and no rendered, visual, or networked pass is claimed.
```
