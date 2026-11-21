#!/usr/bin/env python3
"""PreToolUse safety guard for Pitborn autonomous agent runs.

Enforces AGENTS.md git and PR guardrails via shell/token parsing:
- Hard-denies direct pushes to `main` (explicit refspecs, bare pushes on `main`,
  including via URL remotes or `git -C`/`-c` global flags).
- Hard-denies `--no-verify` / `-n` hook bypasses on `git commit` / `git push` and
  `gh pr merge --admin` branch-protection bypasses.
- Allows clean, non-force pushes targeting `auto/*` branches (iterate-game PR workflow).
- Prompts (`ask`) before any other `git push` (non-`auto/*` branches, force pushes,
  deletes, tags, dynamic vars), `gh pr merge`, or destructive `git reset --hard` /
  `git clean -f/-d`.
- Allows normal local development and verification commands without false positives
  on commit messages or `auto/*` branch names containing `main` (e.g., `auto/main-menu`).
"""

from __future__ import annotations

import json
import re
import shlex
import subprocess
import sys
from pathlib import Path

AUTO_BRANCH_RE = re.compile(r"^auto/[A-Za-z0-9._-]+(?:/[A-Za-z0-9._-]+)*$")
SAFE_REMOTE_RE = re.compile(r"^[A-Za-z0-9._-]+$")
SAFE_SRC_RE = re.compile(r"^[A-Za-z0-9._/@^~-]+$")
SAFE_PUSH_FLAGS = {
    "-u",
    "--set-upstream",
    "-v",
    "--verbose",
    "-q",
    "--quiet",
    "--porcelain",
    "--progress",
    "--no-progress",
    "--atomic",
}
GIT_GLOBAL_VALUE_FLAGS = {"-C", "-c", "--git-dir", "--work-tree", "--namespace"}
GIT_COMMIT_VALUE_FLAGS = {
    "-m",
    "-F",
    "-C",
    "-c",
    "-t",
    "--message",
    "--file",
    "--reuse-message",
    "--reedit-message",
    "--fixup",
    "--squash",
    "--template",
    "--author",
    "--date",
    "--cleanup",
    "--trailer",
}
GIT_PUSH_VALUE_FLAGS = {
    "-o",
    "--push-option",
    "--repo",
    "--receive-pack",
    "--exec",
    "--force-with-lease",
    "--signed",
}
GH_VALUE_FLAGS = {"-R", "--repo", "-H", "--hostname"}
FALSEY_VALUES = {"false", "0", "no", "f", "off"}


def current_branch(cwd: str) -> str:
    try:
        out = subprocess.check_output(
            ["git", "rev-parse", "--abbrev-ref", "HEAD"],
            cwd=cwd,
            stderr=subprocess.DEVNULL,
            text=True,
        )
        return out.strip()
    except Exception:
        return ""


def is_safe_auto_branch(name: str) -> bool:
    return bool(AUTO_BRANCH_RE.match(name)) and ".." not in name


def is_safe_auto_refspec(refspec: str) -> bool:
    if not refspec or refspec.startswith(("+", ":", "-")):
        return False
    if ":" in refspec:
        parts = refspec.split(":")
        if len(parts) != 2:
            return False
        src, dst = parts
        if not src or not SAFE_SRC_RE.match(src) or ".." in src:
            return False
    else:
        dst = refspec
    if dst.startswith("refs/heads/"):
        dst = dst[len("refs/heads/") :]
    return is_safe_auto_branch(dst)


