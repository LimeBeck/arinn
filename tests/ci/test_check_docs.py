# SPDX-FileCopyrightText: 2026 Arinn contributors
#
# SPDX-License-Identifier: GPL-3.0-or-later

"""Tests for the Agent task phase check in scripts/check-docs.py."""

import importlib.util
import pathlib
import tempfile
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "scripts" / "check-docs.py"
spec = importlib.util.spec_from_file_location("check_docs", SCRIPT)
check_docs = importlib.util.module_from_spec(spec)
spec.loader.exec_module(check_docs)

ROADMAP = """# Roadmap

## Phase 0 — Architecture spike

## Phase 1 — Core skeleton

## Backlog
"""

TEMPLATE = """name: Agent task
body:
  - type: dropdown
    id: phase
    attributes:
      label: Roadmap phase
      options:
        - Phase 0 — Architecture spike
        - "Phase 1 — Core skeleton"
        - Backlog
        - Not assigned
    validations:
      required: true
  - type: textarea
    id: goal
"""


class PhaseOptionsTest(unittest.TestCase):
    def test_roadmap_phases_follow_the_headings(self):
        self.assertEqual(check_docs.roadmap_phases(ROADMAP), [
            "Phase 0 — Architecture spike",
            "Phase 1 — Core skeleton",
            "Backlog",
            "Not assigned",
        ])

    def test_template_options_are_read_from_the_phase_dropdown(self):
        self.assertEqual(check_docs.template_phases(TEMPLATE), check_docs.roadmap_phases(ROADMAP))

    def test_template_without_the_field_has_no_options(self):
        self.assertEqual(check_docs.template_phases("name: Agent task\nbody:\n  - type: textarea\n    id: goal\n"), [])

    def test_repository_template_matches_the_roadmap(self):
        roadmap = check_docs.roadmap_phases(check_docs.ROADMAP.read_text(encoding="utf-8"))
        template = check_docs.template_phases(check_docs.TASK_TEMPLATE.read_text(encoding="utf-8"))
        self.assertIn("Phase 5 — Product MVP", roadmap)
        self.assertEqual(template, roadmap)

    def test_renamed_phase_is_reported(self):
        saved = check_docs.ROADMAP, check_docs.TASK_TEMPLATE
        with tempfile.TemporaryDirectory() as tmp:
            roadmap = pathlib.Path(tmp) / "ROADMAP.md"
            template = pathlib.Path(tmp) / "agent_task.yml"
            roadmap.write_text(ROADMAP.replace("Core skeleton", "Core"), encoding="utf-8")
            template.write_text(TEMPLATE, encoding="utf-8")
            check_docs.ROADMAP, check_docs.TASK_TEMPLATE = roadmap, template
            check_docs.ROOT, saved_root = pathlib.Path(tmp), check_docs.ROOT
            try:
                check_docs.errors.clear()
                check_docs.check_task_template()
                self.assertEqual(len(check_docs.errors), 1)
                self.assertIn("'Roadmap phase' options must be", check_docs.errors[0])
            finally:
                check_docs.ROADMAP, check_docs.TASK_TEMPLATE = saved
                check_docs.ROOT = saved_root
                check_docs.errors.clear()

    def test_repository_template_is_a_valid_issue_form(self):
        try:
            import yaml
        except ImportError:
            self.skipTest("PyYAML is not installed")
        form = yaml.safe_load(check_docs.TASK_TEMPLATE.read_text(encoding="utf-8"))
        fields = [field for field in form["body"] if field["type"] != "markdown"]
        self.assertEqual(fields[0]["id"], "phase")
        self.assertEqual(fields[0]["type"], "dropdown")
        self.assertTrue(fields[0]["validations"]["required"])
        self.assertEqual(fields[0]["attributes"]["options"], check_docs.roadmap_phases(check_docs.ROADMAP.read_text(encoding="utf-8")))


if __name__ == "__main__":
    unittest.main()
