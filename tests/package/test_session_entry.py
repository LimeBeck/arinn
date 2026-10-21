#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Arinn contributors
# SPDX-License-Identifier: GPL-3.0-or-later
"""The dedicated Arinn session entry and the session switch (ADR-0107).

Runs `arinn-session` and `arinn-session-switch` against stub systemctl,
busctl and desktop commands in a temporary home; nothing touches the real
desktop, user manager or display manager. ARINN_SESSION_SCRIPT and
ARINN_SESSION_SWITCH select installed copies instead of the sources.
"""
import os
from pathlib import Path
import signal
import socket
import stat
import subprocess
import tempfile
import time
import unittest

ROOT = Path(__file__).resolve().parents[2]
SESSION = Path(os.environ.get("ARINN_SESSION_SCRIPT", ROOT / "packaging/bin/arinn-session"))
SWITCH = Path(os.environ.get("ARINN_SESSION_SWITCH", ROOT / "packaging/bin/arinn-session-switch"))

SYSTEMCTL = """#!/bin/sh
echo "systemctl $*" >> "$FAKE_LOG"
case "$*" in
*"is-active --quiet graphical-session.target"*)
    n=$(cat "$FAKE_ROOT/ready"); n=$((n + 1)); echo $n > "$FAKE_ROOT/ready"
    [ "$n" -gt "${FAKE_READY_AFTER:-0}" ]; exit $? ;;
*"start arinn-session.target"*) exit "${FAKE_START_RC:-0}" ;;
esac
exit 0
"""

BUSCTL = """#!/bin/sh
echo "busctl $*" >> "$FAKE_LOG"
case "$*" in
*"status org.xfce.SessionManager"*)
    n=$(cat "$FAKE_ROOT/ready"); n=$((n + 1)); echo $n > "$FAKE_ROOT/ready"
    [ "$n" -gt "${FAKE_READY_AFTER:-0}" ]; exit $? ;;
*" call "*) exit "${FAKE_CALL_RC:-0}" ;;
esac
exit 9
"""

# The desktop runs until the test creates `stop`, or the wrapper forwards a
# termination signal; it reports the bus address it was started with.
DESKTOP = """#!/bin/sh
echo "desktop $(basename "$0") bus=${DBUS_SESSION_BUS_ADDRESS:-}" >> "$FAKE_LOG"
trap 'echo "desktop terminated" >> "$FAKE_LOG"; exit 0' TERM
n=0
while [ ! -f "$FAKE_ROOT/stop" ] && [ $n -lt 400 ]; do sleep 0.05; n=$((n + 1)); done
exit "${FAKE_DESKTOP_STATUS:-0}"
"""

XFCE_HELPER = """#!/bin/sh
echo "xfce-helper" >> "$FAKE_LOG"
"""


class Fixture:
    def __init__(self, test):
        folder = tempfile.TemporaryDirectory(prefix="arinn-session-test-")
        test.addCleanup(folder.cleanup)
        self.root = Path(folder.name)
        self.bin = self.root / "bin"
        self.bin.mkdir()
        self.home = self.root / "home"
        self.home.mkdir()
        self.log = self.root / "log"
        self.log.touch()
        (self.root / "ready").write_text("0")
        for name, text in (("systemctl", SYSTEMCTL), ("busctl", BUSCTL),
                           ("startplasma-wayland", DESKTOP), ("startxfce4", DESKTOP),
                           ("arinn-xfce-session", XFCE_HELPER)):
            self.executable(self.bin / name, text)
        self.session = self.executable(
            self.root / "arinn-session",
            SESSION.read_text()
            .replace("/usr/lib/arinn/arinn-xfce-session", str(self.bin / "arinn-xfce-session"))
            .replace("sleep 0.5", "sleep 0"))
        self.switch = self.executable(self.root / "arinn-session-switch", SWITCH.read_text())

    @staticmethod
    def executable(path, text):
        path.write_text(text)
        path.chmod(0o755)
        return path

    def env(self, **extra):
        env = {
            "PATH": f"{self.bin}:/usr/bin:/bin",
            "HOME": str(self.home),
            "FAKE_LOG": str(self.log),
            "FAKE_ROOT": str(self.root),
        }
        env.update({key: str(value) for key, value in extra.items() if value is not None})
        return env

    @property
    def mode_file(self):
        return self.home / ".local/state/arinn/session-mode"

    def calls(self):
        return self.log.read_text().splitlines()

    def desktop(self):
        """The desktop's start line; it runs concurrently with the readiness checks."""
        return [c for c in self.calls() if c.startswith("desktop ")]

    def wait_for(self, predicate, timeout=10):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            if predicate(self.calls()):
                return True
            time.sleep(0.02)
        return False

    def stop_desktop(self):
        (self.root / "stop").touch()


