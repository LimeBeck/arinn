# SPDX-FileCopyrightText: 2026 Arinn contributors
#
# SPDX-License-Identifier: GPL-3.0-or-later

"""Tests for scripts/ci-scope.py (ADR-0044, ADR-0049, ADR-0102)."""

import importlib.util
import os
import pathlib
import subprocess
import sys
import tempfile
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


class ScopedPullRequestTest(unittest.TestCase):
    def test_public_website_and_tooling_do_not_need_native_builds(self):
        for path in (
            "docs/site/index.html", "docs/site/assets/site.js", "docs/site/assets/site.css",
            "docs/user/index.html", "docs/user/assets/home.png", "docs/user/assets/arinn-mark.svg",
            "docs/design/showcase/home.png", "scripts/build-site.py", "scripts/check-site.py",
            "scripts/check-user-site.py", "tests/ci/test_site.py",
        ):
            with self.subTest(path=path):
                self.assertEqual(ci_scope.scope(["README.md", path], True), "website")

    def test_capture_sources_select_the_renderer(self):
        for path in (
            "tests/ui/visual/capture.qml", "tests/ui/visual/ShowcaseMusic.qml",
            "tests/ui/visual/manifest.json", "scripts/ui-compare.py", "tests/ci/test_ui_compare.py",
        ):
            with self.subTest(path=path):
                self.assertEqual(ci_scope.scope([path, "docs/design/showcase/home.png"], True), "visual")

    def test_native_inputs_and_unknown_paths_override_scoped_files(self):
        for path in (
            "apps/arinn-shell/ui/HomeView.qml", "tests/ui/main.cpp", "tests/ui/tst_home.qml",
            "apps/arinn-shell/CMakeLists.txt", "CMakeLists.txt", "Cargo.lock",
            "crates/arinnd/src/main.rs", "docs/plugin-api/daemon1.xml",
            "docs/design/handoff/assets/arinn-mark.svg", "docs/architecture/evidence.json",
            "docs/site/tool.py", "docs/user/template.txt", "tests/ui/visual/helper.cpp",
            ".github/workflows/ci.yml", "scripts/ci-scope.py", "changes/fixed/tool.py",
            "../docs/site/index.html", "/docs/site/index.html", "unknown.data",
        ):
            with self.subTest(path=path):
                self.assertEqual(ci_scope.scope([
                    "README.md", "docs/site/index.html", "tests/ui/visual/capture.qml", path,
                ], True), "full")

    def test_unknown_changes_fail_closed(self):
        for paths in (None, [], [""]):
            with self.subTest(paths=paths):
                self.assertEqual(ci_scope.scope(paths, True), "full")

    def test_non_pr_events_keep_full_coverage(self):
        for paths in (["README.md"], ["docs/site/index.html"], ["tests/ui/visual/capture.qml"]):
            with self.subTest(paths=paths):
                self.assertEqual(ci_scope.scope(paths, False), "full")

    def test_failed_git_discovery_reports_full_pr_checks_without_packages(self):
        with tempfile.TemporaryDirectory() as directory:
            output = pathlib.Path(directory) / "output"
            result = subprocess.run(
                [sys.executable, str(SCRIPT)], cwd=directory,
                env=dict(os.environ, GITHUB_EVENT_NAME="pull_request", GITHUB_OUTPUT=str(output)),
                capture_output=True, text=True, check=True,
            )
            self.assertIn("scope=full", result.stdout)
            self.assertIn("native=true", output.read_text())
            self.assertIn("visual=true", output.read_text())
            self.assertIn("website=true", output.read_text())
            self.assertIn("package=false", output.read_text())

    def test_event_cli_flags_match_selected_scope(self):
        expected = {
            "README.md": ("docs", "false", "false", "false"),
            "docs/site/index.html": ("website", "false", "true", "false"),
            "tests/ui/visual/capture.qml": ("visual", "false", "true", "true"),
            "apps/arinn-shell/ui/HomeView.qml": ("full", "true", "false", "false"),
        }
        for path, (selected, native, website, visual) in expected.items():
            with self.subTest(path=path), tempfile.TemporaryDirectory() as directory:
                output = pathlib.Path(directory) / "output"
                subprocess.run([sys.executable, str(SCRIPT), path], check=True, capture_output=True,
                               env=dict(os.environ, GITHUB_OUTPUT=str(output)))
                flags = dict(line.split("=", 1) for line in output.read_text().splitlines())
                self.assertEqual((flags["scope"], flags["native"], flags["website"], flags["visual"]),
                                 (selected, native, website, visual))
                self.assertEqual(flags["package"], "false")

    def test_scheduled_manual_push_and_called_workflows_are_full(self):
        for event in ("push", "schedule", "workflow_dispatch", "workflow_call"):
            with self.subTest(event=event), tempfile.TemporaryDirectory() as directory:
                result = subprocess.run([sys.executable, str(SCRIPT)], cwd=directory, check=True,
                                        capture_output=True, text=True,
                                        env=dict(os.environ, GITHUB_OUTPUT="", GITHUB_EVENT_NAME=event))
                self.assertIn("scope=full", result.stdout)
                self.assertIn("native=true", result.stdout)
                self.assertIn("package=true", result.stdout)

    def test_mixed_native_and_capture_changes_run_both_checks(self):
        result = subprocess.run([
            sys.executable, str(SCRIPT), "apps/arinn-shell/ui/HomeView.qml",
            "tests/ui/visual/capture.qml", "docs/site/index.html",
        ], check=True, capture_output=True, text=True, env=dict(os.environ, GITHUB_OUTPUT=""))
        for flag in ("scope=full", "native=true", "website=true", "visual=true", "package=false"):
            self.assertIn(flag, result.stdout)

    def test_git_merge_diff_ignores_unrelated_base_changes(self):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)

            def git(*args):
                return subprocess.run(["git", *args], cwd=root, check=True, text=True, capture_output=True)

            git("init", "--quiet", "--initial-branch=main")
            git("config", "user.name", "Fixture")
            git("config", "user.email", "fixture@example.invalid")
            git("config", "commit.gpgsign", "false")
            git("config", "core.hooksPath", "/dev/null")
            (root / "README.md").write_text("Fixture\n")
            git("add", ".")
            git("commit", "--quiet", "-m", "Base")
            git("switch", "--quiet", "-c", "website")
            (root / "docs/site").mkdir(parents=True)
            (root / "docs/site/index.html").write_text("<!doctype html>\n")
            git("add", ".")
            git("commit", "--quiet", "-m", "Website")
            git("switch", "--quiet", "main")
            (root / "native.rs").write_text("// Unrelated base change\n")
            git("add", ".")
            git("commit", "--quiet", "-m", "Native base")
            git("merge", "--quiet", "--no-ff", "website", "-m", "Merge fixture")
            result = subprocess.run([sys.executable, str(SCRIPT)], cwd=root, check=True, text=True,
                                    capture_output=True,
                                    env=dict(os.environ, GITHUB_OUTPUT="", GITHUB_EVENT_NAME="pull_request"))
            self.assertIn("scope=website", result.stdout)
            self.assertIn("native=false", result.stdout)
            # Rename detection must not hide the removal of a native input.
            git("switch", "--quiet", "-c", "rename")
            git("mv", "native.rs", "docs/site/native.html")
            git("commit", "--quiet", "-m", "Rename native input")
            result = subprocess.run([sys.executable, str(SCRIPT)], cwd=root, check=True, text=True,
                                    capture_output=True,
                                    env=dict(os.environ, GITHUB_OUTPUT="", GITHUB_EVENT_NAME="pull_request"))
            self.assertIn("scope=full", result.stdout)
            self.assertIn("native=true", result.stdout)


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
