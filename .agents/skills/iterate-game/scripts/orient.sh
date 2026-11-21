#!/usr/bin/env bash
# Deterministic read-only orientation snapshot for the iterate-game skill.
# Prints git hygiene, branch sync state, open auto/* PRs (when gh is available),
# top Codebase Patterns + latest iteration log entry, and top backlog queues.
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root"

branch="$(git branch --show-current 2>/dev/null || true)"
branch="${branch:-DETACHED}"
head_sha="$(git rev-parse --short HEAD 2>/dev/null || echo 'unknown')"

echo "=== GIT STATE ==="
echo "Repo:   $repo_root"
echo "Branch: $branch ($head_sha)"

status_out="$(git status --short)"
if [[ -z "$status_out" ]]; then
  echo "Tree:   clean"
else
  echo "Tree:   DIRTY (inspect before starting a new iteration)"
  printf '%s\n' "$status_out"
fi

if git rev-parse --verify main >/dev/null 2>&1; then
  if counts="$(git rev-list --left-right --count main...HEAD 2>/dev/null)"; then
    read -r behind ahead <<<"$counts"
    echo "vs main:        +${ahead} ahead / -${behind} behind"
  fi
fi
if git rev-parse --verify origin/main >/dev/null 2>&1; then
  if counts="$(git rev-list --left-right --count origin/main...HEAD 2>/dev/null)"; then
    read -r obehind oahead <<<"$counts"
    echo "vs origin/main: +${oahead} ahead / -${obehind} behind"
  fi
fi

echo ""
echo "=== RECENT COMMITS ==="
git log --oneline -n 5

if command -v gh >/dev/null 2>&1; then
  echo ""
  echo "=== OPEN AUTO/* PRS ==="
  if open_prs="$(gh pr list --state open --json number,title,headRefName,baseRefName --jq '.[] | select(.headRefName | startswith("auto/")) | "#\(.number) [\(.headRefName) -> \(.baseRefName)] \(.title)"' 2>/dev/null)"; then
    if [[ -n "$open_prs" ]]; then
      printf '%s\n' "$open_prs"
    else
      echo "(none)"
    fi
  else
    echo "(gh unavailable or offline)"
  fi
fi

if [[ -f planning/BACKLOG.md ]]; then
  echo ""
  echo "=== BACKLOG (Playtest Inbox & Lane A) ==="
  awk '
    /^## Playtest Inbox/ { show=1 }
    /^## Lane B/ { show=0 }
    show { print }
  ' planning/BACKLOG.md
fi

if [[ -f planning/ITERATION-LOG.md ]]; then
  echo ""
  echo "=== CODEBASE PATTERNS ==="
  awk '
    /^## Codebase Patterns/ { show=1; print; next }
    show && /^## / { show=0 }
    show { print }
  ' planning/ITERATION-LOG.md

  echo ""
  echo "=== LATEST ITERATION LOG ENTRY ==="
  awk '
    /^## [0-9]{4}-[0-9]{2}-[0-9]{2}/ { buf = $0 "\n"; seen = 1; next }
    seen { buf = buf $0 "\n" }
    END { if (seen) printf "%s", buf; else print "(none)" }
  ' planning/ITERATION-LOG.md
fi
