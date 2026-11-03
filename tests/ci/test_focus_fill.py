# SPDX-FileCopyrightText: 2026 Arinn contributors
#
# SPDX-License-Identifier: GPL-3.0-or-later

"""The shell focus-fill guard (#722)."""

import importlib.util
import pathlib
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "tests" / "ui" / "check_focus_fill.py"
spec = importlib.util.spec_from_file_location("check_focus_fill", SCRIPT)
check_focus_fill = importlib.util.module_from_spec(spec)
spec.loader.exec_module(check_focus_fill)


class FocusFillGuardTest(unittest.TestCase):
    def test_focus_selected_white_plate_fails(self):
        offenders = check_focus_fill.offenders_in(
            "color: activeFocus ? Qt.rgba(1, 1, 1, 0.20) : Ui.Theme.plate\n"
        )
        self.assertEqual(len(offenders), 1)
        offenders = check_focus_fill.offenders_in(
            "color: item.activeFocus\n"
            "       ? Qt.rgba(1, 1, 1, 0.2)\n"
            "       : Ui.Theme.plate\n"
        )
        self.assertEqual(len(offenders), 1)

    def test_switch_track_is_not_a_focus_plate(self):
        offenders = check_focus_fill.offenders_in(
            "color: row.currentValue === true ? Ui.Theme.ember : Qt.rgba(1, 1, 1, 0.20)\n"
        )
        self.assertEqual(offenders, [])

    def test_other_alphas_and_comments_pass(self):
        self.assertEqual(check_focus_fill.offenders_in(
            "color: activeFocus ? Qt.rgba(1, 1, 1, 0.25) : Ui.Theme.plate\n"
            "color: activeFocus ? Qt.rgba(1, 1, 1, 0.24) : Ui.Theme.plate\n"
            "color: activeFocus ? Qt.rgba(1, 1, 1, 0.12) : Ui.Theme.plate\n"
            "// color: activeFocus ? Qt.rgba(1, 1, 1, 0.20) : Ui.Theme.plate\n"
        ), [])

    def test_hex_argb_focus_plate_fails(self):
        offenders = check_focus_fill.offenders_in(
            'color: activeFocus ? "#33ffffff" : Ui.Theme.plate\n'
        )
        self.assertEqual(len(offenders), 1)

    def test_hex_rgba_focus_plate_fails(self):
        offenders = check_focus_fill.offenders_in(
            'color: activeFocus ? "#ffffff33" : Ui.Theme.plate\n'
        )
        self.assertEqual(len(offenders), 1)

    def test_opacity_focus_plate_fails(self):
        offenders = check_focus_fill.offenders_in(
            "opacity: activeFocus ? 0.2 : 0\n"
        )
        self.assertEqual(len(offenders), 1)
        offenders = check_focus_fill.offenders_in(
            "opacity: activeFocus ? 0.20 : 1\n"
        )
        self.assertEqual(len(offenders), 1)

    def test_current_item_selects_a_focus_plate(self):
        offenders = check_focus_fill.offenders_in(
            "color: isCurrentItem ? Qt.rgba(1, 1, 1, 0.20) : Ui.Theme.plate\n"
        )
        self.assertEqual(len(offenders), 1)
        offenders = check_focus_fill.offenders_in(
            'color: isCurrentItem ? "#33ffffff" : Ui.Theme.plate\n'
        )
        self.assertEqual(len(offenders), 1)

    def test_scrim_and_full_opacity_are_not_focus_plates(self):
        self.assertEqual(check_focus_fill.offenders_in(
            'color: "#10000000"\n'
            'color: "#e60a0b0d"\n'
            "opacity: activeFocus ? 1 : 0\n"
            "opacity: plate.activeFocus ? 1 : 0\n"
        ), [])

    def test_scans_every_shell_qml_except_theme(self):
        ui = ROOT / "apps" / "arinn-shell" / "ui"
        expected = sorted(path for path in ui.rglob("*.qml") if path.name != "Theme.qml")
        files = check_focus_fill.screen_files(ROOT)
        self.assertEqual(files, expected)
        self.assertGreater(len(files), 12)
        self.assertTrue(all(path.name != "Theme.qml" for path in files))

    def test_shell_qml_uses_theme_plate_focus(self):
        self.assertEqual(check_focus_fill.scan(ROOT), [])
