# SPDX-FileCopyrightText: 2026 Arinn contributors
# SPDX-License-Identifier: GPL-3.0-or-later
"""Check publication boundaries and links that break GitHub Pages project sites."""

import contextlib
import importlib.util
import io
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def load(name):
    spec = importlib.util.spec_from_file_location(name, ROOT / f"scripts/{name}.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


BUILDER = load("build-site")
CHECKER = load("check-site")


class ProductSiteTest(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.site = Path(self.directory.name) / "site"
        with contextlib.redirect_stdout(io.StringIO()):
            BUILDER.build(self.site)

    def change_html(self, content):
        page = self.site / "index.html"
        page.write_text(page.read_text().replace("</main>", f"{content}</main>"))

    def test_assembled_guide_links_and_current_captures(self):
        self.assertEqual(CHECKER.check(self.site), [])
        guide = (self.site / "guide/index.html").read_text()
        self.assertIn('href="../index.html"', guide)
        self.assertEqual((self.site / "assets/screenshots/home.png").read_bytes(),
                         (ROOT / "docs/design/current-ui/home.png").read_bytes())

    def test_project_root_links_are_rejected(self):
        self.change_html('<a href="/guide/index.html">Guide</a>')
        self.assertTrue(any("root-relative" in error for error in CHECKER.check(self.site)))

    def test_missing_chapter_and_assets_are_rejected(self):
        self.change_html('<a href="guide/index.html#missing-chapter">Guide</a>')
        (self.site / "assets/screenshots/settings.png").unlink()
        errors = CHECKER.check(self.site)
        self.assertTrue(any("missing anchor" in error for error in errors))
        self.assertTrue(any("missing or out-of-site" in error for error in errors))

    def test_extra_repository_files_cannot_be_published(self):
        (self.site / "private-config.toml").write_text("synthetic fixture")
        self.assertTrue(any("extra=" in error for error in CHECKER.check(self.site)))
        with self.assertRaisesRegex(ValueError, "unexpected files"):
            BUILDER.build(self.site)

    def test_encoded_escape_and_remote_runtime_assets_are_rejected(self):
        self.change_html('<a href="%2e%2e/outside.txt">Outside</a>'
                         '<link rel="stylesheet" href="https://example.org/style.css">')
        errors = CHECKER.check(self.site)
        self.assertTrue(any("out-of-site" in error for error in errors))
        self.assertTrue(any("third-party runtime asset" in error for error in errors))

    def test_symlinks_are_rejected_before_overwriting(self):
        outside = Path(self.directory.name) / "outside.txt"
        outside.write_text("synthetic fixture")
        target = self.site / "assets/site.css"
        target.unlink()
        target.symlink_to(outside)
        self.assertTrue(any("symlinks" in error for error in CHECKER.check(self.site)))
        with self.assertRaisesRegex(ValueError, "escapes"):
            BUILDER.build(self.site)
        self.assertEqual(outside.read_text(), "synthetic fixture")


if __name__ == "__main__":
    unittest.main()
