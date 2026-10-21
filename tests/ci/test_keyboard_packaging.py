# SPDX-FileCopyrightText: 2026 Arinn contributors
#
# SPDX-License-Identifier: GPL-3.0-or-later

"""Packaged files of the global on-screen keyboard (ADR-0105)."""

import configparser
import pathlib
import re
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[2]


class KeyboardPackagingTest(unittest.TestCase):
    def test_hwdb_remaps_only_the_remote_microphone(self):
        lines = [line.rstrip("\n") for line in
                 (ROOT / "packaging/udev/70-arinn-remote.hwdb").read_text().splitlines()
                 if line.strip() and not line.startswith("#")]
        # One match line, then one property line indented by one space.
        self.assertEqual(lines, ["evdev:input:b0003v1915p1001*", " KEYBOARD_KEY_c00cf=prog1"])
        self.assertRegex(lines[0], r"^evdev:input:b[0-9A-F]{4}v[0-9A-F]{4}p[0-9A-F]{4}\*$")

    def test_input_method_desktop_file_is_a_kwin_virtual_keyboard(self):
        parser = configparser.ConfigParser(interpolation=None)
        parser.optionxform = str
        parser.read(ROOT / "packaging/desktop/org.arinn.InputMethod.desktop")
        entry = parser["Desktop Entry"]
        self.assertEqual(entry["X-KDE-Wayland-VirtualKeyboard"], "true")
        self.assertEqual(entry["NoDisplay"], "true")
        self.assertEqual(entry["Exec"], "/usr/lib/arinn/arinn-input-method")

    def test_the_package_installs_them(self):
        pkgbuild = (ROOT / "packaging/arch/PKGBUILD").read_text()
        for installed in (
            "/usr/lib/arinn/arinn-input-method",
            "/usr/share/applications/org.arinn.InputMethod.desktop",
            "/usr/lib/udev/hwdb.d/70-arinn-remote.hwdb",
        ):
            self.assertIn(installed, pkgbuild)
        # The daemon looks the desktop file up by this name.
        config = (ROOT / "platform/kde/keyboard_config.rs").read_text()
        self.assertTrue(re.search(r'INPUT_METHOD_DESKTOP: &str = "org\.arinn\.InputMethod\.desktop"', config))


if __name__ == "__main__":
    unittest.main()