def split_shell_segments(cmd: str) -> list[str] | None:
    """Split a shell command into executable segments, stripping comments & grouping."""
    segments: list[str] = []
    buf: list[str] = []
    quote: str | None = None
    i = 0
    n = len(cmd)
    while i < n:
        ch = cmd[i]
        if quote:
            buf.append(ch)
            if ch == "\\" and quote == '"' and i + 1 < n:
                i += 1
                buf.append(cmd[i])
            elif ch == quote:
                quote = None
            i += 1
            continue
        if ch in ("'", '"'):
            quote = ch
            buf.append(ch)
            i += 1
            continue
        if ch == "\\" and i + 1 < n:
            buf.append(ch)
            buf.append(cmd[i + 1])
            i += 2
            continue
        # Unquoted `#` at start of word starts a shell comment until newline.
        if ch == "#" and (i == 0 or cmd[i - 1] in (" ", "\t", "\n", ";", "&", "|", "(", "{")):
            while i < n and cmd[i] != "\n":
                i += 1
            continue
        if ch in (";", "\n", "(", ")", "{", "}"):
            segments.append("".join(buf).strip())
            buf = []
            i += 1
            continue
        if ch == "|":
            segments.append("".join(buf).strip())
            buf = []
            i += 2 if (i + 1 < n and cmd[i + 1] == "|") else 1
            continue
        if ch == "&":
            # Double && is a command separator.
            if i + 1 < n and cmd[i + 1] == "&":
                segments.append("".join(buf).strip())
                buf = []
                i += 2
                continue
            # Redirections like `2>&1`, `<&0`, `&>log` are not separators.
            prev_ch = cmd[i - 1] if i > 0 else ""
            next_ch = cmd[i + 1] if i + 1 < n else ""
            if prev_ch in (">", "<") or next_ch == ">":
                buf.append(ch)
                i += 1
                continue
            # Single `&` runs a background command and starts a new segment.
            segments.append("".join(buf).strip())
            buf = []
            i += 1
            continue
        buf.append(ch)
        i += 1
    if quote is not None:
        return None
    segments.append("".join(buf).strip())
    return [s for s in segments if s]


def strip_wrappers(tokens: list[str]) -> list[str]:
    """Skip env assignments and common command wrappers (`env`, `command`, `exec`, etc.)."""
    idx = 0
    n = len(tokens)
    while idx < n:
        tok = tokens[idx]
        if re.match(r"^[A-Za-z_][A-Za-z0-9_]*=", tok):
            idx += 1
            continue
        base = Path(tok).name
        if base == "env":
            idx += 1
            while idx < n:
                t = tokens[idx]
                if t == "--":
                    idx += 1
                    break
                if t in ("-u", "--unset", "-C", "--chdir", "-S", "--split-string"):
                    idx += 2
                    continue
                if t.startswith("-") or re.match(r"^[A-Za-z_][A-Za-z0-9_]*=", t):
                    idx += 1
                    continue
                break
            continue
        if base in ("command", "exec", "nohup", "time"):
            idx += 1
            while idx < n and tokens[idx].startswith("-"):
                if tokens[idx] == "--":
                    idx += 1
                    break
                idx += 1
            continue
        break
    return tokens[idx:]


def parse_git_command(
    tokens: list[str], cwd: str
) -> tuple[str, list[str], bool, str] | None:
    """Return (subcommand, sub_args, has_global_override, effective_cwd) if invoking git."""
    tokens = strip_wrappers(tokens)
    if not tokens or Path(tokens[0]).name != "git":
        return None
    i = 1
    has_global_override = False
    effective_cwd = cwd
    while i < len(tokens):
        tok = tokens[i]
        if tok in GIT_GLOBAL_VALUE_FLAGS:
            has_global_override = True
            val = tokens[i + 1] if i + 1 < len(tokens) else ""
            if tok == "-C" and val:
                effective_cwd = str((Path(effective_cwd) / val).resolve())
            i += 2
            continue
        if tok.startswith(("--git-dir=", "--work-tree=", "--namespace=", "-C", "-c")):
            has_global_override = True
            if tok.startswith("-C") and len(tok) > 2:
                effective_cwd = str((Path(effective_cwd) / tok[2:]).resolve())
            i += 1
            continue
        if tok.startswith("-"):
            i += 1
            continue
        return tok, tokens[i + 1 :], has_global_override, effective_cwd
    return None


