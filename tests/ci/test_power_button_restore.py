# SPDX-FileCopyrightText: 2026 Arinn contributors
#
# SPDX-License-Identifier: GPL-3.0-or-later

"""The XFCE power button restore entry (ADR-0111): enabled for every XFCE
login, it only runs the adapter's restore and starts no part of Arinn."""

import configparser
import pathlib
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[2]
ENTRY = "org.arinn.PowerButtonRestore.desktop"


class PowerButtonRestoreTest(unittest.TestCase):
    def entry(self):
        parser = configparser.ConfigParser(interpolation=None)
        parser.optionxform = str
        parser.read(ROOT / "packaging/autostart" / ENTRY)
        return parser["Desktop Entry"]

    def test_only_xfce_runs_the_restore(self):
        entry = self.entry()
        self.assertEqual(entry["OnlyShowIn"], "XFCE;")
        self.assertEqual(entry["Exec"], "/usr/bin/arinn-platform-x11 --restore-power-button")
        self.assertEqual(entry["NoDisplay"], "true")
        # It never starts the session target, the shell or the daemon.
        self.assertNotIn("arinn-xfce-session", entry["Exec"])

    def test_the_adapter_knows_the_option(self):
        main = (ROOT / "platform/x11/src/main.rs").read_text()
        self.assertIn('"--restore-power-button"', main)

    def test_adapter_process_survives_its_session_bus_name(self):
        # The binary releases org.arinn.X11 at each native session's end.
        # Type=dbus would make systemd stop it before it could wait for the
        # daemon and reconnect. Keep readiness separate from process lifetime.
        parser = configparser.ConfigParser(interpolation=None)
        parser.optionxform = str
        parser.read(ROOT / "packaging/systemd/arinn-ewmh.service")
        service = parser["Service"]
        self.assertEqual(service["Type"], "exec")
        self.assertNotIn("BusName", service)
        self.assertEqual(service["Restart"], "on-failure")
        self.assertIn("--release-power-key", service["ExecStopPost"])

    def test_both_packages_enable_it_system_wide(self):
        for path in ("packaging/arch/PKGBUILD", "packaging/debian/debian/rules"):
            text = (ROOT / path).read_text()
            self.assertIn("etc/xdg/autostart/" + ENTRY, text, path)


if __name__ == "__main__":
    unittest.main()
