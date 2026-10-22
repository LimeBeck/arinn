#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Arinn contributors
# SPDX-License-Identifier: GPL-3.0-or-later
"""Restart running Arinn user services after an upgrade (#355).

    python tests/package/test_restart_user_services.py

Runs the restart script, the Arch install script and the Debian postinst
against stub systemctl and getent; never talks to a real user manager.
ARINN_RESTART_SCRIPT points at an installed copy instead of the source.
"""
import fnmatch
import os
from pathlib import Path
import re
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
SCRIPT = Path(os.environ.get("ARINN_RESTART_SCRIPT", ROOT / "packaging/bin/arinn-restart-user-services"))
HOOK = ROOT / "packaging/arch/arinn-restart-user-services.hook"
PKGBUILD = ROOT / "packaging/arch/PKGBUILD"
ORDER = ["arinnd.service", "arinn-kwin.service", "arinn-ewmh.service", "arinn-shell.service"]
PASSWD = {"0": "root", "1000": "alice", "1001": "bob", "1002": "carol"}


def stub(folder, name, body):
    path = folder / name
    path.write_text("#!/bin/sh\n" + body)
    path.chmod(0o755)
    return path


class RestartUserServices(unittest.TestCase):
    def run_restart(self, managers, running=(), failing=(), systemd=True):
        """managers: user@<uid>.service units of the system manager;
        running: users with at least one active Arinn unit;
        failing: units whose try-restart fails."""
        with tempfile.TemporaryDirectory(prefix="arinn-restart-test-") as folder:
            root = Path(folder)
            log = root / "log"
            log.write_text("")
            listing = "".join(f"{unit} loaded active running User Manager for UID {unit[5:-8]}\n"
                              for unit in managers)
            stub(root, "systemctl",
                 f'echo "systemctl $*" >> {log}\n'
                 'case "$*" in\n'
                 f'  list-units*) [ {int(systemd)} = 1 ] || exit 1; printf "%s" "{listing}" ;;\n'
                 + "".join(f'  *"-M {user}@ is-active"*) exit 0 ;;\n' for user in running)
                 + '  *is-active*) exit 3 ;;\n'
                 + "".join(f'  *try-restart*{unit}) exit 1 ;;\n' for unit in failing)
                 + 'esac\nexit 0\n')
            stub(root, "getent",
                 'case "$2" in\n'
                 + "".join(f'  {uid}) echo "{name}:x:{uid}:{uid}::/home/{name}:/bin/sh" ;;\n'
                           for uid, name in PASSWD.items())
                 + '  *) exit 2 ;;\nesac\n')
            env = dict(os.environ, PATH=f"{root}:{os.environ['PATH']}")
            result = subprocess.run(["sh", str(SCRIPT)], env=env, stdin=subprocess.DEVNULL,
                                    capture_output=True, text=True, timeout=10)
            return result, log.read_text().splitlines()

    def restarts(self, log):
        return [line.split()[-2:] for line in log if "try-restart" in line]

    def test_restarts_only_users_who_run_arinn_in_order(self):
        result, log = self.run_restart(
            ["user@1000.service", "user@1001.service", "user@1002.service"],
            running=["alice", "carol"])
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(
            [line for line in log if "try-restart" in line],
            [f"systemctl --user -M {user}@ try-restart {unit}"
             for user in ("alice", "carol") for unit in ORDER])
        self.assertIn("systemctl --user -M bob@ is-active --quiet " + " ".join(ORDER), log)
        self.assertNotIn("bob", "".join(line for line in log if "try-restart" in line))
        self.assertIn("playback in progress is interrupted", result.stdout)
        self.assertEqual(result.stdout.count("Restarting Arinn for user"), 2, result.stdout)

    def test_only_lists_running_user_managers(self):
        result, log = self.run_restart([])
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(log, ["systemctl list-units --plain --no-legend --no-pager "
                               "--state=running user@*.service"])
        self.assertEqual(result.stdout, "")

    def test_nothing_runs_without_systemd(self):
        result, log = self.run_restart(["user@1000.service"], running=["alice"], systemd=False)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(len(log), 1, log)
        self.assertEqual(result.stdout, "")

    def test_skips_unknown_users(self):
        result, log = self.run_restart(["user@4242.service"], running=["alice"])
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(len(log), 1, log)

    def test_a_failed_restart_never_fails_the_transaction(self):
        result, log = self.run_restart(["user@1000.service"], running=["alice"],
                                       failing=["arinn-kwin.service"])
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual([unit for _, unit in self.restarts(log)], ORDER)
        self.assertIn("arinn-kwin.service of user alice was not restarted", result.stdout)


