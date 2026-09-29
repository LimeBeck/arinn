# SPDX-FileCopyrightText: 2026 Arinn contributors
#
# SPDX-License-Identifier: GPL-3.0-or-later

"""Tests for scripts/ci-scope.py (ADR-0044, ADR-0049)."""

import importlib.util
import pathlib
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
    def test_package_inputs_build_the_package(self):
        for path in (
            "packaging/arch/PKGBUILD",
            "packaging/systemd/arinn-shell.service",
            "packaging/chrome/org.arinn.chrome_host.json",
            "tests/package/session.py",
            ".github/workflows/package.yml",
            "Cargo.lock",
            "Cargo.toml",
            "plugins/chrome/Cargo.toml",
            "plugins/desktop/desktop.toml",
            "plugins/chrome/extension/background.js",
            "platform/kde/phase0.js",
            "platform/kde/arinn-kwin-adapter",
            "CMakeLists.txt",
        ):
            with self.subTest(path=path):
                self.assertTrue(ci_scope.package([path]))

    def test_ordinary_code_changes_skip_the_package(self):
        self.assertFalse(ci_scope.package([
            "crates/arinnd/src/runtime.rs",
            "apps/arinn-shell/Main.qml",
            "tools/arinnctl/src/main.rs",
            "tests/integration/phase1.py",
        ]))

    def test_unknown_changes_build_the_package(self):
        # Push, nightly, release and manual runs have no path list.
        self.assertTrue(ci_scope.package(None))


if __name__ == "__main__":
    unittest.main()
