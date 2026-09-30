#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Arinn contributors
# SPDX-License-Identifier: GPL-3.0-or-later

"""Build the isolated Home measurement fixture and summarize its samples."""

import argparse
import json
import math
import os
from pathlib import Path
import subprocess
import sys
import tempfile

BUDGETS_MS = {"input_to_focus_ms": 50.0, "frame_interval_ms": 16.7}


def summarize(samples, budget=None):
    """Nearest-rank p95, retaining misses instead of filtering outliers."""
    if not samples or any(
        isinstance(value, bool)
        or not isinstance(value, (int, float))
        or not math.isfinite(value)
        or value < 0
        for value in samples
    ):
        raise ValueError("missing_or_invalid_samples")
    ordered = sorted(samples)
    result = {
        "count": len(ordered),
        "min_ms": ordered[0],
        "p50_ms": ordered[math.ceil(len(ordered) * 0.50) - 1],
        "p95_ms": ordered[math.ceil(len(ordered) * 0.95) - 1],
        "max_ms": ordered[-1],
    }
    if budget is not None:
        result.update(budget_ms=budget, within_budget=result["p95_ms"] <= budget)
    return result


def report(raw, samples):
    if raw.get("schema_version") != 1 or raw.get("target_qualification") != "not_run":
        raise ValueError("unsupported_report")
    if (
        raw.get("focus_samples_while_read_pending", 0) < 1
        or raw.get("completed_delayed_reads", 0) < 1
    ):
        raise ValueError("delayed_read_not_exercised")
    scenarios = raw.get("scenarios", [])
    if [row.get("name") for row in scenarios] != ["warm_home", "delayed_read"]:
        raise ValueError("missing_scenario")
    for row in scenarios:
        if len(row.get("input_to_focus_ms", [])) != samples:
            raise ValueError("incomplete_focus_measurement")
        row["summary"] = {
            name: summarize(row.get(name, []), budget)
            for name, budget in BUDGETS_MS.items()
        }
    raw["navigation_open_summary"] = summarize(raw.get("navigation_open_ms", []))
    raw["home_return_summary"] = summarize(raw.get("home_return_ms", []))
    raw["limitations"] = [
        "Input timing starts at Qt key dispatch; device/compositor delivery is excluded.",
        "Frame intervals are frameSwapped cadence, not GPU execution time.",
        "Offscreen/software rendering is harness validation, not target performance.",
        "The delayed read is a fixture; production model/daemon routing is not measured.",
        "Navigation timings end at local state/focus change, not animation completion.",
        "G-5.4 requires a separate owner-run report on the target HTPC at 60 Hz.",
    ]
    return raw


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--samples", type=int, default=200)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--build-dir", type=Path)
    parser.add_argument("--executable", type=Path, help="Use an already-built fixture (CTest).")
    parser.add_argument("--platform", choices=("offscreen", "wayland", "xcb"), default="offscreen")
    parser.add_argument("--backend", choices=("software", "default"), default="software")
    args = parser.parse_args(argv)
    if not 20 <= args.samples <= 10000:
        parser.error("--samples must be between 20 and 10000")
    source = Path(__file__).resolve().parent
    try:
        with tempfile.TemporaryDirectory(prefix="arinn-perf-build-") as temporary:
            build = args.build_dir.resolve() if args.build_dir else Path(temporary)
            if not args.executable:
                commands = (
                    ["cmake", "-S", str(source), "-B", str(build), "-G", "Ninja",
                     "-DCMAKE_BUILD_TYPE=Release"],
                    ["cmake", "--build", str(build)],
                )
                for command in commands:
                    subprocess.run(command, check=True, stdout=sys.stderr, timeout=300)
            env = os.environ.copy()
            env["QT_QPA_PLATFORM"] = args.platform
            if args.backend == "software":
                env["QT_QUICK_BACKEND"] = "software"
            else:
                env.pop("QT_QUICK_BACKEND", None)
            executable = args.executable.resolve() if args.executable else build / "arinn-perf"
            measured = subprocess.run(
                ["dbus-run-session", "--", str(executable), "--samples", str(args.samples)],
                env=env, check=True, capture_output=True, text=True,
                timeout=20 + args.samples * 0.25,
            )
            result = report(json.loads(measured.stdout), args.samples)
            result["requested_backend"] = args.backend
            args.output.parent.mkdir(parents=True, exist_ok=True)
            # Reports contain fixture timings, never hostnames, home paths or
            # provider payloads. Preserve an existing report if this run fails.
            with tempfile.NamedTemporaryFile(
                mode="w", dir=args.output.parent, delete=False, encoding="utf-8"
            ) as output:
                temporary_output = Path(output.name)
                json.dump(result, output, indent=2, allow_nan=False)
                output.write("\n")
            temporary_output.replace(args.output)
    except (OSError, subprocess.SubprocessError, ValueError, KeyError, TypeError):
        # Qt stderr can contain local paths. Do not copy it into a report or
        # diagnostic log; run the fixture directly when debugging locally.
        print("performance_measurement_failed; no report written", file=sys.stderr)
        return 1
    print("performance_report_written")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