def git_commit_has_no_verify(args: list[str]) -> bool:
    skip_next = False
    for tok in args:
        if skip_next:
            skip_next = False
            continue
        if tok == "--":
            break
        if tok in GIT_COMMIT_VALUE_FLAGS:
            skip_next = True
            continue
        if tok == "--no-verify":
            return True
        if tok.startswith("-") and not tok.startswith("--") and len(tok) >= 2:
            cluster = tok[1:]
            for idx, ch in enumerate(cluster):
                if ch == "n":
                    return True
                if f"-{ch}" in GIT_COMMIT_VALUE_FLAGS:
                    if idx == len(cluster) - 1:
                        skip_next = True
                    break
    return False


def git_push_has_no_verify(args: list[str]) -> bool:
    for tok in args:
        if tok == "--":
            break
        if tok == "--no-verify":
            return True
    return False


def is_main_ref(ref: str) -> bool:
    return ref in ("main", "refs/heads/main")


def git_push_targets_main(args: list[str], branch: str) -> bool:
    """Return True if parsed `git push` arguments would update or delete `main`."""
    positionals: list[str] = []
    skip_next = False
    after_dd = False
    is_delete = False
    pushes_all_or_mirror = False

    for tok in args:
        if skip_next:
            skip_next = False
            continue
        if not after_dd and tok == "--":
            after_dd = True
            continue
        if not after_dd and tok.startswith("-"):
            if tok in ("-d", "--delete"):
                is_delete = True
            elif tok in ("--all", "--mirror"):
                pushes_all_or_mirror = True
            elif tok in GIT_PUSH_VALUE_FLAGS:
                skip_next = True
            continue
        positionals.append(tok)

    if pushes_all_or_mirror:
        return True

    # Bare `git push` or `git push <remote>` (including URL remotes) pushes current branch.
    if len(positionals) <= 1:
        return branch == "main"

    # `positionals[0]` is remote; `positionals[1:]` are refspecs.
    for raw_ref in positionals[1:]:
        ref = raw_ref[1:] if raw_ref.startswith("+") else raw_ref
        if ":" in ref:
            src, dst = ref.split(":", 1)
            if is_main_ref(dst):
                return True
            if dst == "" and is_main_ref(src):
                return True
        else:
            if is_main_ref(ref):
                return True
            if is_delete and is_main_ref(ref):
                return True
    return False


def is_safe_auto_push(
    args: list[str], segment: str, branch: str, has_global_override: bool
) -> bool:
    if has_global_override or "`" in segment or "$(" in segment or "$" in segment:
        return False
    positionals: list[str] = []
    for tok in args:
        if tok == "--":
            return False
        if tok.startswith("-"):
            if tok not in SAFE_PUSH_FLAGS:
                return False
        else:
            positionals.append(tok)
    if len(positionals) == 0:
        return is_safe_auto_branch(branch)
    if len(positionals) == 1:
        return bool(SAFE_REMOTE_RE.match(positionals[0])) and is_safe_auto_branch(branch)
    if len(positionals) == 2:
        remote, refspec = positionals
        return bool(SAFE_REMOTE_RE.match(remote)) and is_safe_auto_refspec(refspec)
    return False


def git_reset_is_hard(args: list[str]) -> bool:
    for tok in args:
        if tok == "--":
            break
        if tok == "--hard":
            return True
    return False


def git_clean_is_destructive(args: list[str]) -> bool:
    for tok in args:
        if tok == "--":
            break
        if tok == "--force":
            return True
        if tok.startswith("-") and not tok.startswith("--"):
            if any(ch in ("f", "d") for ch in tok[1:]):
                return True
    return False


