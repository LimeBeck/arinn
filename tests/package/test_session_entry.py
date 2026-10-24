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

UNSET = "systemctl --user unset-environment ARINN_SESSION_ENTRY ARINN_SESSION_MODE"
# SIGINT (2) and SIGQUIT (3) in a /proc SigIgn mask.
INT_QUIT = (1 << 1) | (1 << 2)

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
*" call "*)
    # A desktop that never answers: only a bounded call returns, failed, as
    # busctl does when its --timeout expires.
    if [ -n "${FAKE_CALL_HANG:-}" ]; then
        case "$*" in *--timeout=*) exit 1 ;; esac
        exec sleep 60
    fi
    exit "${FAKE_CALL_RC:-0}" ;;
esac
exit 9
"""

# The desktop runs until the test creates `stop`, or the wrapper forwards a
# termination signal; it reports the bus address it was started with.
DESKTOP = """#!/bin/sh
# The mask of a program the desktop starts (sed's own), read before the trap
# below changes anything. Not the shell's own mask: bash ignores SIGQUIT for
# itself and restores the inherited disposition for its children.
sigign=$(sed -n 's/^SigIgn:[[:space:]]*//p' /proc/self/status)
trap 'echo "desktop terminated" >> "$FAKE_LOG"; exit 0' TERM
echo "desktop $(basename "$0") bus=${DBUS_SESSION_BUS_ADDRESS:-}" >> "$FAKE_LOG"
echo "sigign $sigign" >> "$FAKE_LOG"
echo "markers mode=${ARINN_SESSION_MODE:-} entry=${ARINN_SESSION_ENTRY:-}" >> "$FAKE_LOG"
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
        return [c for c in self.log.read_text().splitlines()
                if not c.startswith(("sigign ", "markers "))]

    def markers(self):
        """The ARINN_SESSION_* variables the desktop process got."""
        return [c for c in self.log.read_text().splitlines() if c.startswith("markers ")]

    def ignored_signals(self):
        """The desktop's ignored-signal mask (/proc SigIgn)."""
        return [int(c.split()[1], 16) for c in self.log.read_text().splitlines()
                if c.startswith("sigign ")]

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
        process = subprocess.Popen([str(fixture.session), entry], cwd=fixture.root,
                                   env=fixture.env(**env),
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
        marker = calls.index("systemctl --user set-environment ARINN_SESSION_ENTRY=plasma "
                             f"ARINN_SESSION_MODE={fixture.mode_file}")
        self.assertLess(marker, calls.index("systemctl --user start arinn-session.target"))
        self.assertEqual(calls[-2:], ["systemctl --user stop arinn-session.target", UNSET])
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
        self.assertIn("systemctl --user set-environment ARINN_SESSION_ENTRY=xfce "
                      f"ARINN_SESSION_MODE={fixture.mode_file}", calls)
        # The helper owns the EWMH target; the entry starts no KDE target.
        self.assertFalse(any("arinn-session.target" in c for c in calls), calls)
        self.assertEqual(calls[-1], UNSET)

    def test_desktop_mode_starts_the_plain_desktop(self):
        for entry, desktop in (("plasma", "startplasma-wayland"), ("xfce", "startxfce4")):
            with self.subTest(entry=entry):
                fixture = Fixture(self)
                fixture.mode_file.parent.mkdir(parents=True)
                fixture.mode_file.write_text("desktop\n")
                fixture.stop_desktop()
                code, err = self.finish(self.start(fixture, entry, FAKE_DESKTOP_STATUS=4,
                                                   ARINN_SESSION_ENTRY="plasma"))
                self.assertEqual(code, 4, err)
                # A stale entry marker is cleared and the mode file named in
                # both environments, for Return to Arinn; nothing starts.
                self.assertEqual(fixture.calls(), [
                    "systemctl --user unset-environment ARINN_SESSION_ENTRY",
                    f"systemctl --user set-environment ARINN_SESSION_MODE={fixture.mode_file}",
                    f"desktop {desktop} bus="])
                self.assertEqual(fixture.markers(), [f"markers mode={fixture.mode_file} entry="])

    def test_exactly_desktop_without_a_newline_is_desktop(self):
        fixture = Fixture(self)
        fixture.mode_file.parent.mkdir(parents=True)
        fixture.mode_file.write_text("desktop")
        fixture.stop_desktop()
        self.finish(self.start(fixture, "plasma"))
        self.assertFalse(any("start arinn" in c for c in fixture.calls()), fixture.calls())

    def test_state_home_is_honoured(self):
        fixture = Fixture(self)
        state = fixture.root / "state"
        (state / "arinn").mkdir(parents=True)
        (state / "arinn/session-mode").write_text("desktop\n")
        fixture.stop_desktop()
        self.finish(self.start(fixture, "plasma", XDG_STATE_HOME=state))
        self.assertEqual(fixture.desktop(), ["desktop startplasma-wayland bus="])
        self.assertFalse(any("start arinn" in c for c in fixture.calls()), fixture.calls())

    def test_anything_but_desktop_starts_arinn(self):
        for content in ("", "arinn\n", "Desktop\n", "desktop extra\n", " desktop\n", "\n",
                        "desktop\nextra\n", "desktop\n\narinn\n", "desktop" + " " * 20,
                        "desktop\n\n", "desktop\n\n\n", "desktop\r\n", "desktop" + "\n" * 20):
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
        # The stub desktop logs its start line after installing its trap.
        self.assertTrue(fixture.wait_for(lambda c: "systemctl --user start arinn-session.target" in c
                                         and fixture.desktop()))
        process.send_signal(signal.SIGTERM)
        code, err = self.finish(process)
        self.assertEqual(code, 0, err)
        calls = fixture.calls()
        self.assertIn("desktop terminated", calls)
        self.assertEqual(calls[-1], UNSET)

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

    def test_desktop_gets_default_interrupt_and_quit_in_both_modes(self):
        # Even when the display manager ignores them, the desktop starts with
        # SIGINT and SIGQUIT at their defaults, like the stock entries.
        def ignore():
            signal.signal(signal.SIGINT, signal.SIG_IGN)
            signal.signal(signal.SIGQUIT, signal.SIG_IGN)
        for mode in ("arinn", "desktop"):
            with self.subTest(mode=mode):
                fixture = Fixture(self)
                fixture.mode_file.parent.mkdir(parents=True)
                fixture.mode_file.write_text(mode + "\n")
                process = subprocess.Popen([str(fixture.session), "xfce"], cwd=fixture.root, env=fixture.env(),
                                           preexec_fn=ignore, stderr=subprocess.PIPE, text=True)
                self.addCleanup(lambda p=process: p.poll() is None and p.kill())
                self.assertTrue(fixture.wait_for(lambda c: any(c) and fixture.ignored_signals()))
                fixture.stop_desktop()
                process.communicate(timeout=20)
                [mask] = fixture.ignored_signals()
                self.assertEqual(mask & INT_QUIT, 0, hex(mask))

    def test_unknown_entry_is_refused(self):
        fixture = Fixture(self)
        for args in ([], ["gnome"], ["Plasma"]):
            result = subprocess.run([str(fixture.session), *args], cwd=fixture.root, env=fixture.env(),
                                    capture_output=True, text=True, timeout=10)
            self.assertEqual(result.returncode, 2, args)
        self.assertEqual(fixture.calls(), [])


class SessionSwitch(unittest.TestCase):
    def switch(self, fixture, *args, **env):
        return subprocess.run([str(fixture.switch), *args], cwd=fixture.root, env=fixture.env(**env),
                              capture_output=True, text=True, timeout=10)

    def test_switch_to_desktop_from_the_plasma_entry(self):
        fixture = Fixture(self)
        result = self.switch(fixture, "desktop", ARINN_SESSION_ENTRY="plasma")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(fixture.mode_file.read_text(), "desktop\n")
        self.assertEqual(fixture.calls(),
                         ["busctl --user --timeout=5 call org.kde.Shutdown /Shutdown "
                          "org.kde.Shutdown logout"])
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
            "busctl --user --timeout=5 call org.xfce.SessionManager /org/xfce/SessionManager "
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

    def test_relative_paths_never_leave_the_temporary_directory(self):
        # Every script runs with the fixture as its working directory, so a
        # relative state path (or a regression that honours one) stays there.
        fixture = Fixture(self)
        self.switch(fixture, "desktop", ARINN_SESSION_ENTRY="plasma", XDG_STATE_HOME="relative",
                    ARINN_SESSION_MODE="relative/arinn/session-mode")
        self.assertFalse((Path.cwd() / "relative").exists())

    def test_switch_writes_the_file_the_session_read(self):
        # The daemon's environment comes from the user manager, which may lack
        # the XDG_STATE_HOME of the login; the session exports its file.
        fixture = Fixture(self)
        state = fixture.root / "login-state" / "arinn"
        state.mkdir(parents=True)
        result = self.switch(fixture, "desktop", ARINN_SESSION_ENTRY="plasma",
                             ARINN_SESSION_MODE=state / "session-mode")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual((state / "session-mode").read_text(), "desktop\n")
        self.assertFalse(fixture.mode_file.exists())
        # Anything that is not an absolute .../arinn/session-mode is ignored.
        for value in ("relative/arinn/session-mode", str(fixture.root / "elsewhere"),
                      str(fixture.root / "arinn/session-mode.bak")):
            with self.subTest(value=value):
                fixture = Fixture(self)
                result = self.switch(fixture, "desktop", ARINN_SESSION_ENTRY="plasma",
                                     ARINN_SESSION_MODE=value)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(fixture.mode_file.read_text(), "desktop\n")

    def test_choice_is_replaced_atomically(self):
        fixture = Fixture(self)
        fixture.mode_file.parent.mkdir(parents=True, mode=0o755)
        fixture.mode_file.parent.chmod(0o755)
        fixture.mode_file.write_text("arinn\n")
        before = fixture.mode_file.stat().st_ino
        result = self.switch(fixture, "desktop", ARINN_SESSION_ENTRY="plasma")
        self.assertEqual(result.returncode, 0, result.stderr)
        # A new file renamed over the old one, never rewritten in place.
        self.assertNotEqual(fixture.mode_file.stat().st_ino, before)
        self.assertEqual(stat.S_IMODE(fixture.mode_file.stat().st_mode), 0o600)
        # An existing directory keeps its mode (it is also the daemon's).
        self.assertEqual(stat.S_IMODE(fixture.mode_file.parent.stat().st_mode), 0o755)
        self.assertEqual(sorted(p.name for p in fixture.mode_file.parent.iterdir()),
                         ["session-mode"])

    def test_failed_write_leaves_the_old_choice_and_no_temporary_file(self):
        fixture = Fixture(self)
        fixture.mode_file.parent.mkdir(parents=True)
        fixture.mode_file.write_text("arinn\n")
        fixture.executable(fixture.bin / "mv", "#!/bin/sh\nexit 1\n")
        result = self.switch(fixture, "desktop", ARINN_SESSION_ENTRY="plasma")
        # Distinct from a refused logout: the daemon reports Internal (#501).
        self.assertEqual(result.returncode, 3)
        self.assertEqual(fixture.mode_file.read_text(), "arinn\n")
        self.assertEqual(sorted(p.name for p in fixture.mode_file.parent.iterdir()),
                         ["session-mode"])
        self.assertEqual(fixture.calls(), [], "no logout without a recorded choice")

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

    def test_unwritable_state_directory_is_a_distinct_failure(self):
        fixture = Fixture(self)
        fixture.mode_file.parent.parent.mkdir(parents=True)
        fixture.mode_file.parent.write_text("not a directory")
        result = self.switch(fixture, "desktop", ARINN_SESSION_ENTRY="plasma")
        self.assertEqual(result.returncode, 3)
        self.assertEqual(fixture.calls(), [])

    def test_a_logout_call_that_does_not_answer_restores_the_previous_choice(self):
        # The daemon's switch: busctl gives up after its bound and the helper
        # restores the choice before the daemon's own limit (#501).
        fixture = Fixture(self)
        fixture.mode_file.parent.mkdir(parents=True)
        fixture.mode_file.write_text("arinn\n")
        for entry in ("plasma", "xfce"):
            with self.subTest(entry=entry):
                result = self.switch(fixture, "desktop", ARINN_SESSION_ENTRY=entry,
                                     FAKE_CALL_HANG="1")
                self.assertEqual(result.returncode, 1)
                self.assertIn("did not accept", result.stderr)
                self.assertEqual(fixture.mode_file.read_text(), "arinn\n")

    def test_refused_logout_restores_what_the_session_would_start(self):
        # The previous value is judged by the session's exact rule, so damaged
        # content that meant Arinn is never restored as the desktop.
        for before, after in (("desktop\n", "desktop\n"), ("desktop", "desktop\n"),
                              ("desktop\nextra\n", "arinn\n"), ("desktop\n\n\n", "arinn\n"),
                              ("garbage", "arinn\n")):
            with self.subTest(before=before):
                fixture = Fixture(self)
                fixture.mode_file.parent.mkdir(parents=True)
                fixture.mode_file.write_text(before)
                result = self.switch(fixture, "arinn" if before == "desktop\n" else "desktop",
                                     ARINN_SESSION_ENTRY="plasma", FAKE_CALL_RC=1)
                self.assertEqual(result.returncode, 1)
                self.assertEqual(fixture.mode_file.read_text(), after)

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
        subprocess.run([str(fixture.session), "plasma"], cwd=fixture.root, env=fixture.env(), timeout=20, check=True)
        self.assertFalse(any("start arinn" in c for c in fixture.calls()), fixture.calls())
        fixture.log.write_text("")
        (fixture.root / "stop").unlink()
        self.assertEqual(self.switch(fixture, "arinn", "--prompt",
                                     XDG_CURRENT_DESKTOP="KDE").returncode, 0)
        process = subprocess.Popen([str(fixture.session), "plasma"], cwd=fixture.root, env=fixture.env())
        self.addCleanup(lambda: process.poll() is None and process.kill())
        self.assertTrue(fixture.wait_for(lambda c: "systemctl --user start arinn-session.target" in c),
                        fixture.calls())
        fixture.stop_desktop()
        self.assertEqual(process.wait(timeout=20), 0)


if __name__ == "__main__":
    unittest.main()
