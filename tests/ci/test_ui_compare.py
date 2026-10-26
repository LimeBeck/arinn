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


if __name__ == "__main__":
    unittest.main()
