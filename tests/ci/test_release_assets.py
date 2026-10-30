# SPDX-FileCopyrightText: 2026 Arinn contributors
# SPDX-License-Identifier: GPL-3.0-or-later

"""Only distributable packages and source archives reach publication."""
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "scripts/stage-release-assets.py"


class ReleaseAssetsTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.source = self.root / "incoming"
        self.source.mkdir()
        self.output = self.root / "dist"
        self.products = {
            "arinn-0.2.0-1-x86_64.pkg.tar.zst",
            "arinn-debug-0.2.0-1-x86_64.pkg.tar.zst",
            "arinn-0.2.0.tar.gz",
            "arinn_0.2.0-1_amd64.deb",
            "arinn-dbgsym_0.2.0-1_amd64.deb",
        }
        for name in self.products:
            (self.source / name).write_bytes(name.encode())

    def stage(self, *args):
        return subprocess.run(
            [sys.executable, str(SCRIPT), str(self.source), str(self.output), *args],
            capture_output=True, text=True,
        )

    def test_mixed_ci_artifact_excludes_helpers_and_old_checksums(self):
        for name in ("private-systemd", "README", "SHA256SUMS", "other.tar.gz", "other.deb"):
            (self.source / name).write_text("internal CI fixture")
        result = self.stage("--require-debian")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual({p.name for p in self.output.iterdir()}, self.products)
        for name in self.products:
            self.assertEqual((self.output / name).read_bytes(), (self.source / name).read_bytes())
        self.assertTrue((self.source / "private-systemd").exists())

    def test_snapshot_does_not_require_debian(self):
        (self.source / "arinn_0.2.0-1_amd64.deb").unlink()
        result = self.stage()
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_missing_required_category_fails_before_creating_output(self):
        for name in ("arinn-0.2.0-1-x86_64.pkg.tar.zst", "arinn-0.2.0.tar.gz", "arinn_0.2.0-1_amd64.deb"):
            path = self.source / name
            original = path.read_bytes()
            path.unlink()
            # Debug packages cannot replace the main Arch package.
            result = self.stage("--require-debian")
            self.assertNotEqual(result.returncode, 0)
            self.assertFalse(self.output.exists())
            path.write_bytes(original)

    def test_selected_symlink_is_rejected(self):
        name = "arinn-0.2.0.tar.gz"
        (self.source / name).unlink()
        outside = self.root / "outside"
        outside.write_text("not a build artifact")
        (self.source / name).symlink_to(outside)
        result = self.stage()
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(self.output.exists())

    def test_stale_output_is_rejected(self):
        self.output.mkdir()
        (self.output / "private-systemd").write_text("old artifact")
        self.assertNotEqual(self.stage().returncode, 0)
        self.assertEqual(list(self.output.iterdir()), [self.output / "private-systemd"])

    def test_both_publication_workflows_stage_before_publishing(self):
        for filename, flag in (("release.yml", " --require-debian"), ("ci.yml", "")):
            text = (ROOT / ".github/workflows" / filename).read_text()
            publication = text[text.index("  publish:" if filename == "release.yml" else "  snapshot:"):]
            command = "python scripts/stage-release-assets.py incoming dist" + flag
            self.assertIn(command, publication)
            self.assertLess(publication.index(command), publication.index("files: dist/*"))
            self.assertNotIn("path: dist", publication)


if __name__ == "__main__":
    unittest.main()
