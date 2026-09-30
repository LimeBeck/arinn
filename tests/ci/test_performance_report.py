# SPDX-FileCopyrightText: 2026 Arinn contributors
# SPDX-License-Identifier: GPL-3.0-or-later

import importlib.util
import contextlib
import io
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock

SPEC = importlib.util.spec_from_file_location(
    "performance_report", Path(__file__).resolve().parents[2] / "scripts/perf/run.py"
)
perf = importlib.util.module_from_spec(SPEC)
bytecode = sys.dont_write_bytecode
try:
    sys.dont_write_bytecode = True
    SPEC.loader.exec_module(perf)
finally:
    sys.dont_write_bytecode = bytecode


class PerformanceReportTests(unittest.TestCase):
    def raw(self):
        return {
            "schema_version": 1,
            "target_qualification": "not_run",
            "focus_samples_while_read_pending": 20,
            "completed_delayed_reads": 1,
            "navigation_open_ms": [1.0],
            "home_return_ms": [2.0],
            "scenarios": [
                {"name": name, "input_to_focus_ms": [1.0] * 20, "frame_interval_ms": [16.0] * 20}
                for name in ("warm_home", "delayed_read")
            ],
        }

    def test_nearest_rank_p95_keeps_outliers(self):
        result = perf.summarize([1] * 94 + [51] * 6, 50)
        self.assertEqual(result["p95_ms"], 51)
        self.assertFalse(result["within_budget"])
        self.assertEqual(perf.summarize([50] * 20, 50)["within_budget"], True)

    def test_missing_and_invalid_samples_never_pass(self):
        for values in ([], [-1], [float("nan")], [float("inf")], ["1"], [True]):
            with self.subTest(values=values), self.assertRaises(ValueError):
                perf.summarize(values, 50)

    def test_report_has_budgets_but_no_target_acceptance(self):
        result = perf.report(self.raw(), 20)
        self.assertEqual(result["target_qualification"], "not_run")
        self.assertEqual(result["scenarios"][0]["summary"]["frame_interval_ms"]["budget_ms"], 16.7)
        self.assertNotIn("budget_ms", result["home_return_summary"])
        self.assertTrue(result["limitations"])

    def test_incomplete_run_cannot_produce_a_report(self):
        for field, value in (("completed_delayed_reads", 0), ("focus_samples_while_read_pending", 0),
                             ("scenarios", []), ("schema_version", 2)):
            raw = self.raw()
            raw[field] = value
            with self.subTest(field=field), self.assertRaises(ValueError):
                perf.report(raw, 20)
        raw = self.raw()
        raw["scenarios"][1]["input_to_focus_ms"].pop()
        with self.assertRaises(ValueError):
            perf.report(raw, 20)

    def test_failed_run_preserves_existing_report(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "report.json"
            output.write_text("previous report", encoding="utf-8")
            with mock.patch.object(perf.subprocess, "run", return_value=mock.Mock(stdout="{}")), \
                    contextlib.redirect_stderr(io.StringIO()):
                code = perf.main(["--samples", "20", "--executable", "fixture", "--output", str(output)])
            self.assertEqual(code, 1)
            self.assertEqual(output.read_text(encoding="utf-8"), "previous report")

    def test_budget_miss_is_written_as_a_measurement(self):
        raw = self.raw()
        raw["scenarios"][0]["input_to_focus_ms"] = [60.0] * 20
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "report.json"
            with mock.patch.object(perf.subprocess, "run", return_value=mock.Mock(stdout=json.dumps(raw))), \
                    contextlib.redirect_stdout(io.StringIO()):
                code = perf.main(["--samples", "20", "--executable", "fixture", "--output", str(output)])
            self.assertEqual(code, 0)
            result = json.loads(output.read_text(encoding="utf-8"))
            self.assertFalse(result["scenarios"][0]["summary"]["input_to_focus_ms"]["within_budget"])


if __name__ == "__main__":
    unittest.main()