def hook():
    sections, section = {}, None
    for line in HOOK.read_text().splitlines():
        line = line.strip()
        if line.startswith("[") and line.endswith("]"):
            section = sections.setdefault(line[1:-1], {})
        elif line and not line.startswith("#"):
            key, value = (part.strip() for part in line.split("=", 1))
            section.setdefault(key, []).append(value)
    return sections


class ArchHook(unittest.TestCase):
    def test_runs_after_upgrades_only(self):
        sections = hook()
        self.assertEqual(sections["Trigger"]["Type"], ["Path"])
        self.assertEqual(sections["Trigger"]["Operation"], ["Upgrade"])
        self.assertEqual(sections["Action"]["When"], ["PostTransaction"])
        self.assertEqual(sections["Action"]["Exec"], ["/usr/lib/arinn/arinn-restart-user-services"])

    def test_targets_match_the_shipped_binaries(self):
        text = PKGBUILD.read_text()
        executables = re.findall(r'install -Dm755 [^\n]*"\$\{pkgdir\}/(usr/[^"]+)"', text)
        files = re.findall(r'"\$\{pkgdir\}/(usr/[^"]+)"', text)
        self.assertIn("usr/bin/arinnd", executables)
        self.assertIn("usr/lib/arinn/arinn-restart-user-services", executables)
        self.assertIn("usr/share/libalpm/hooks/arinn-restart-user-services.hook", files)
        targets = hook()["Trigger"]["Target"]
        # libalpm matches with fnmatch(3) without FNM_PATHNAME: * spans slashes.
        long_running = [path for path in executables if path != "usr/bin/arinnctl"]
        for path in long_running + ["usr/share/arinn/kwin/phase0.js"]:
            self.assertTrue(any(fnmatch.fnmatchcase(path, target) for target in targets), path)
        for target in targets:
            self.assertTrue(any(fnmatch.fnmatchcase(path, target) for path in files), target)


class InstallScripts(unittest.TestCase):
    def test_arch_post_upgrade_says_sessions_restart(self):
        with tempfile.TemporaryDirectory(prefix="arinn-install-test-") as folder:
            stubs = Path(folder)
            stub(stubs, "vercmp", 'echo 1\n')
            script = f'. "{ROOT / "packaging/arch/arinn.install"}"; post_upgrade 0.2.0-1 0.1.1-1'
            result = subprocess.run(["sh", "-c", script], capture_output=True, text=True, timeout=10,
                                    env=dict(os.environ, PATH=f"{stubs}:{os.environ['PATH']}"))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout,
                         ":: Running Arinn sessions are restarted at the end of this transaction.\n")

    def run_postinst(self, *args):
        with tempfile.TemporaryDirectory(prefix="arinn-postinst-test-") as folder:
            root = Path(folder)
            log = root / "log"
            restart = stub(root, "restart", f'echo restart >> {log}\nexit 1\n')
            postinst = root / "postinst"
            postinst.write_text((ROOT / "packaging/debian/debian/postinst").read_text()
                                .replace("/usr/lib/arinn/arinn-restart-user-services", str(restart)))
            result = subprocess.run(["sh", str(postinst), *args], capture_output=True, text=True,
                                    timeout=10)
            return result, log.read_text() if log.exists() else ""

    def test_debian_upgrade_restarts_and_install_does_not(self):
        result, log = self.run_postinst("configure", "0.1.0-1")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(log, "restart\n")
        result, log = self.run_postinst("configure", "")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(log, "")
        result, log = self.run_postinst("configure")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(log, "")


if __name__ == "__main__":
    unittest.main()
