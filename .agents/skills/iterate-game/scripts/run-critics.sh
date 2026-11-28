#!/usr/bin/env bash
# Run Pitborn's automated verification & smoke critics sequentially.
# Usage:
#   .agents/skills/iterate-game/scripts/run-critics.sh [--auto] [--base <ref>] [--smoke] [--wasm] [--web-build] [--all]
#
# Always runs:
#   1. ./scripts/check.sh (fmt, clippy, Rust/Python/JS/TS tests, palette, lint)
#   2. cargo run -p arena-headless --locked (deterministic scripted duel)
# Optional flags:
#   --auto        Auto-enable --smoke, --web-build, and --wasm based on files changed vs --base
#   --base <ref>  Base git ref for --auto diff detection (default: main, fallback: origin/main)
#   --smoke       Build arena-world + arena-bot and run ./scripts/server-smoke.sh
#   --web-build   Run `npm run build` (matches CI `browser` job)
#   --wasm        Run wasm32-unknown-unknown Clippy check on arena-client
#   --all         Enable --smoke, --web-build, and --wasm
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root"

auto_detect=false
base_ref="main"
explicit_base=false
run_smoke=false
run_web_build=false
run_wasm=false

while [[ $# -gt 0 ]]; do
  case "$1" in
    --auto) auto_detect=true ;;
    --base)
      if [[ $# -lt 2 ]]; then
        echo "--base requires a git ref argument" >&2
        exit 2
      fi
      base_ref="$2"
      explicit_base=true
      shift
      ;;
    --smoke) run_smoke=true ;;
    --web-build) run_web_build=true ;;
    --wasm) run_wasm=true ;;
    --all)
      run_smoke=true
      run_web_build=true
      run_wasm=true
      ;;
    *)
      echo "Unknown option: $1 (supported: --auto, --base <ref>, --smoke, --web-build, --wasm, --all)" >&2
      exit 2
      ;;
  esac
  shift
done

if $auto_detect; then
  resolved_base=""
  if git rev-parse --verify "$base_ref^{commit}" >/dev/null 2>&1; then
    resolved_base="$base_ref"
  elif ! $explicit_base && git rev-parse --verify "origin/main^{commit}" >/dev/null 2>&1; then
    resolved_base="origin/main"
  else
    echo "Base ref '$base_ref' does not exist; pass a valid --base <ref>." >&2
    exit 2
  fi

  merge_base="$(git merge-base "$resolved_base" HEAD)"
  committed_files="$(git diff --name-only "$merge_base" HEAD)"
  unstaged_files="$(git diff --name-only)"
  staged_files="$(git diff --cached --name-only)"
  untracked_files="$(git ls-files --others --exclude-standard)"
  all_changed="$(printf '%s\n%s\n%s\n%s\n' "$committed_files" "$unstaged_files" "$staged_files" "$untracked_files" | sed '/^$/d' | sort -u)"

  # Matches crates participating in CI `rust` server-smoke job.
  if grep -Eq '^(crates/(arena-sim|arena-protocol|arena-world|arena-bot|arena-map|arena-palette)/|Cargo\.toml$|Cargo\.lock$)' <<<"$all_changed"; then
    run_smoke=true
  fi
  # Matches inputs to CI `browser` `npm run build`.
  if grep -Eq '^(web/|package\.json$|package-lock\.json$|vite\.config\.ts$|tsconfig\.json$|docs/palette\.json$)' <<<"$all_changed"; then
    run_web_build=true
  fi
  # Matches `scripts/client-build-needed.sh` pathspecs for CI `client-build` WASM check.
  if grep -Eq '^(crates/|(.*/)?Cargo\.(toml|lock)$|rust-toolchain|\.cargo/|assets/|scripts/|\.github/workflows/)' <<<"$all_changed"; then
    run_wasm=true
  fi

  echo "==> [auto-detect vs $resolved_base] smoke=$run_smoke web_build=$run_web_build wasm=$run_wasm"
fi

echo "==> [critic:check] ./scripts/check.sh"
./scripts/check.sh

echo "==> [critic:headless] cargo run -p arena-headless --locked"
cargo run -p arena-headless --locked

if $run_smoke; then
  echo "==> [critic:smoke] building arena-world & arena-bot + running ./scripts/server-smoke.sh"
  cargo build -p arena-world -p arena-bot --locked
  ./scripts/server-smoke.sh
fi

if $run_web_build; then
  echo "==> [critic:web-build] npm run build"
  npm run build
fi

if $run_wasm; then
  echo "==> [critic:wasm] wasm32-unknown-unknown clippy check"
  cargo clippy -p arena-client --target wasm32-unknown-unknown --locked -- -D warnings
fi

echo "==> All requested critics passed (smoke=$run_smoke, web_build=$run_web_build, wasm=$run_wasm)."
