# SPDX-FileCopyrightText: 2026 Arinn contributors
# SPDX-License-Identifier: GPL-3.0-or-later

"""Exercise the actual scoped workflow guards, including failed classification."""

import pathlib
import re
import unittest

from test_ci_build_scope import jobs, value

ROOT = pathlib.Path(__file__).resolve().parents[2]
CI = (ROOT / ".github/workflows/ci.yml").read_text()
PAGES = (ROOT / ".github/workflows/pages.yml").read_text()


def enabled(expression, *, result="success", native="", visual="", website="", cancelled=False):
    body = re.fullmatch(r"\$\{\{(.*)\}\}", expression.strip()).group(1)
    for key, replacement in {
        "needs.scope.result": result,
        "needs.scope.outputs.native": native,
        "needs.scope.outputs.visual": visual,
        "needs.scope.outputs.website": website,
        "vars.ARINN_RUNNER": "",
    }.items():
        body = body.replace(key, repr(replacement))
    body = body.replace("cancelled()", repr(cancelled))
    body = body.replace("&&", " and ").replace("||", " or ")
    body = re.sub(r"!(?!=)", " not ", body)
    assert "needs." not in body and "vars." not in body, body
    return bool(eval(body, {"__builtins__": {}}))  # noqa: S307 - fixed workflow text


def step(job, needle):
    found = [part for part in re.split(r"^      - ", job, flags=re.M)[1:] if needle in part]
    assert len(found) == 1, (needle, len(found))
    return found[0]


class ScopedWorkflowTest(unittest.TestCase):
    def setUp(self):
        self.jobs = jobs(CI)
        self.build = self.jobs["build"]

    def test_website_and_markdown_skip_images_and_native_job(self):
        for selected in ("docs", "website"):
            context = dict(native="false", visual="false", website=str(selected == "website").lower())
            for name in ("images", "build"):
                with self.subTest(scope=selected, job=name):
                    self.assertFalse(enabled(value(self.jobs[name], "if"), **context))

    def test_renderer_uses_the_existing_required_check(self):
        self.assertIn("name: Rust, Qt shell and integration (Arch Linux)", self.build)
        self.assertTrue(enabled(value(self.build, "if"), native="false", visual="true"))
        self.assertTrue(enabled(value(self.jobs["images"], "if"), native="false", visual="true"))
        render = step(self.build, "name: Render visual fixtures")
        self.assertIn("env -u ARINN_CAPTURE_ONLY python", render)
        self.assertIn("test -s build/visual-ci/report.html", render)
        self.assertIn("test -s build/visual-ci/summary.md", render)
        self.assertIn("python scripts/ui-compare.py --build-dir build --output build/visual-ci", render)
        self.assertNotIn("continue-on-error", self.build)

    def test_visual_scope_never_runs_full_native_commands(self):
        for needle in (
            "name: Restore build cache", "name: Link the cached target directory",
            "run: cargo fmt", "name: Package symbol collector tests", "run: cargo clippy",
            "run: cargo test --locked\n", "name: Build Rust workspace", "name: Isolated X11 EWMH tests",
            "name: Build shell", "name: QML component tests", "name: KWin adapter and Chrome",
            "name: Private-bus integration suite",
        ):
            condition = value(step(self.build, needle), "if")
            with self.subTest(step=needle):
                self.assertFalse(enabled(condition, native="false", visual="true"))
                self.assertTrue(enabled(condition, native="true", visual="false"))
                self.assertTrue(enabled(condition))  # absent outputs run native checks
                self.assertTrue(enabled(condition, result="failure", native="false", visual="true"))

    def test_capture_steps_only_run_for_positive_visual_classification(self):
        for needle in ("name: Validate visual fixture manifest", "name: Build visual fixture renderer",
                       "name: Render visual fixtures", "name: Save visual fixture evidence"):
            block = step(self.build, needle)
            condition = value(block, "if")
            with self.subTest(step=needle):
                self.assertTrue(enabled(condition, native="false", visual="true"))
                self.assertTrue(enabled(condition, native="true", visual="true"))
                self.assertFalse(enabled(condition, native="true", visual="false"))
                self.assertFalse(enabled(condition))
                self.assertFalse(enabled(condition, result="failure", native="false", visual="true"))
        renderer = step(self.build, "name: Build visual fixture renderer")
        self.assertIn("cmake --build build --target arinn-ui-test", renderer)
        self.assertNotRegex(renderer, r"cargo (build|test|clippy)")
        evidence = step(self.build, "name: Save visual fixture evidence")
        self.assertIn("retention-days: 7", evidence)
        self.assertIn("path: build/visual-ci", evidence)
        self.assertIn("if-no-files-found: error", evidence)

    def test_failed_missing_or_cancelled_scope_cannot_silently_skip_native_checks(self):
        for name in ("images", "build"):
            condition = value(self.jobs[name], "if")
            with self.subTest(job=name):
                self.assertTrue(enabled(condition))
                self.assertTrue(enabled(condition, result="failure", native="false", visual="false"))
                self.assertFalse(enabled(condition, cancelled=True))

    def test_site_checks_run_for_website_and_visual_scopes(self):
        docs = self.jobs["docs"]
        self.assertIn("needs: scope", docs)
        site = step(docs, "name: Validate public website")
        condition = value(site, "if")
        self.assertTrue(enabled(condition, website="true"))
        self.assertFalse(enabled(condition, website="false"))
        self.assertFalse(enabled(condition, result="failure", website="true"))
        for command in ("check-user-site.py", "build-site.py", "check-site.py"):
            self.assertIn(f"python scripts/{command}", site)

    def test_site_screenshot_only_changes_trigger_pages_validation_and_deployment(self):
        self.assertEqual(PAGES.count("      - docs/design/showcase/*.png\n"), 2)


if __name__ == "__main__":
    unittest.main()
