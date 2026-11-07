# SPDX-FileCopyrightText: 2026 Arinn contributors
#
# SPDX-License-Identifier: GPL-3.0-or-later

"""Tests for scripts/ui-compare.py and its manifest (#120, #513).

The docs job runs this suite, so a handoff screenshot added without a
manifest entry, or a rendered state without a capture scene, fails CI.
"""

import importlib.util
import pathlib
import re
import tempfile
import unittest
from contextlib import redirect_stderr
from io import StringIO
from unittest.mock import patch

ROOT = pathlib.Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "scripts" / "ui-compare.py"
spec = importlib.util.spec_from_file_location("ui_compare", SCRIPT)
ui_compare = importlib.util.module_from_spec(spec)
spec.loader.exec_module(ui_compare)


class RepositoryManifestTest(unittest.TestCase):
    def test_every_handoff_file_is_mapped_or_skipped(self):
        self.assertEqual(ui_compare.validate(ui_compare.load_manifest()), [])

    def test_every_rendered_state_has_a_capture_scene(self):
        capture = ui_compare.CAPTURE_QML.read_text(encoding="utf-8")
        scenes = set(re.findall(r'id === "([a-z0-9-]+)"', capture))
        manifest = ui_compare.load_manifest()
        missing = [s["id"] for s in manifest["states"] if s.get("render") and s["id"] not in scenes]
        self.assertEqual(missing, [])

    def test_capture_does_not_use_a_removed_power_step(self):
        # #478 moved the ambient screen out of PowerMenu; its "ambient" step
        # no longer exists and rendered an empty panel (#513).
        capture = ui_compare.CAPTURE_QML.read_text(encoding="utf-8")
        self.assertNotIn('step: "ambient"', capture)


class ValidateTest(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        screenshots = pathlib.Path(self.tmp.name)
        for name in ("a.png", "b.png"):
            (screenshots / name).write_bytes(b"")
        self.saved = ui_compare.SCREENSHOTS
        ui_compare.SCREENSHOTS = screenshots
        self.addCleanup(setattr, ui_compare, "SCREENSHOTS", self.saved)

    @staticmethod
    def manifest(*states):
        return {"version": 1, "states": list(states)}

    def test_complete_manifest_passes(self):
        errors = ui_compare.validate(self.manifest(
            {"id": "one", "reference": "a.png", "render": True},
            {"id": "two", "reference": "b.png", "render": False, "reason": "Not implemented."},
            {"id": "three", "reference": None, "render": True},
        ))
        self.assertEqual(errors, [])

    def test_unmapped_handoff_file_fails(self):
        errors = ui_compare.validate(self.manifest(
            {"id": "one", "reference": "a.png", "render": True},
        ))
        self.assertEqual(errors, ["handoff files not in the manifest: b.png"])

    def test_skipped_state_needs_a_reason(self):
        errors = ui_compare.validate(self.manifest(
            {"id": "one", "reference": "a.png", "render": True},
            {"id": "two", "reference": "b.png", "render": False},
        ))
        self.assertEqual(errors, ["two: a state that is not rendered needs a reason"])

    def test_file_mapped_twice_fails(self):
        errors = ui_compare.validate(self.manifest(
            {"id": "one", "reference": "a.png", "render": True},
            {"id": "two", "reference": "a.png", "render": True},
            {"id": "three", "reference": "b.png", "render": True},
        ))
        self.assertEqual(errors, ["a.png is mapped 2 times"])


class FourKCaptureTest(unittest.TestCase):
    def test_failed_or_incomplete_probe_cannot_pass_or_be_hidden_by_a_retry(self):
        # A QTest cleanup failure can follow valid capture records. The CI
        # gate must retain that failure, even with a correctly sized frame.
        full_size = [{"id": "home", "width": 3840, "height": 2160}]
        for code, rows in ((1, full_size), (0, [])):
            with self.subTest(code=code, rows=rows), patch.object(
                ui_compare, "run_capture", return_value=(code, "probe failed", rows)
            ), patch.object(ui_compare, "require_capture") as fallback, redirect_stderr(StringIO()):
                with self.assertRaisesRegex(SystemExit, "capture failed for 4k"):
                    ui_compare.four_k_captures(pathlib.Path("renderer"), pathlib.Path("out/4k"))
                fallback.assert_not_called()

    def test_clean_logical_probe_uses_the_offscreen_layout_fallback(self):
        logical = [{"id": "home", "width": 1920, "height": 1080}]
        physical = [{"id": "home", "width": 3840, "height": 2160}]
        with patch.object(ui_compare, "run_capture", return_value=(0, "", logical)), patch.object(
            ui_compare, "require_capture", return_value=physical
        ) as fallback:
            rows, method = ui_compare.four_k_captures(pathlib.Path("renderer"), pathlib.Path("out/4k"))
        self.assertEqual(rows, physical)
        self.assertIn("2× scale", method)
        fallback.assert_called_once_with(pathlib.Path("renderer"), pathlib.Path("out/4k"), "4k-layout", None)


if __name__ == "__main__":
    unittest.main()