def check_gh_pr_merge(tokens: list[str]) -> tuple[str, str] | None:
    tokens = strip_wrappers(tokens)
    if not tokens or Path(tokens[0]).name != "gh":
        return None
    subcommands: list[str] = []
    flags: list[str] = []
    skip_next = False
    after_dd = False
    for tok in tokens[1:]:
        if skip_next:
            skip_next = False
            continue
        if not after_dd and tok == "--":
            after_dd = True
            continue
        if not after_dd and tok.startswith("-"):
            if tok in GH_VALUE_FLAGS:
                skip_next = True
            else:
                flags.append(tok)
            continue
        if len(subcommands) < 2:
            subcommands.append(tok)
    if subcommands[:2] != ["pr", "merge"]:
        return None
    for tok in flags:
        if tok == "--admin":
            return (
                "deny",
                "AGENTS.md forbids bypassing required reviews, status checks, or branch protections (`gh pr merge --admin`).",
            )
        if tok.startswith("--admin="):
            val = tok.split("=", 1)[1].strip().lower()
            if val not in FALSEY_VALUES:
                return (
                    "deny",
                    "AGENTS.md forbids bypassing required reviews, status checks, or branch protections (`gh pr merge --admin`).",
                )
    return (
        "ask",
        "Merging pull requests requires human review and confirmation; autonomous iterations leave PRs open for review and CI.",
    )


def emit(decision: str, reason: str | None = None) -> None:
    payload: dict[str, str] = {"decision": decision}
    if reason:
        payload["reason"] = reason
    sys.stdout.write(json.dumps(payload))


def main() -> None:
    try:
        data = json.load(sys.stdin)
    except Exception:
        emit("allow")
        return

    tool_call = data.get("toolCall") or {}
    args = tool_call.get("args") or {}
    cmd = str(args.get("CommandLine") or "")
    cwd = str(data.get("cwd") or Path.cwd())

    if not cmd.strip():
        emit("allow")
        return

    ask_reason: str | None = None
    segments = split_shell_segments(cmd)

    # Fail safe when quoting is unparseable and command mentions git/gh.
    if segments is None:
        if re.search(r"\b(?:git|gh)\b", cmd):
            emit(
                "ask",
                "Could not safely parse shell quoting in command involving git/gh; requires confirmation.",
            )
            return
        emit("allow")
        return

    for seg in segments:
        try:
            tokens = shlex.split(seg)
        except ValueError:
            if re.search(r"\b(?:git|gh)\b", seg):
                ask_reason = ask_reason or (
                    "Could not safely tokenize segment involving git/gh; requires confirmation."
                )
            continue

        parsed_git = parse_git_command(tokens, cwd)
        if parsed_git is not None:
            sub, sub_args, has_global_override, effective_cwd = parsed_git
            if sub == "commit" and git_commit_has_no_verify(sub_args):
                emit(
                    "deny",
                    "Bypassing git hooks with `--no-verify` / `-n` is forbidden; fix the failing check instead.",
                )
                return
            if sub == "push":
                if git_push_has_no_verify(sub_args):
                    emit(
                        "deny",
                        "Bypassing git push hooks with `--no-verify` is forbidden; fix the failing check instead.",
                    )
                    return
                branch = current_branch(effective_cwd)
                if git_push_targets_main(sub_args, branch):
                    emit(
                        "deny",
                        "AGENTS.md forbids pushing directly to `main`. Push to a working branch and open a PR instead.",
                    )
                    return
                if not is_safe_auto_push(sub_args, seg, branch, has_global_override):
                    ask_reason = ask_reason or (
                        "Publishing non-`auto/*` branches or force-pushing requires explicit "
                        "user approval (AGENTS.md keeps ordinary work local until requested)."
                    )
            elif sub == "reset" and git_reset_is_hard(sub_args):
                ask_reason = ask_reason or (
                    "Destructive git command (`reset --hard` or `clean -f/-d`) requires confirmation."
                )
            elif sub == "clean" and git_clean_is_destructive(sub_args):
                ask_reason = ask_reason or (
                    "Destructive git command (`reset --hard` or `clean -f/-d`) requires confirmation."
                )

        gh_decision = check_gh_pr_merge(tokens)
        if gh_decision is not None:
            decision, reason = gh_decision
            if decision == "deny":
                emit("deny", reason)
                return
            ask_reason = ask_reason or reason

    if ask_reason:
        emit("ask", ask_reason)
        return

    emit("allow")


if __name__ == "__main__":
    main()
