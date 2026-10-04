#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Arinn contributors
# SPDX-License-Identifier: GPL-3.0-or-later
"""Run the desktop entry launcher (#289) against stub systemctl and arinnctl."""
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]


class ArinnOpen(unittest.TestCase):
    def run_open(self, active, home_failures, home_ever=True,
                 shell_active=True, shell_start_fails=False):
        with tempfile.TemporaryDirectory(prefix="arinn-open-test-") as folder:
            root = Path(folder)
            log = root / "log"
            count = root / "count"
            count.write_text("0")
            systemctl = root / "systemctl"
            systemctl.write_text(
                "#!/bin/sh\n"
                f"echo \"systemctl $*\" >> {log}\n"
                "case \"$*\" in\n"
                f"  *is-active*{active}*) exit 0 ;;\n"
                f"  *is-active*arinn-shell.service*) exit {0 if shell_active else 3} ;;\n"
                f"  *start*arinn-shell.service*) exit {1 if shell_start_fails else 0} ;;\n"
                "  *is-active*) exit 3 ;;\n"
                "esac\n"
                "exit 0\n")
            systemctl.chmod(0o755)
            arinnctl = root / "arinnctl"
            arinnctl.write_text(
                "#!/bin/sh\n"
                f"n=$(cat {count}); n=$((n + 1)); echo $n > {count}\n"
                f"echo \"arinnctl $*\" >> {log}\n"
                f"[ {'1' if home_ever else '0'} = 1 ] && [ $n -gt {home_failures} ]\n")
            arinnctl.chmod(0o755)
            script = root / "arinn-open"
            script.write_text((ROOT / "packaging/bin/arinn-open").read_text()
                              .replace("/usr/bin/arinnctl", str(arinnctl))
                              .replace("sleep 0.25", "sleep 0"))
            env = dict(os.environ, PATH=f"{root}:{os.environ['PATH']}")
            result = subprocess.run(["sh", str(script)], env=env,
                                    capture_output=True, text=True, timeout=10)
            return result, log.read_text().splitlines()

    def test_starts_the_session_when_none_runs(self):
        result, log = self.run_open(active="nothing", home_failures=2)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("systemctl --user start arinn-session.target", log)
        self.assertEqual(log.count("arinnctl home"), 3)

    def test_keeps_a_running_session(self):
        for target in ("arinn-session.target", "arinn-ewmh-session.target"):
            result, log = self.run_open(active=target, home_failures=0)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertFalse(any("start" in line for line in log), log)
            self.assertEqual(log.count("arinnctl home"), 1)

    def test_reopens_a_closed_shell_without_restarting_the_session(self):
        for target in ("arinn-session.target", "arinn-ewmh-session.target"):
            with self.subTest(target=target):
                result, log = self.run_open(active=target, home_failures=0,
                                            shell_active=False)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertIn("systemctl --user start arinn-shell.service", log)
                self.assertFalse(any("start " in line and ".target" in line for line in log), log)
                self.assertFalse(any("enable" in line or "restart" in line for line in log), log)
                self.assertLess(log.index("systemctl --user start arinn-shell.service"),
                                log.index("arinnctl home"))

    def test_failed_shell_start_does_not_report_home_success(self):
        result, log = self.run_open(active="arinn-session.target", home_failures=0,
                                    shell_active=False, shell_start_fails=True)
        self.assertEqual(result.returncode, 1)
        self.assertNotIn("arinnctl home", log)

    def test_gives_up_when_the_daemon_never_answers(self):
        result, log = self.run_open(active="nothing", home_failures=0, home_ever=False)
        self.assertEqual(result.returncode, 1)
        self.assertEqual(log.count("arinnctl home"), 20)
        self.assertIn("did not answer", result.stderr)


if __name__ == "__main__":
    unittest.main()
