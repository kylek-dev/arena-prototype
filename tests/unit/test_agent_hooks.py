import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
GUARD_HOOK = REPO_ROOT / ".agents" / "hooks" / "guard-commands.py"
STOP_HOOK = REPO_ROOT / ".agents" / "hooks" / "stop-check.py"
CONFLICT_FIXTURE = (
    "<" * 7 + " HEAD\nours\n" + "=" * 7 + "\ntheirs\n" + ">" * 7 + " main\n"
)


def run_hook(script: Path, payload: dict | str, extra_args: list[str] | None = None) -> dict:
    cmd = [sys.executable, str(script), *(extra_args or [])]
    stdin_text = payload if isinstance(payload, str) else json.dumps(payload)
    proc = subprocess.run(
        cmd,
        input=stdin_text,
        capture_output=True,
        text=True,
        check=True,
    )
    return json.loads(proc.stdout)


def init_temp_repo(branch: str) -> tempfile.TemporaryDirectory[str]:
    tmp = tempfile.TemporaryDirectory()
    root = Path(tmp.name)
    subprocess.run(
        ["git", "init", "-b", branch],
        cwd=root,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        check=True,
    )
    subprocess.run(
        ["git", "config", "user.email", "test@example.com"],
        cwd=root,
        check=True,
    )
    subprocess.run(
        ["git", "config", "user.name", "Test User"],
        cwd=root,
        check=True,
    )
    (root / "README.md").write_text("hello\n", encoding="utf-8")
    subprocess.run(["git", "add", "README.md"], cwd=root, check=True)
    subprocess.run(
        ["git", "commit", "-m", "initial"],
        cwd=root,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        check=True,
    )
    return tmp


class GuardCommandsHookTests(unittest.TestCase):
    def _eval(self, command: str, cwd: str) -> dict:
        return run_hook(
            GUARD_HOOK,
            {
                "toolCall": {"name": "run_command", "args": {"CommandLine": command}},
                "cwd": cwd,
            },
        )

    def test_allows_ordinary_local_commands_and_malformed_json(self) -> None:
        with init_temp_repo("main") as tmp:
            self.assertEqual(
                self._eval("./scripts/check.sh && git status -sb", tmp)["decision"],
                "allow",
            )
            self.assertEqual(run_hook(GUARD_HOOK, "not-json")["decision"], "allow")

    def test_denies_push_to_main_including_global_flags_and_url_remotes(self) -> None:
        with init_temp_repo("main") as tmp:
            for cmd in (
                "git push origin main",
                "git push origin HEAD:main",
                "git push origin refs/heads/main",
                "git push",
                "git push origin",
                "git push https://github.com/example/repo.git",
                "git push git@github.com:example/repo.git",
                "git push origin --all",
                "git push origin --mirror",
                "git -C . push origin main",
                "git -c push.default=current push origin main",
                "/usr/bin/git push origin main",
                "env GIT_TRACE=1 git push origin main",
            ):
                with self.subTest(cmd=cmd):
                    self.assertEqual(self._eval(cmd, tmp)["decision"], "deny")

    def test_allows_clean_auto_branch_push_including_main_substring(self) -> None:
        with init_temp_repo("auto/main-menu") as tmp:
            for cmd in (
                "git push -u origin auto/main-menu",
                "git push -u origin HEAD:auto/main-menu",
                "git push origin",
                "git push",
            ):
                with self.subTest(cmd=cmd):
                    self.assertEqual(self._eval(cmd, tmp)["decision"], "allow")

    def test_no_false_positives_in_commit_messages(self) -> None:
        with init_temp_repo("auto/round-clock") as tmp:
            for cmd in (
                "git commit -m 'docs: forbid git push origin main'",
                "git commit -m 'docs: guard git reset --hard and git clean -fd'",
                "git commit -m 'test -n and --no-verify parsing'",
            ):
                with self.subTest(cmd=cmd):
                    self.assertEqual(self._eval(cmd, tmp)["decision"], "allow")

    def test_asks_on_non_auto_force_global_override_or_dynamic_push(self) -> None:
        with init_temp_repo("feat/something") as tmp:
            self.assertEqual(
                self._eval("git push -u origin feat/something", tmp)["decision"],
                "ask",
            )
        with init_temp_repo("auto/round-clock") as tmp:
            for cmd in (
                "git push --force origin auto/round-clock",
                "git push --force-with-lease origin auto/round-clock",
                "git push origin +auto/round-clock",
                "git push origin $BRANCH",
                "git -C . push -u origin auto/round-clock",
                "git -c push.default=current push origin auto/round-clock",
            ):
                with self.subTest(cmd=cmd):
                    self.assertEqual(self._eval(cmd, tmp)["decision"], "ask")

    def test_denies_no_verify_across_comments_subshells_paths_and_optional_flags(self) -> None:
        with init_temp_repo("auto/round-clock") as tmp:
            for cmd in (
                "git commit --no-verify -m 'bypass'",
                "git commit -n -m 'bypass'",
                "git commit -an -m 'bypass'",
                "git commit -S -n -m 'bypass'",
                "git commit -u -n -m 'bypass'",
                "git push --no-verify origin auto/round-clock",
                "# Don't bypass\ngit commit --no-verify -m 'bypass'",
                "(git commit --no-verify -m 'bypass')",
                "echo hi & git commit --no-verify -m 'bypass'",
                "/usr/bin/git commit --no-verify -m 'bypass'",
                "git status && git commit --no-verify -m 'bypass'",
            ):
                with self.subTest(cmd=cmd):
                    self.assertEqual(self._eval(cmd, tmp)["decision"], "deny")

    def test_guards_gh_pr_merge_variants(self) -> None:
        with init_temp_repo("auto/round-clock") as tmp:
            for cmd in (
                "gh pr merge 42 --admin --squash",
                "gh pr merge 42 --admin=true",
                "gh pr -R kylek-dev/arena-prototype merge 42 --admin",
                "(gh pr merge 42 --admin)",
                "/usr/local/bin/gh pr merge 42 --admin",
            ):
                with self.subTest(cmd=cmd):
                    self.assertEqual(self._eval(cmd, tmp)["decision"], "deny")
            for cmd in (
                "gh pr merge 42 --squash",
                "gh pr -R kylek-dev/arena-prototype merge 42",
                "gh pr merge 42 --admin=false",
            ):
                with self.subTest(cmd=cmd):
                    self.assertEqual(self._eval(cmd, tmp)["decision"], "ask")
            self.assertEqual(
                self._eval("gh pr create --base main --head auto/round-clock", tmp)["decision"],
                "allow",
            )

    def test_asks_on_destructive_reset_and_clean_variants(self) -> None:
        with init_temp_repo("auto/round-clock") as tmp:
            for cmd in (
                "git reset --hard HEAD~1",
                "git reset HEAD~1 --hard",
                "git -C . reset --hard HEAD~1",
                "git clean -fd",
                "git clean --force",
                "git -C . clean -d",
            ):
                with self.subTest(cmd=cmd):
                    self.assertEqual(self._eval(cmd, tmp)["decision"], "ask")


