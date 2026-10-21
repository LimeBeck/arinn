#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Arinn contributors
# SPDX-License-Identifier: GPL-3.0-or-later
"""Tell the user when an Arinn unit hits its start limit (#437).

    python tests/package/test_session_failed.py

Runs the OnFailure= handler against stub systemctl and busctl and checks the
session units, without a user manager or a notification server.
ARINN_SESSION_FAILED points at an installed handler instead of the source.
"""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
UNITS = ROOT / "packaging/systemd"
HANDLER = Path(os.environ.get("ARINN_SESSION_FAILED", UNITS / "arinn-session-failed"))
WATCHED = ["arinnd.service", "arinn-shell.service"]


def unit(name):
    sections, section = {}, None
    for line in (UNITS / name).read_text().splitlines():
        line = line.strip()
        if line.startswith("[") and line.endswith("]"):
            section = sections.setdefault(line[1:-1], {})
        elif line and not line.startswith("#"):
            key, value = line.split("=", 1)
            section.setdefault(key, []).append(value)
    return sections


class Handler(unittest.TestCase):
    def run_handler(self, name, monitor=None, shown="", notify_fails=False):
        with tempfile.TemporaryDirectory(prefix="arinn-failed-test-") as folder:
            root = Path(folder)
            log = root / "log"
            log.write_text("")
            for tool, body in (
                    ("systemctl", f'echo "systemctl $*" >> {log}\n'
                                  f'case "$*" in *show*Result*) echo "{shown}" ;; esac\n'),
                    ("busctl", f'for a in "$@"; do printf "%s\\n" "$a"; done > {root}/notify\n'
                               f'echo "busctl $1 $2 $3 $4" >> {log}\n'
                               f'exit {1 if notify_fails else 0}\n')):
                path = root / tool
                path.write_text("#!/bin/sh\n" + body)
                path.chmod(0o755)
            env = dict(os.environ, PATH=f"{root}:{os.environ['PATH']}")
            env.pop("MONITOR_SERVICE_RESULT", None)
            if monitor is not None:
                env["MONITOR_SERVICE_RESULT"] = monitor
            result = subprocess.run(["sh", str(HANDLER), name], env=env, stdin=subprocess.DEVNULL,
                                    capture_output=True, text=True, timeout=10)
            notify = (root / "notify").read_text().splitlines() if (root / "notify").exists() else None
            return result, log.read_text().splitlines(), notify

    def test_start_limit_shows_a_notification(self):
        for name in WATCHED:
            result, log, notify = self.run_handler(name, monitor="start-limit-hit")
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(log, ["busctl --user --timeout=10 call org.freedesktop.Notifications"])
            self.assertEqual(notify[:5], ["--user", "--timeout=10", "call", "org.freedesktop.Notifications",
                                          "/org/freedesktop/Notifications"])
            self.assertEqual(notify[5:9], ["org.freedesktop.Notifications", "Notify", "susssasa{sv}i", "Arinn"])
            self.assertEqual(notify[9:12], ["0", "org.arinn.Shell", "Arinn stopped after repeated failures"])
            body = notify[12]
            self.assertIn("arinnctl diagnostics", body)
            self.assertIn(f"journalctl --user -u {name}", body)
            self.assertIn(f"systemctl --user reset-failed {name}, then systemctl --user start {name}", body)
            # Plain text: no markup a notification server could misread.
            self.assertFalse(set(body) & set("<>&"), body)
            self.assertEqual(notify[13:], ["0", "2", "urgency", "y", "2", "desktop-entry", "s",
                                           "org.arinn.Shell", "0"])
            self.assertEqual(result.stdout, "")
            self.assertEqual(result.stderr, f"arinn-session-failed: {name} stopped after repeated failures\n")

    def test_reads_the_result_when_systemd_does_not_pass_it(self):
        result, log, notify = self.run_handler("arinnd.service", shown="start-limit-hit")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(log[0], "systemctl --user show --property=Result --value arinnd.service")
        self.assertIsNotNone(notify)

    def test_exit_status_2_stays_silent_as_before(self):
        # RestartPreventExitStatus=2 leaves the shell failed with Result=exit-code;
        # that is a duplicate shell or a wrong backend, not a crash loop.
        for monitor, shown in (("exit-code", ""), (None, "exit-code"), (None, "")):
            result, log, notify = self.run_handler("arinn-shell.service", monitor=monitor, shown=shown)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIsNone(notify)
            self.assertFalse(any("start" in line or "restart" in line for line in log), log)
            self.assertIn("below its start limit; no notice", result.stderr)

    def test_missing_notification_service_is_logged(self):
        result, _, notify = self.run_handler("arinn-shell.service", monitor="start-limit-hit",
                                             notify_fails=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIsNotNone(notify)
        self.assertIn("no notification service", result.stderr)

    @unittest.skipUnless(all(shutil.which(tool) for tool in ("busctl", "dbus-run-session", "dbus-monitor")),
                         "needs busctl, dbus-run-session and dbus-monitor")
    def test_real_busctl_sends_a_notify_call(self):
        # The stub above cannot check the argument encoding; the real busctl
        # sends the call on a private bus with no notification server.
        with tempfile.TemporaryDirectory(prefix="arinn-failed-bus-") as folder:
            log = Path(folder) / "monitor"
            script = (
                'dbus-monitor --session interface=org.freedesktop.Notifications > "$LOG" 2>&1 &\n'
                'i=0; until grep -q NameLost "$LOG" || [ $i -ge 100 ]; do sleep 0.1; i=$((i+1)); done\n'
                'MONITOR_SERVICE_RESULT=start-limit-hit sh "$HANDLER" arinnd.service\n'
                'i=0; until grep -q "^   int32 0" "$LOG" || [ $i -ge 100 ]; do sleep 0.1; i=$((i+1)); done\n'
                'kill $!\n')
            env = dict(os.environ, LOG=str(log), HANDLER=str(HANDLER))
            result = subprocess.run(["dbus-run-session", "--", "sh", "-c", script], env=env,
                                    stdin=subprocess.DEVNULL, capture_output=True, text=True, timeout=60)
            self.assertIn("no notification service", result.stderr)
            lines = [" ".join(line.split()) for line in log.read_text().splitlines()]
        start = next(i for i, line in enumerate(lines) if line.startswith("method call") and "member=Notify" in line)
        self.assertIn("path=/org/freedesktop/Notifications", lines[start])
        call = lines[start + 1:]
        call = call[:call.index("int32 0") + 1]
        self.assertEqual(call[:4], ['string "Arinn"', "uint32 0", 'string "org.arinn.Shell"',
                                    'string "Arinn stopped after repeated failures"'])
        self.assertTrue(call[4].startswith('string "arinnd.service failed repeatedly'), call[4])
        self.assertEqual(call[5:7], ["array [", "]"])
        self.assertEqual(call[7:], ["array [", "dict entry(", 'string "urgency"', "variant byte 2", ")",
                                    "dict entry(", 'string "desktop-entry"', 'variant string "org.arinn.Shell"',
                                    ")", "]", "int32 0"])

    def test_other_units_are_refused(self):
        for name in ("", "other.service", "arinn-kwin.service", "arinnd.service; true"):
            result, log, notify = self.run_handler(name, monitor="start-limit-hit")
            self.assertEqual(result.returncode, 2, name)
            self.assertIsNone(notify)
            self.assertEqual(log, [])


class Units(unittest.TestCase):
    def test_daemon_and_shell_report_the_start_limit(self):
        for name in WATCHED:
            sections = unit(name)
            self.assertEqual(sections["Unit"]["OnFailure"], ["arinn-session-failed@%n.service"], name)
            self.assertEqual(sections["Unit"]["StartLimitBurst"], ["5"], name)
            self.assertEqual(sections["Unit"]["StartLimitIntervalSec"], ["60s"], name)
            self.assertEqual(sections["Service"]["Restart"], ["on-failure"], name)

    def test_restart_statuses_are_unchanged(self):
        # ADR-0098: 75 restarts the shell whatever Restart= says; #75: 2 never
        # restarts it.
        shell = unit("arinn-shell.service")["Service"]
        self.assertEqual(shell["RestartForceExitStatus"], ["75"])
        self.assertEqual(shell["RestartPreventExitStatus"], ["2"])
        self.assertEqual(shell["ExecStopPost"], ["/usr/lib/arinn/arinn-shell-launch --stopped"])
        self.assertNotIn("SuccessExitStatus", shell)
        self.assertNotIn("RestartMode", shell)

    def test_handler_runs_once_and_restarts_nothing(self):
        sections = unit("arinn-session-failed@.service")
        self.assertEqual(sections["Service"]["Type"], ["oneshot"])
        self.assertEqual(sections["Service"]["ExecStart"], ["/usr/lib/arinn/arinn-session-failed %i"])
        self.assertNotIn("Restart", sections["Service"])
        self.assertNotIn("OnFailure", sections["Unit"])
        self.assertNotIn("Install", sections)


if __name__ == "__main__":
    unittest.main()
