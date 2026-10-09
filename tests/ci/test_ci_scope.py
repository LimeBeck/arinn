# SPDX-FileCopyrightText: 2026 Arinn contributors
#
# SPDX-License-Identifier: GPL-3.0-or-later

"""Tests for scripts/ci-scope.py (ADR-0044, ADR-0049, ADR-0102)."""

import importlib.util
import os
import pathlib
import subprocess
import sys
import unittest

SCRIPT = pathlib.Path(__file__).resolve().parents[2] / "scripts" / "ci-scope.py"
spec = importlib.util.spec_from_file_location("ci_scope", SCRIPT)
ci_scope = importlib.util.module_from_spec(spec)
spec.loader.exec_module(ci_scope)


class DocsOnlyTest(unittest.TestCase):
    def test_markdown_and_fragments_are_docs_only(self):
        self.assertTrue(ci_scope.docs_only([
            "README.md",
            "docs/adr/0043-web-shortcuts.md",
            "docs/specifications/arinn-specification.md",
            "changes/changed/53-web-shortcuts-decision.md",
        ]))

    def test_build_inputs_under_docs_need_a_full_run(self):
        for path in (
            "docs/plugin-api/daemon1.xml",
            "docs/design/handoff/assets/arinn-mark.svg",
            "docs/architecture/phase-1-live-evidence.json",
        ):
            with self.subTest(path=path):
                self.assertFalse(ci_scope.docs_only(["README.md", path]))

    def test_code_and_workflows_need_a_full_run(self):
        for path in (
            "crates/arinnd/src/main.rs",
            "apps/arinn-shell/qml/Home.qml",
            "packaging/arch/PKGBUILD",
            ".github/workflows/ci.yml",
            "scripts/ci-scope.py",
            "Cargo.lock",
        ):
            with self.subTest(path=path):
                self.assertFalse(ci_scope.docs_only([path]))

    def test_no_known_changes_need_a_full_run(self):
        self.assertFalse(ci_scope.docs_only([]))


class PackageTest(unittest.TestCase):
    # ADR-0102: the Arch and Debian packages build for master, the nightly,
    # releases and manual runs, never for a pull request.
    def test_pull_requests_never_build_the_packages(self):
        self.assertFalse(ci_scope.package(pull_request=True))

    def test_other_events_build_the_packages(self):
        self.assertTrue(ci_scope.package(pull_request=False))

    def test_a_pull_request_that_changes_packaging_skips_the_packages(self):
        env = dict(os.environ, GITHUB_OUTPUT="")
        result = subprocess.run(
            [sys.executable, str(SCRIPT), "packaging/arch/PKGBUILD", ".github/workflows/package.yml"],
            capture_output=True, text=True, env=env, check=True)
        self.assertIn("docs_only=false", result.stdout)
        self.assertIn("package=false", result.stdout)

    def test_a_push_builds_the_packages(self):
        env = dict(os.environ, GITHUB_OUTPUT="", GITHUB_EVENT_NAME="push")
        result = subprocess.run([sys.executable, str(SCRIPT)], capture_output=True, text=True,
                                env=env, check=True)
        self.assertIn("package=true", result.stdout)


if __name__ == "__main__":
    unittest.main()
