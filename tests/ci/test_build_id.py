# SPDX-FileCopyrightText: 2026 Arinn contributors
# SPDX-License-Identifier: GPL-3.0-or-later

"""Build id format and sources (ADR-0097)."""
import importlib.util
import os
import re
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "scripts/build_id.py"
spec = importlib.util.spec_from_file_location("build_id", SCRIPT)
build_id = importlib.util.module_from_spec(spec)
spec.loader.exec_module(build_id)
COMMIT = "1bd20d647afe" + "0" * 28


class BuildId(unittest.TestCase):
    def setUp(self):
        self.version = build_id.release.version()
        patcher = mock.patch.dict(os.environ, {}, clear=False)
        patcher.start()
        os.environ.pop("ARINN_BUILD_ID", None)
        self.addCleanup(patcher.stop)

    def test_format(self):
        for good in ["0.1.0", "0.1.0+g1bd20d647afe", "0.1.0+g1bd20d647afe-dirty"]:
            self.assertTrue(build_id.valid(good, "0.1.0"), good)
        for bad in ["0.1.1", "0.1.0+g1bd20d647af", "0.1.0+G1bd20d647afe", "0.1.0-dirty", "v0.1.0"]:
            self.assertFalse(build_id.valid(bad, "0.1.0"), bad)
        self.assertEqual(build_id.compose("0.1.0", COMMIT, True), "0.1.0+g1bd20d647afe-dirty")
        self.assertEqual(build_id.compose("0.1.0", None, True), "0.1.0")

    def test_explicit_id_wins_and_is_validated(self):
        os.environ["ARINN_BUILD_ID"] = f"{self.version}+g1bd20d647afe"
        self.assertEqual(build_id.build_id(), f"{self.version}+g1bd20d647afe")
        os.environ["ARINN_BUILD_ID"] = "0.0.0+g1bd20d647afe"
        with self.assertRaises(SystemExit):
            build_id.build_id()

    def test_archived_commit_comes_before_git(self):
        with tempfile.TemporaryDirectory() as folder:
            archived = Path(folder) / "build-commit"
            archived.write_text("# header\n" + COMMIT + "\n", encoding="utf-8")
            with mock.patch.object(build_id, "ARCHIVED", archived), \
                    mock.patch.object(build_id, "git", side_effect=AssertionError("no git")):
                self.assertEqual(build_id.build_id(), f"{self.version}+g1bd20d647afe")
            # In a checkout the placeholder is not a commit.
            archived.write_text("# header\n$Format:%H$\n", encoding="utf-8")
            with mock.patch.object(build_id, "ARCHIVED", archived), \
                    mock.patch.object(build_id, "git", return_value=None):
                self.assertEqual(build_id.build_id(), self.version)

    def test_git_checkout_reports_dirty_tracked_changes(self):
        answers = {("rev-parse", "--verify", "HEAD"): COMMIT}

        def git(*args):
            if args[-3:] == ("status", "--porcelain", "--untracked-files=no"):
                return answers.get("status", "")
            return answers.get(args)

        with mock.patch.object(build_id, "archived_commit", return_value=None), \
                mock.patch.object(build_id, "git", side_effect=git):
            self.assertEqual(build_id.build_id(), f"{self.version}+g1bd20d647afe")
            answers["status"] = " M crates/arinnd/src/main.rs"
            self.assertEqual(build_id.build_id(), f"{self.version}+g1bd20d647afe-dirty")

    def test_this_checkout(self):
        printed = subprocess.run(
            ["python3", str(SCRIPT)], capture_output=True, text=True, check=True,
            env={k: v for k, v in os.environ.items() if k != "ARINN_BUILD_ID"},
        ).stdout.strip()
        self.assertTrue(build_id.valid(printed, self.version), printed)



    def test_snapshot_pkgver_carries_count_and_hash(self):
        answers = {
            ("rev-parse", "--is-shallow-repository"): "false",
            ("rev-parse", "--verify", "HEAD"): COMMIT,
            ("rev-list", "--count", "HEAD"): "394",
        }
        with mock.patch.object(build_id, "git", side_effect=lambda *a: answers.get(a)):
            self.assertEqual(build_id.snapshot_pkgver(), f"{self.version}.r394.g1bd20d647afe")
        # A shallow clone would count only what it fetched: refuse it.
        answers[("rev-parse", "--is-shallow-repository")] = "true"
        with mock.patch.object(build_id, "git", side_effect=lambda *a: answers.get(a)):
            with self.assertRaises(SystemExit):
                build_id.snapshot_pkgver()
        with mock.patch.object(build_id, "git", return_value=None):
            with self.assertRaises(SystemExit):
                build_id.snapshot_pkgver()

    def test_snapshot_pkgver_in_this_checkout(self):
        shallow = build_id.git("rev-parse", "--is-shallow-repository")
        if shallow != "false":
            self.skipTest("shallow or no git checkout")
        result = subprocess.run(["python3", str(SCRIPT), "--pkgver"], capture_output=True, text=True, check=True)
        self.assertRegex(result.stdout.strip(), rf"^{re.escape(self.version)}\.r\d+\.g[0-9a-f]{{12}}$")


if __name__ == "__main__":
    unittest.main()
