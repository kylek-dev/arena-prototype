# GitHub collaboration and backups

Repository: https://github.com/kylek-dev/arena-prototype (temporary name).

Code, design records, research, wiki sources and issue definitions belong in the main Git repository. GitHub issue discussion, Projects and the GitHub wiki are separate data stores; a normal code clone does not back them up.

## Local-only default

Develop and verify locally. Ordinary edits, intermediate milestones and local commits do not require a push or pull request. Keep work local until it is complete and the user requests publication; permission to edit or commit locally is not permission to publish.

When pushing completed work, always push to a new working branch and open a pull request targeting `main`, including for documentation and configuration changes. A request to push completed work authorizes this branch-and-PR workflow. Never push directly to `main` or bypass required reviews, status checks or branch protections.

Publishing the wiki, changing issues/projects and manually triggering remote workflows require separate user authorization. The commands below are reference procedures, not standing authorization to run them.

## Initial publication

The public repository, wiki, and Project are published. The authenticated account now has the required project access. Do not store tokens in this repo.

## Restore or repeat setup

1. Clone the repository and run `./scripts/check.sh`.
2. Use `planning/issues.json` and its referenced Markdown bodies to recover issue intent. `planning/github-issues.json` records published issue numbers and URLs.
3. With project permissions, run `gh project create --owner kylek-dev --title 'Arena Prototype'`, make its visibility PUBLIC, link this repository and add the published issue URLs with `gh project item-add`. Configure Todo, In Progress and Done views. Project creation/editing requires the `project` scope; the maintainer can add it with `gh auth refresh -h github.com -s project`.
4. GitHub may require the first wiki page to be created through the repository Wiki tab before its separate Git remote exists. Then clone `https://github.com/kylek-dev/arena-prototype.wiki.git`, copy `docs/wiki/*.md` into that checkout, commit and push. Wiki sources in this repository remain canonical.
5. To export issues including discussion, use the GitHub API with pagination, including each issue's comments. Keep exports under planning/exports and inspect them for sensitive content before committing. Export Project fields/items with project-scoped authentication. The initial issue snapshot alone is not a complete ongoing discussion backup.

When renaming, update origin, wiki source links, publication records and any future workflows. GitHub redirects are useful but should not be the sole migration plan.

## Verified publication status — 2026-10-31

- Public repository created, MIT license recognized, main branch pushed.
- Issues #1–#6 created; identities recorded in planning/github-issues.json.
- Wiki and Projects features enabled in repository settings.
- Public [Project #3](https://github.com/users/kylek-dev/projects/3) created and linked to this repository. All six starter issues are included. The Board view uses Status columns (Todo, In Progress, Done); an All issues table provides a second view. Identity is stored in planning/github-project.json.
- [GitHub wiki](https://github.com/kylek-dev/arena-prototype/wiki) published from docs/wiki after initial UI creation. Home and sidebar match their versioned sources. Run `./scripts/sync-wiki.sh` to publish future changes; the separate wiki Git history preserves the initial UI-created page.
- `python3 scripts/export-github.py` exports current issues/comments, selected repository settings, Project metadata, field definitions/options, items/statuses, views, and repository links to planning/exports. Run at milestones and commit reviewed changes. Exports fail if configured Project result limits would truncate data. Attachments, pull request reviews, automation settings, and wiki history need separate backups.

The original OAuth-scope and wiki-initialization blockers are resolved. Repository settings enabling Projects are separate from creating the board itself.

Initial GitHub Actions CI passed: https://github.com/kylek-dev/arena-prototype/actions/runs/41241261338. Local export verified six issues with comment arrays. The export script supports the installed older gh CLI without --slurp.

## Recovery from the committed exports

`planning/exports/project.json` records the public Project description/readme, field definitions and option names, issue URLs and statuses, and Board/table layouts. To recreate it, create a new public Project, link the repository, recreate fields/options by name, add the exported issue URLs, assign each exported status, and recreate the recorded views. GitHub IDs change on recreation: map by field/option name and issue URL, never reuse old IDs blindly. This is a documented manual recovery procedure, not an automated importer. Initial verification checked six exported issue URLs against all six Project items, all three Status options, public visibility, and Status grouping of the Board.

For wiki recovery, initialize a replacement wiki and run `./scripts/sync-wiki.sh` from a clone of the main repo. This restores the maintained page content, not the entire wiki edit history. To preserve edit history as well, separately mirror the wiki remote with `git clone --mirror https://github.com/kylek-dev/arena-prototype.wiki.git` into your backup storage.

## Repository security — verified 2026-11-01

`main` requires a pull request, one write-access approval, approval of the latest reviewable push, dismissal of stale approvals, resolved review conversations, and up-to-date `rust`, `browser`, and `client-build` checks from the GitHub Actions app (15368). The `rust` job lints/tests rules and the headless runner; `browser` checks JavaScript syntax and input unit tests. The `client-build` gate requires successful native/WASM builds when Rust/dependency/build inputs change, or successful detection of an unrelated change with both builds intentionally skipped (ADR 0006). Manual dispatch always requests full builds. E2E review is manual (ADR 0004). The full-build workflow always reports the gate, avoiding pending required checks on unrelated PRs. Rules apply to admins; force pushes and branch deletion are blocked. Your own PR needs another collaborator's approval. CODEOWNERS requests owner attention for sensitive paths but is not an additional merge requirement.

Actions has read-only default tokens and cannot approve PRs. Only GitHub-owned actions and `Swatinem/rust-cache` are allowed, with full commit SHA pinning required. All external fork contributors require workflow approval. Review their workflow and executable changes before approving a run. CI disables persisted checkout credentials and limits fast CI jobs to 10 minutes and full client-build jobs to 30 minutes. Changes to the action allowlist require an admin settings update as well as a reviewed workflow change.

Secret scanning and push protection are enabled. Dependabot alerts and security updates and private vulnerability reporting are enabled. Weekly Cargo, npm and Actions update PRs are configured in `.github/dependabot.yml`; dependency updates still require review and CI. Merged branches are automatically deleted. See [SECURITY.md](../SECURITY.md) for private reporting and credential incident guidance.

Verify live settings with `gh api repos/kylek-dev/arena-prototype/branches/main/protection`, `/actions/permissions`, `/actions/permissions/selected-actions`, `/actions/permissions/workflow`, `/actions/permissions/fork-pr-contributor-approval`, `/private-vulnerability-reporting`, and `/automated-security-fixes` (append each suffix to the same repository API path). `gh api repos/kylek-dev/arena-prototype/vulnerability-alerts` returns HTTP 204 when alerts are enabled. Inspect repository `security_and_analysis` for secret scanning settings. These server settings are not restored by a Git clone or by merging this documentation.

Account follow-up: each collaborator should enable strong 2FA or passkeys, protect recovery codes, and periodically review personal tokens, SSH keys and authorized apps. Repository settings cannot enforce 2FA for collaborators on this personal-account repository. Keep write access limited to trusted contributors; admins can still edit protection settings. Required checks attest to the workflow in the PR, so workflow changes need careful human review.