class StopCheckHookTests(unittest.TestCase):
    def test_allows_clean_repo_and_non_normal_termination(self) -> None:
        with init_temp_repo("auto/test") as tmp:
            res = run_hook(STOP_HOOK, {"cwd": tmp, "terminationReason": "NO_TOOL_CALL"})
            self.assertEqual(res["decision"], "allow")
            (Path(tmp) / "README.md").write_text(CONFLICT_FIXTURE, encoding="utf-8")
            res_canceled = run_hook(
                STOP_HOOK, {"cwd": tmp, "terminationReason": "USER_CANCELED"}
            )
            self.assertEqual(res_canceled["decision"], "allow")

    def test_detects_conflicts_in_subdirs_spaces_and_new_untracked_dirs(self) -> None:
        with init_temp_repo("auto/test") as tmp:
            root = Path(tmp)
            subdir = root / "nested" / "sub"
            subdir.mkdir(parents=True)
            spaced = subdir / "file with spaces.txt"
            spaced.write_text(CONFLICT_FIXTURE, encoding="utf-8")
            # Invoked from a subdirectory cwd, stop-check must resolve repo root & unquote paths.
            res = run_hook(
                STOP_HOOK, {"cwd": str(subdir), "terminationReason": "NO_TOOL_CALL"}
            )
            self.assertEqual(res["decision"], "continue")
            self.assertIn("nested/sub/file with spaces.txt", res["reason"])

    def test_ignores_markdown_equals_underline_without_conflict_markers(self) -> None:
        with init_temp_repo("auto/test") as tmp:
            (Path(tmp) / "README.md").write_text("Title\n=======\nbody\n", encoding="utf-8")
            res = run_hook(STOP_HOOK, {"cwd": tmp, "terminationReason": "NO_TOOL_CALL"})
            self.assertEqual(res["decision"], "allow")

    def test_does_not_loop_when_continuation_active(self) -> None:
        with init_temp_repo("auto/test") as tmp:
            (Path(tmp) / "README.md").write_text(CONFLICT_FIXTURE, encoding="utf-8")
            res = run_hook(
                STOP_HOOK,
                {
                    "cwd": tmp,
                    "terminationReason": "NO_TOOL_CALL",
                    "continuationActive": True,
                },
            )
            self.assertEqual(res["decision"], "allow")

    def test_run_check_mode_pass_fail_and_non_code_skip(self) -> None:
        with init_temp_repo("auto/test") as tmp:
            root = Path(tmp)
            scripts = root / "scripts"
            scripts.mkdir()
            check_sh = scripts / "check.sh"
            check_sh.write_text("#!/usr/bin/env bash\nexit 1\n", encoding="utf-8")
            check_sh.chmod(0o755)

            # Doc-only change skips check.sh
            (root / "README.md").write_text("updated docs\n", encoding="utf-8")
            res_doc = run_hook(
                STOP_HOOK,
                {"cwd": tmp, "terminationReason": "NO_TOOL_CALL"},
                extra_args=["--run-check"],
            )
            self.assertEqual(res_doc["decision"], "allow")

            # Code change runs check.sh and blocks on non-zero exit
            (root / "src.rs").write_text("fn main() {}\n", encoding="utf-8")
            res_fail = run_hook(
                STOP_HOOK,
                {"cwd": str(scripts), "terminationReason": "NO_TOOL_CALL"},
                extra_args=["--run-check"],
            )
            self.assertEqual(res_fail["decision"], "continue")
            self.assertIn("check.sh", res_fail["reason"])

            # Passing check.sh allows stopping
            check_sh.write_text("#!/usr/bin/env bash\nexit 0\n", encoding="utf-8")
            res_pass = run_hook(
                STOP_HOOK,
                {"cwd": str(scripts), "terminationReason": "NO_TOOL_CALL"},
                extra_args=["--run-check"],
            )
            self.assertEqual(res_pass["decision"], "allow")


if __name__ == "__main__":
    unittest.main()
