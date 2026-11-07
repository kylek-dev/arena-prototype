#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

# Dispatch and a new branch without a prior commit always get full coverage.
if [[ "$EVENT_NAME" == workflow_dispatch || -z "$BASE_SHA" || "$BASE_SHA" =~ ^0+$ ]]; then
  echo 'build=true'
  exit 0
fi

base="$BASE_SHA"
if [[ "$EVENT_NAME" == pull_request ]]; then
  base="$(git merge-base "$BASE_SHA" "$HEAD_SHA")"
fi

# Git's glob pathspecs include root Cargo files and all nested crates. Diff
# failures must fail the job rather than masquerade as an unrelated change.
if git diff --quiet "$base" "$HEAD_SHA" -- \
  ':(glob)crates/**' \
  ':(glob)**/Cargo.toml' \
  ':(glob)**/Cargo.lock' \
  ':(glob)rust-toolchain*' \
  ':(glob).cargo/**' \
  ':(glob)assets/**' \
  ':(glob)scripts/**' \
  ':(glob).github/workflows/**'; then
  echo 'build=false'
else
  status=$?
  if [[ "$status" != 1 ]]; then
    exit "$status"
  fi
  echo 'build=true'
fi
