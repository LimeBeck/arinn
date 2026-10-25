#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Arinn contributors
# SPDX-License-Identifier: GPL-3.0-or-later
"""XFCE lifetime on a private bus, with a stub systemctl; no desktop access.

ARINN_XFCE_SESSION selects the built or installed helper.
"""
import ctypes
import ctypes.util
import os
from pathlib import Path
import subprocess
import tempfile
import time
import unittest

ROOT = Path(__file__).resolve().parents[2]
HELPER = Path(os.environ.get("ARINN_XFCE_SESSION", ROOT / "target/debug/arinn-xfce-session"))


class XfceSession(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory(prefix="arinn-xfce-")
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        self.log = self.root / "calls"
        self.log.touch()
        self.bus = subprocess.Popen(
            ["dbus-daemon", "--session", "--nofork", "--print-address=1"],
            stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True)
        self.addCleanup(self.stop_process, self.bus)
        address = self.bus.stdout.readline().strip()
        self.assertTrue(address)
        self.dbus = ctypes.CDLL(ctypes.util.find_library("dbus-1"))
        self.dbus.dbus_connection_open_private.argtypes = [ctypes.c_char_p, ctypes.c_void_p]
        self.dbus.dbus_connection_open_private.restype = ctypes.c_void_p
        self.dbus.dbus_bus_register.argtypes = [ctypes.c_void_p, ctypes.c_void_p]
        self.dbus.dbus_bus_request_name.argtypes = [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_uint, ctypes.c_void_p]
        self.dbus.dbus_connection_close.argtypes = [ctypes.c_void_p]
        self.dbus.dbus_connection_unref.argtypes = [ctypes.c_void_p]
        self.owner = self.dbus.dbus_connection_open_private(address.encode(), None)
        self.assertTrue(self.owner)
        self.assertTrue(self.dbus.dbus_bus_register(self.owner, None))
        self.assertEqual(self.dbus.dbus_bus_request_name(self.owner, b"org.xfce.SessionManager", 4, None), 1)
        self.addCleanup(self.close_owner)
        stub = self.root / "systemctl"
        stub.write_text("""#!/bin/sh
echo "$*" >> "$CALL_LOG"
case "$*" in
*is-active*) [ "${ALREADY_ACTIVE:-0}" = 1 ]; exit $? ;;
*"start arinn-ewmh-session.target"*) exit "${START_FAIL:-0}" ;;
esac
exit 0
""")
        stub.chmod(0o755)
        self.env = {**os.environ, "DBUS_SESSION_BUS_ADDRESS": address,
                    "DISPLAY": ":987", "XDG_SESSION_TYPE": "x11",
                    "XAUTHORITY": str(self.root / "synthetic-authority"),
                    "PATH": f"{self.root}:{os.environ['PATH']}", "CALL_LOG": str(self.log)}
        self.env.pop("ARINN_SESSION_ENTRY", None)

    @staticmethod
    def stop_process(child):
        if child.poll() is None:
            child.terminate()
        child.wait(timeout=5)
        if child.stdout:
            child.stdout.close()
        if child.stderr:
            child.stderr.close()

    def close_owner(self):
        if self.owner:
            self.dbus.dbus_connection_close(self.owner)
            self.dbus.dbus_connection_unref(self.owner)
            self.owner = None

    def launch(self, **changes):
        env = {**self.env, **changes}
        env = {key: value for key, value in env.items() if value is not None}
        child = subprocess.Popen([str(HELPER)], env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        self.addCleanup(self.stop_process, child)
        return child

    def wait_for_start(self, child):
        deadline = time.monotonic() + 5
        while time.monotonic() < deadline:
            if "--user --no-ask-password start arinn-ewmh-session.target" in self.log.read_text():
                return
            self.assertIsNone(child.poll(), "helper exited before starting")
            time.sleep(0.02)
        self.fail("startup timed out")

    def assert_cleanup(self):
        calls = self.log.read_text().splitlines()
        self.assertEqual(sum(" stop arinn-ewmh-session.target" in call for call in calls), 1)
        self.assertTrue(calls[-1].endswith("unset-environment DISPLAY XAUTHORITY XDG_SESSION_TYPE ARINN_PLATFORM_BACKEND ARINN_SESSION_BACKEND"))
        self.assertFalse(any(" enable " in call for call in calls))

    def test_login_imports_environment_before_start_and_logout_stops(self):
        child = self.launch(XDG_SESSION_TYPE=None)
        self.wait_for_start(child)
        calls = self.log.read_text().splitlines()
        self.assertIn("--user --no-ask-password import-environment DISPLAY", calls)
        self.assertIn("--user --no-ask-password import-environment XAUTHORITY", calls)
        selection = next(i for i, call in enumerate(calls) if "set-environment XDG_SESSION_TYPE=x11 ARINN_PLATFORM_BACKEND=ewmh ARINN_SESSION_BACKEND=x11" in call)
        self.assertLess(selection, len(calls) - 1)
        # ADR-0111: the ordinary XFCE desktop keeps its power dialog. The
        # adapter recognises it by org.xfce.SessionManager on the session
        # bus, which this bridge requires before it starts the target (see
        # test_missing_xfce_owner_never_touches_manager), not by
        # XDG_CURRENT_DESKTOP. A stock login lacks the dedicated session's
        # marker, so the bridge clears one a killed wrapper left in a
        # lingering user manager before the target starts: the adapter
        # never takes the key here.
        start = calls.index("--user --no-ask-password start arinn-ewmh-session.target")
        clear = calls.index("--user --no-ask-password unset-environment ARINN_SESSION_ENTRY")
        self.assertLess(clear, start)
        self.assertFalse(any(" set-environment ARINN_SESSION_ENTRY" in call for call in calls))
        self.assertFalse(any("XDG_CURRENT_DESKTOP" in call for call in calls))
        self.close_owner()
        self.assertEqual(child.wait(timeout=5), 0)
        self.assert_cleanup()

    def test_dedicated_login_keeps_its_marker(self):
        # ADR-0111: the dedicated session's wrapper puts the marker into the
        # desktop's environment; the bridge binds the user manager's marker
        # to this login before the target starts, so the adapter takes the
        # power key there.
        child = self.launch(ARINN_SESSION_ENTRY="xfce")
        self.wait_for_start(child)
        calls = self.log.read_text().splitlines()
        start = calls.index("--user --no-ask-password start arinn-ewmh-session.target")
        mark = calls.index("--user --no-ask-password set-environment ARINN_SESSION_ENTRY=xfce")
        self.assertLess(mark, start)
        self.assertFalse(any("unset-environment ARINN_SESSION_ENTRY" in call for call in calls))
        self.close_owner()
        self.assertEqual(child.wait(timeout=5), 0)

    def test_signal_stops_and_missing_authority_clears_stale_value(self):
        child = self.launch(XAUTHORITY=None)
        self.wait_for_start(child)
        self.assertIn("--user --no-ask-password unset-environment XAUTHORITY", self.log.read_text())
        child.terminate()
        self.assertEqual(child.wait(timeout=5), 0)
        self.assert_cleanup()

    def test_duplicate_cannot_stop_first_launcher(self):
        first = self.launch()
        self.wait_for_start(first)
        before = self.log.read_text()
        duplicate = self.launch()
        self.assertEqual(duplicate.wait(timeout=5), 1)
        self.assertEqual(self.log.read_text(), before)
        self.assertIsNone(first.poll())
        self.close_owner()
        self.assertEqual(first.wait(timeout=5), 0)
        self.assert_cleanup()

    def test_failed_start_cleans_partial_session(self):
        child = self.launch(START_FAIL="1")
        self.assertEqual(child.wait(timeout=5), 1)
        self.assert_cleanup()

    def test_refuses_existing_session_without_stopping_it(self):
        child = self.launch(ALREADY_ACTIVE="1")
        self.assertEqual(child.wait(timeout=5), 1)
        self.assertNotIn(" stop ", self.log.read_text())
        self.assertNotIn("set-environment", self.log.read_text())

    def test_wayland_or_missing_display_never_touches_manager(self):
        for changes in ({"XDG_SESSION_TYPE": "wayland"}, {"DISPLAY": None}):
            child = self.launch(**changes)
            self.assertEqual(child.wait(timeout=5), 1)
            self.assertEqual(self.log.read_text(), "")

    def test_missing_xfce_owner_never_touches_manager(self):
        # Also ADR-0111: the target never runs without the XFCE session
        # manager's name, by which the adapter leaves the power key to XFCE.
        self.close_owner()
        child = self.launch()
        self.assertEqual(child.wait(timeout=5), 1)
        self.assertEqual(self.log.read_text(), "")

    def test_bus_disconnect_cleans_up(self):
        child = self.launch()
        self.wait_for_start(child)
        self.bus.terminate()
        self.bus.wait(timeout=5)
        # Graceful bus shutdown can report owner loss before EOF. Both paths
        # must stop the target; exit status distinguishes normal owner loss.
        self.assertIn(child.wait(timeout=5), (0, 1))
        self.assert_cleanup()


if __name__ == "__main__":
    unittest.main()
