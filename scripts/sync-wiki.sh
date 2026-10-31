#!/usr/bin/env bash
# Requires the first wiki page to have been initialized through GitHub's Wiki tab.
set -euo pipefail
cd "$(dirname "$0")/.."
repository="$(gh repo view --json nameWithOwner --jq .nameWithOwner)"
wiki_directory="$(mktemp -d)"
trap 'rm -rf "$wiki_directory"' EXIT
git clone "https://github.com/${repository}.wiki.git" "$wiki_directory"
cp docs/wiki/*.md "$wiki_directory/"
git -C "$wiki_directory" add -- '*.md'
if ! git -C "$wiki_directory" diff --cached --quiet; then
  git -C "$wiki_directory" commit -m 'Sync wiki from main repository'
  git -C "$wiki_directory" push origin HEAD
fi
