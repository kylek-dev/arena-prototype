# GitHub collaboration and backups

Repository: https://github.com/kylek-dev/arena-prototype (temporary name).

Code, design records, research, wiki sources and issue definitions belong in the main Git repository. GitHub issue discussion, Projects and the GitHub wiki are separate data stores; a normal code clone does not back them up.

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