class SessionEntry(unittest.TestCase):
    def start(self, fixture, entry, **env):
        process = subprocess.Popen([str(fixture.session), entry], env=fixture.env(**env),
                                   stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        self.addCleanup(lambda: process.poll() is None and process.kill())
        return process

    def finish(self, process):
        out, err = process.communicate(timeout=20)
        return process.returncode, err

    def test_plasma_starts_arinn_after_the_graphical_session(self):
        fixture = Fixture(self)
        process = self.start(fixture, "plasma", FAKE_READY_AFTER=3, FAKE_DESKTOP_STATUS=3)
        self.assertTrue(fixture.wait_for(lambda c: "systemctl --user start arinn-session.target" in c),
                        fixture.calls())
        fixture.stop_desktop()
        code, err = self.finish(process)
        calls = fixture.calls()
        self.assertEqual(code, 3, err)
        self.assertEqual(fixture.desktop(), ["desktop startplasma-wayland bus="], calls)
        ready = [c for c in calls if "is-active --quiet graphical-session.target" in c]
        self.assertEqual(len(ready), 4, calls)
        marker = calls.index("systemctl --user set-environment ARINN_SESSION_ENTRY=plasma")
        self.assertLess(marker, calls.index("systemctl --user start arinn-session.target"))
        self.assertEqual(calls[-2:], ["systemctl --user stop arinn-session.target",
                                      "systemctl --user unset-environment ARINN_SESSION_ENTRY"])
        self.assertNotIn("xfce-helper", calls)

    def test_xfce_starts_the_helper_once_the_session_manager_is_up(self):
        fixture = Fixture(self)
        process = self.start(fixture, "xfce", FAKE_READY_AFTER=2)
        self.assertTrue(fixture.wait_for(lambda c: "xfce-helper" in c), fixture.calls())
        fixture.stop_desktop()
        code, err = self.finish(process)
        calls = fixture.calls()
        self.assertEqual(code, 0, err)
        self.assertEqual(fixture.desktop(), ["desktop startxfce4 bus="], calls)
        self.assertEqual(len([c for c in calls if "status org.xfce.SessionManager" in c]), 3, calls)
        self.assertIn("systemctl --user set-environment ARINN_SESSION_ENTRY=xfce", calls)
        # The helper owns the EWMH target; the entry starts no KDE target.
        self.assertFalse(any("arinn-session.target" in c for c in calls), calls)
        self.assertEqual(calls[-1], "systemctl --user unset-environment ARINN_SESSION_ENTRY")

    def test_desktop_mode_starts_the_plain_desktop(self):
        for entry, desktop in (("plasma", "startplasma-wayland"), ("xfce", "startxfce4")):
            with self.subTest(entry=entry):
                fixture = Fixture(self)
                fixture.mode_file.parent.mkdir(parents=True)
                fixture.mode_file.write_text("desktop\n")
                fixture.stop_desktop()
                code, err = self.finish(self.start(fixture, entry, FAKE_DESKTOP_STATUS=4))
                self.assertEqual(code, 4, err)
                calls = fixture.calls()
                self.assertEqual(len(calls), 1, calls)
                self.assertTrue(calls[0].startswith(f"desktop {desktop}"), calls)

    def test_state_home_is_honoured(self):
        fixture = Fixture(self)
        state = fixture.root / "state"
        (state / "arinn").mkdir(parents=True)
        (state / "arinn/session-mode").write_text("desktop\n")
        fixture.stop_desktop()
        self.finish(self.start(fixture, "plasma", XDG_STATE_HOME=state))
        self.assertEqual(len(fixture.calls()), 1, fixture.calls())

    def test_anything_but_desktop_starts_arinn(self):
        for content in ("", "arinn\n", "Desktop\n", "desktop extra\n", " desktop\n", "\n",
                        "desktop\nextra\n", "desktop\n\narinn\n", "desktop" + " " * 20):
            with self.subTest(content=content):
                fixture = Fixture(self)
                fixture.mode_file.parent.mkdir(parents=True)
                fixture.mode_file.write_text(content)
                process = self.start(fixture, "plasma")
                self.assertTrue(fixture.wait_for(
                    lambda c: "systemctl --user start arinn-session.target" in c), fixture.calls())
                fixture.stop_desktop()
                self.assertEqual(self.finish(process)[0], 0)

    def test_desktop_that_never_gets_ready_keeps_running_without_arinn(self):
        fixture = Fixture(self)
        process = self.start(fixture, "plasma", FAKE_READY_AFTER=10**6)
        self.assertTrue(fixture.wait_for(
            lambda c: len([x for x in c if "graphical-session.target" in x]) >= 240), fixture.calls())
        time.sleep(0.2)
        self.assertIsNone(process.poll(), "the desktop must keep running")
        fixture.stop_desktop()
        code, err = self.finish(process)
        self.assertEqual(code, 0)
        self.assertIn("Arinn did not start", err)
        calls = fixture.calls()
        self.assertFalse(any(c.startswith("systemctl --user set-environment") or "start arinn" in c
                             for c in calls), calls)
        self.assertEqual(len([c for c in calls if "is-active" in c]), 240)

    def test_failed_target_start_is_reported_and_the_desktop_continues(self):
        fixture = Fixture(self)
        process = self.start(fixture, "plasma", FAKE_START_RC=1)
        self.assertTrue(fixture.wait_for(lambda c: "systemctl --user start arinn-session.target" in c))
        time.sleep(0.1)
        self.assertIsNone(process.poll())
        fixture.stop_desktop()
        code, err = self.finish(process)
        self.assertEqual(code, 0)
        self.assertIn("Arinn did not start", err)

    def test_desktop_exiting_during_startup_stops_waiting(self):
        fixture = Fixture(self)
        fixture.stop_desktop()
        code, err = self.finish(self.start(fixture, "plasma", FAKE_READY_AFTER=10**6,
                                           FAKE_DESKTOP_STATUS=5))
        self.assertEqual(code, 5, err)
        self.assertNotIn("Arinn did not start", err)
        self.assertFalse(any("start arinn" in c for c in fixture.calls()))

    def test_termination_is_forwarded_to_the_desktop(self):
        fixture = Fixture(self)
        process = self.start(fixture, "plasma")
        self.assertTrue(fixture.wait_for(lambda c: "systemctl --user start arinn-session.target" in c))
        process.send_signal(signal.SIGTERM)
        code, err = self.finish(process)
        self.assertEqual(code, 0, err)
        calls = fixture.calls()
        self.assertIn("desktop terminated", calls)
        self.assertEqual(calls[-1], "systemctl --user unset-environment ARINN_SESSION_ENTRY")

    def test_user_bus_is_shared_with_the_desktop(self):
        fixture = Fixture(self)
        runtime = fixture.root / "run"
        runtime.mkdir()
        bus = socket.socket(socket.AF_UNIX)
        self.addCleanup(bus.close)
        bus.bind(str(runtime / "bus"))
        fixture.stop_desktop()
        self.finish(self.start(fixture, "xfce", XDG_RUNTIME_DIR=runtime, FAKE_READY_AFTER=10**6))
        self.assertEqual(fixture.desktop(), [f"desktop startxfce4 bus=unix:path={runtime}/bus"])
        # An address from pam_systemd or the display manager is kept.
        fixture = Fixture(self)
        fixture.stop_desktop()
        self.finish(self.start(fixture, "xfce", XDG_RUNTIME_DIR=runtime, FAKE_READY_AFTER=10**6,
                               DBUS_SESSION_BUS_ADDRESS="unix:path=/synthetic"))
        self.assertEqual(fixture.desktop(), ["desktop startxfce4 bus=unix:path=/synthetic"])

    def test_unknown_entry_is_refused(self):
        fixture = Fixture(self)
        for args in ([], ["gnome"], ["Plasma"]):
            result = subprocess.run([str(fixture.session), *args], env=fixture.env(),
                                    capture_output=True, text=True, timeout=10)
            self.assertEqual(result.returncode, 2, args)
        self.assertEqual(fixture.calls(), [])


class SessionSwitch(unittest.TestCase):
    def switch(self, fixture, *args, **env):
        return subprocess.run([str(fixture.switch), *args], env=fixture.env(**env),
                              capture_output=True, text=True, timeout=10)

    def test_switch_to_desktop_from_the_plasma_entry(self):
        fixture = Fixture(self)
        result = self.switch(fixture, "desktop", ARINN_SESSION_ENTRY="plasma")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(fixture.mode_file.read_text(), "desktop\n")
        self.assertEqual(fixture.calls(),
                         ["busctl --user call org.kde.Shutdown /Shutdown org.kde.Shutdown logout"])
        self.assertEqual(stat.S_IMODE(fixture.mode_file.stat().st_mode), 0o600)
        self.assertEqual(stat.S_IMODE(fixture.mode_file.parent.stat().st_mode), 0o700)
        self.assertEqual(sorted(p.name for p in fixture.mode_file.parent.iterdir()), ["session-mode"])

    def test_return_to_arinn_uses_the_desktop_logout_prompt(self):
        cases = (
            ("KDE", "busctl --user call org.kde.LogoutPrompt /LogoutPrompt "
                    "org.kde.LogoutPrompt promptLogout"),
            ("XFCE", "busctl --user call org.xfce.SessionManager /org/xfce/SessionManager "
                     "org.xfce.Session.Manager Logout bb true true"),
        )
        for current, call in cases:
            with self.subTest(desktop=current):
                fixture = Fixture(self)
                fixture.mode_file.parent.mkdir(parents=True)
                fixture.mode_file.write_text("desktop\n")
                result = self.switch(fixture, "arinn", "--prompt", XDG_CURRENT_DESKTOP=current)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(fixture.mode_file.read_text(), "arinn\n")
                self.assertEqual(fixture.calls(), [call])

    def test_xfce_logout_without_dialog(self):
        fixture = Fixture(self)
        result = self.switch(fixture, "desktop", ARINN_SESSION_ENTRY="xfce")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(fixture.calls(), [
            "busctl --user call org.xfce.SessionManager /org/xfce/SessionManager "
            "org.xfce.Session.Manager Logout bb false true"])

    def test_session_entry_wins_over_the_desktop_name(self):
        fixture = Fixture(self)
        result = self.switch(fixture, "desktop", ARINN_SESSION_ENTRY="xfce",
                             XDG_CURRENT_DESKTOP="KDE")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("org.xfce.SessionManager", fixture.calls()[0])
        fixture = Fixture(self)
        result = self.switch(fixture, "desktop", XDG_CURRENT_DESKTOP="ubuntu:XFCE")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("org.xfce.SessionManager", fixture.calls()[0])

    def test_refused_logout_restores_the_previous_choice(self):
        fixture = Fixture(self)
        fixture.mode_file.parent.mkdir(parents=True)
        fixture.mode_file.write_text("arinn\n")
        result = self.switch(fixture, "desktop", ARINN_SESSION_ENTRY="plasma", FAKE_CALL_RC=1)
        self.assertEqual(result.returncode, 1)
        self.assertIn("did not accept", result.stderr)
        self.assertEqual(fixture.mode_file.read_text(), "arinn\n")
        # Without an earlier choice the file is removed again.
        fixture = Fixture(self)
        result = self.switch(fixture, "desktop", ARINN_SESSION_ENTRY="plasma", FAKE_CALL_RC=1)
        self.assertEqual(result.returncode, 1)
        self.assertFalse(fixture.mode_file.exists())
        self.assertEqual(list(fixture.mode_file.parent.iterdir()), [])

    def test_unsupported_desktop_changes_nothing(self):
        for env in ({}, {"XDG_CURRENT_DESKTOP": "GNOME"}, {"ARINN_SESSION_ENTRY": "gnome"}):
            with self.subTest(env=env):
                fixture = Fixture(self)
                result = self.switch(fixture, "desktop", **env)
                self.assertEqual(result.returncode, 1)
                self.assertFalse(fixture.mode_file.parent.exists())
                self.assertEqual(fixture.calls(), [])

    def test_bad_arguments_are_refused(self):
        fixture = Fixture(self)
        for args in ([], ["plasma"], ["desktop", "--now"], ["arinn", "--prompt", "x"]):
            result = self.switch(fixture, *args, ARINN_SESSION_ENTRY="plasma")
            self.assertEqual(result.returncode, 2, args)
        self.assertFalse(fixture.mode_file.parent.exists())
        self.assertEqual(fixture.calls(), [])

    def test_round_trip_through_the_session_entry(self):
        fixture = Fixture(self)
        self.assertEqual(self.switch(fixture, "desktop", ARINN_SESSION_ENTRY="plasma").returncode, 0)
        fixture.stop_desktop()
        subprocess.run([str(fixture.session), "plasma"], env=fixture.env(), timeout=20, check=True)
        self.assertFalse(any("start arinn" in c for c in fixture.calls()), fixture.calls())
        fixture.log.write_text("")
        (fixture.root / "stop").unlink()
        self.assertEqual(self.switch(fixture, "arinn", "--prompt",
                                     XDG_CURRENT_DESKTOP="KDE").returncode, 0)
        process = subprocess.Popen([str(fixture.session), "plasma"], env=fixture.env())
        self.addCleanup(lambda: process.poll() is None and process.kill())
        self.assertTrue(fixture.wait_for(lambda c: "systemctl --user start arinn-session.target" in c),
                        fixture.calls())
        fixture.stop_desktop()
        self.assertEqual(process.wait(timeout=20), 0)


if __name__ == "__main__":
    unittest.main()
