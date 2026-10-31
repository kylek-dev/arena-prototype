# GitHub collaboration and backups

Repository: https://github.com/kylek-dev/arena-prototype (temporary name).

Code, design records, research, wiki sources and issue definitions belong in the main Git repository. GitHub issue discussion, Projects and the GitHub wiki are separate data stores; a normal code clone does not back them up.

## Initial publication

Publication and remaining access limitations are recorded below after verification. The current token has repository/workflow access but lacks Projects scopes. Do not store tokens in this repo.

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
- GitHub Project board is **not created**: the active token lacks project permissions. The roadmap and issue definitions are committed; issue #6 tracks completion.
- GitHub wiki is **not published yet**: cloning its separate remote returns Repository not found until initial wiki creation. Create a first Home page at https://github.com/kylek-dev/arena-prototype/wiki, then run `./scripts/sync-wiki.sh`. All intended pages are already public under docs/wiki.
- `python3 scripts/export-github.py` exports current issues, comments and selected repository settings to planning/exports. Run at milestones and commit reviewed changes. This does not back up attachments, pull request reviews, project fields or wiki history; those require separate exports/clones.

The authentication blocker is a missing OAuth scope, not a request for task approval. Repository settings enabling Projects do not create a Project board.
