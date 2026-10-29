# SPDX-FileCopyrightText: 2026 Arinn contributors
#
# SPDX-License-Identifier: GPL-3.0-or-later

"""The XFCE launcher's name handover and the session entry's watch stay in
step (#483): the launcher waits strictly longer than the previous launcher's
cleanup, and the session entry watches longer than that wait plus a start."""

import pathlib
import re
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[2]
HELPER = ROOT / "platform/x11/src/bin/arinn-xfce-session.rs"
ENTRY = ROOT / "packaging/bin/arinn-session"


def seconds(source, name):
    match = re.search(rf"const {name}: Duration = Duration::from_secs\((\d+)\);", source)
    if not match:
        raise AssertionError(f"{name} not found")
    return int(match.group(1))


class XfceHandover(unittest.TestCase):
    def test_the_watch_outlasts_the_handover_and_a_start(self):
        helper = HELPER.read_text()
        systemctl = seconds(helper, "SYSTEMCTL_TIMEOUT")
        handover = seconds(helper, "HANDOVER")
        # The previous launcher's cleanup: two systemctl calls, then its exit.
        self.assertGreater(handover, 2 * systemctl)
        ticks = re.search(r"^    watch=(\d+)$", ENTRY.read_text(), re.M)
        self.assertIsNotNone(ticks, "the watch bound of arinn-session")
        watch = int(ticks.group(1)) * 0.5
        # The wait, then a start whose target start alone may take one call.
        self.assertGreater(watch, handover + systemctl)


if __name__ == "__main__":
    unittest.main()
