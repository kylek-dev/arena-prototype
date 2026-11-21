#!/usr/bin/env python3
"""Stop hook for Pitborn.

Default mode (fast): checks modified and untracked files across the git repo for
leftover merge conflict markers.
Strict mode (`--run-check`, opt-in in `.agents/hooks.json`): when source/config
files have uncommitted changes, runs `./scripts/check.sh` once before allowing
the loop to stop.
"""

from __future__ import annotations

import json
import re
import subprocess
import sys
from pathlib import Path

CONFLICT_LINE_RE = re.compile(r"^(?:<{7} |>{7} )", re.MULTILINE)
CODE_SUFFIXES = (".rs", ".ts", ".tsx", ".js", ".mjs", ".py", ".json", ".toml", ".css")


def emit(decision: str, reason: str | None = None) -> None:
    payload: dict[str, str] = {"decision": decision}
    if reason:
        payload["reason"] = reason
    sys.stdout.write(json.dumps(payload))


def resolve_repo_root(cwd: str) -> Path | None:
    try:
        out = subprocess.check_output(
            ["git", "rev-parse", "--show-toplevel"],
            cwd=cwd,
            stderr=subprocess.DEVNULL,
            text=True,
        )
        root = out.strip()
        return Path(root) if root else None
    except Exception:
        return None


def modified_files(repo_root: Path) -> list[str]:
    try:
        out = subprocess.check_output(
            ["git", "status", "--porcelain", "-uall", "-z"],
            cwd=str(repo_root),
            stderr=subprocess.DEVNULL,
            text=True,
        )
    except Exception:
        return []
    entries = out.split("\0")
    files: list[str] = []
    i = 0
    while i < len(entries):
        rec = entries[i]
        if len(rec) < 4:
            i += 1
            continue
        status = rec[:2]
        path = rec[3:]
        files.append(path)
        # In `-z` porcelain output, renames/copies (`R` or `C`) emit a second NUL-terminated old path.
        if "R" in status or "C" in status:
            i += 2
        else:
            i += 1
    return files


def has_conflict_markers(repo_root: Path, rel_paths: list[str]) -> list[str]:
    bad: list[str] = []
    for rel in rel_paths:
        p = repo_root / rel
        if not p.is_file():
            continue
        try:
            text = p.read_text(encoding="utf-8", errors="ignore")
        except Exception:
            continue
        if CONFLICT_LINE_RE.search(text):
            bad.append(rel)
    return bad


def main() -> None:
    run_check = "--run-check" in sys.argv[1:]
    try:
        data = json.load(sys.stdin)
    except Exception:
        emit("allow")
        return

    # Never loop forever if a Stop hook already asked the agent to continue this turn,
    # or if the user canceled / hit a hard error.
    if data.get("continuationActive"):
        emit("allow")
        return
    if data.get("terminationReason") not in ("NO_TOOL_CALL", "", None):
        emit("allow")
        return

    cwd = str(data.get("cwd") or Path.cwd())
    repo_root = resolve_repo_root(cwd)
    if repo_root is None:
        emit("allow")
        return

    changed = modified_files(repo_root)
    if not changed:
        emit("allow")
        return

    conflicted = has_conflict_markers(repo_root, changed)
    if conflicted:
        emit(
            "continue",
            f"Unresolved merge conflict markers found in: {', '.join(conflicted)}. Resolve them before stopping.",
        )
        return

    check_script = repo_root / "scripts" / "check.sh"
    if (
        run_check
        and check_script.is_file()
        and any(p.endswith(CODE_SUFFIXES) for p in changed)
    ):
        proc = subprocess.run(
            ["bash", str(check_script)],
            cwd=str(repo_root),
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
        )
        if proc.returncode != 0:
            tail = "\n".join(proc.stdout.splitlines()[-25:])
            emit(
                "continue",
                f"`./scripts/check.sh` failed with uncommitted code edits. Fix the failures before finishing:\n{tail}",
            )
            return

    emit("allow")


if __name__ == "__main__":
    main()
