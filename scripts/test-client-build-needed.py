"""Synthetic Git history tests for the conditional full-client build gate."""

import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


class BuildChangesTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="arena-ci-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        (self.root / "scripts").mkdir()
        shutil.copyfile(
            Path(__file__).with_name("client-build-needed.sh"),
            self.root / "scripts/client-build-needed.sh",
        )
        self.git("init", "-q")
        self.git("config", "user.name", "CI fixture")
        self.git("config", "user.email", "ci@example.invalid")
        self.base = self.commit("README.md")

    def git(self, *args):
        return subprocess.check_output(
            ["git", *args], cwd=self.root, text=True, stderr=subprocess.PIPE
        ).strip()

    def commit(self, path):
        target = self.root / path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text("fixture\n")
        self.git("add", ".")
        self.git("commit", "-qm", "fixture")
        return self.git("rev-parse", "HEAD")

    def detect(self, base, head, event="pull_request"):
        return subprocess.run(
            ["bash", "scripts/client-build-needed.sh"],
            cwd=self.root,
            env={**os.environ, "EVENT_NAME": event, "BASE_SHA": base, "HEAD_SHA": head},
            capture_output=True,
            text=True,
        )

    def test_relevant_paths(self):
        for path in [
            "Cargo.toml", "Cargo.lock", "crates/arena-client/src/main.rs",
            "crates/arena-sim/src/lib.rs", "crates/arena-headless/src/main.rs",
            "crates/new/Cargo.toml", "rust-toolchain.toml", ".cargo/config.toml",
            "assets/model.glb", "scripts/build-web.sh", ".github/workflows/ci.yml",
        ]:
            with self.subTest(path=path):
                before = self.git("rev-parse", "HEAD")
                result = self.detect(before, self.commit(path))
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(result.stdout.strip(), "build=true")

    def test_unrelated_paths(self):
        for path in ["docs/guide.md", "web/input.js", "tests/unit/input.test.mjs", "package.json"]:
            self.commit(path)
        result = self.detect(self.base, self.git("rev-parse", "HEAD"))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.strip(), "build=false")

    def test_dispatch_and_new_branch(self):
        for event, base in [("workflow_dispatch", ""), ("push", "0" * 40)]:
            result = self.detect(base, self.base, event)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(result.stdout.strip(), "build=true")

    def test_invalid_revision_fails_closed(self):
        result = self.detect("invalid-revision", self.base)
        self.assertNotEqual(result.returncode, 0)
        self.assertNotIn("build=false", result.stdout)

    def test_pr_uses_merge_base_but_push_uses_previous_tip(self):
        head = self.commit("docs/guide.md")
        self.git("checkout", "-q", "--detach", self.base)
        base = self.commit("Cargo.lock")
        self.assertEqual(self.detect(base, head).stdout.strip(), "build=false")
        self.assertEqual(self.detect(base, head, "push").stdout.strip(), "build=true")


if __name__ == "__main__":
    unittest.main()
