#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Arinn contributors
# SPDX-License-Identifier: GPL-3.0-or-later
"""Debian postinst must not auto-enable session targets (#708, ADR-0042).

    python tests/package/test_deb_postinst_systemd.py

Uses stub deb-systemd-helper; never touches a real user manager.
"""
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
RULES = ROOT / "packaging/debian/debian/rules"
POSTINST = ROOT / "packaging/debian/debian/postinst"
TARGETS = ("arinn-session.target", "arinn-ewmh-session.target")
# Minimal snippet from dh_installsystemduser --no-enable (was-enabled re-enable path).
DEBHELPER_ENABLE = """
if [ "$1" = "configure" ]; then
  deb-systemd-helper --user enable arinn-session.target
  deb-systemd-helper --user enable arinn-ewmh-session.target
fi
"""


class DebSystemdOptIn(unittest.TestCase):
    def test_rules_installs_user_units_without_enabling(self):
        text = RULES.read_text()
        self.assertIn("override_dh_installsystemduser:", text)
        self.assertIn("dh_installsystemduser --no-enable", text)

    def run_postinst(self, old_version, debhelper_snippet=""):
        with tempfile.TemporaryDirectory(prefix="arinn-deb708-") as folder:
            stubs = Path(folder)
            log = stubs / "log"
            log.write_text("")
            helper = stubs / "systemd-helper-stub"
            helper.write_text(
                "#!/bin/sh\n"
                f'echo "$@" >> {log}\n'
                "exit 0\n"
            )
            helper.chmod(0o755)
            restart = stubs / "restart"
            restart.write_text("#!/bin/sh\nexit 0\n")
            restart.chmod(0o755)
            script = stubs / "postinst"
            body = POSTINST.read_text().replace(
                "/usr/lib/arinn/arinn-restart-user-services", str(restart)
            ).replace("#DEBHELPER#", debhelper_snippet)
            script.write_text(body.replace("deb-systemd-helper", str(helper)))
            script.chmod(0o755)
            args = ["configure", old_version] if old_version is not None else ["configure"]
            result = subprocess.run(
                ["sh", str(script), *args],
                capture_output=True,
                text=True,
                timeout=10,
                env=dict(os.environ, PATH=f"{stubs}:{os.environ['PATH']}"),
            )
            return result, log.read_text().splitlines()

    def test_upgrade_from_0_2_0_1_disables_both_session_targets_once(self):
        result, log = self.run_postinst("0.2.0-1")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(
            log,
            ["--user disable arinn-session.target",
             "--user disable arinn-ewmh-session.target"],
        )

    def test_upgrade_from_0_1_0_1_also_disables_legacy_autostart(self):
        result, log = self.run_postinst("0.1.0-1")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(len(log), 2)

    def test_upgrade_from_newer_release_does_not_disable_opt_in(self):
        result, log = self.run_postinst("0.2.1-1")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(log, [])

    def test_fresh_install_never_calls_disable(self):
        result, log = self.run_postinst("")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(log, [])

    def test_legacy_upgrade_runs_disable_after_debhelper_reenable(self):
        result, log = self.run_postinst("0.2.0-1", DEBHELPER_ENABLE)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(
            log,
            ["--user enable arinn-session.target",
             "--user enable arinn-ewmh-session.target",
             "--user disable arinn-session.target",
             "--user disable arinn-ewmh-session.target"],
        )


if __name__ == "__main__":
    unittest.main()
