#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Arinn contributors
# SPDX-License-Identifier: GPL-3.0-or-later
"""Run the session selector against a synthetic shell, without a desktop."""
import json
import os
import re
import shlex
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]


class ShellLauncher(unittest.TestCase):
    def launch(self, session=None, backend=None, display=None, wayland=None):
        with tempfile.TemporaryDirectory(prefix="arinn-launch-test-") as folder:
            root = Path(folder)
            shell = root / "shell"
            shell.write_text("#!/usr/bin/env python3\nimport json,os,sys\n"
                             "print(json.dumps([os.environ['QT_QPA_PLATFORM'],sys.argv[1:]]))\n")
            shell.chmod(0o755)
            launcher = root / "launcher"
            launcher.write_text((ROOT / "platform/kde/arinn-shell-launch").read_text()
                                .replace("/usr/bin/arinn-shell", str(shell)))
            env = dict(os.environ)
            for name in ("XDG_SESSION_TYPE", "ARINN_SESSION_BACKEND", "DISPLAY", "WAYLAND_DISPLAY"):
                env.pop(name, None)
            if session is not None:
                env["XDG_SESSION_TYPE"] = session
            if backend is not None:
                env["ARINN_SESSION_BACKEND"] = backend
            if display is not None:
                env["DISPLAY"] = display
            if wayland is not None:
                env["WAYLAND_DISPLAY"] = wayland
            return subprocess.run(["sh", str(launcher), "synthetic argument"],
                                  env=env, capture_output=True, text=True, timeout=5)

    def test_wayland(self):
        result = self.launch("wayland")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(result.stdout), ["wayland", ["synthetic argument"]])

    def test_x11(self):
        result = self.launch("x11")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(result.stdout), ["xcb", ["synthetic argument"]])

    def test_override(self):
        result = self.launch("wayland", "x11")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(result.stdout)[0], "xcb")

    def test_stale_selection_with_only_the_other_display_is_rejected(self):
        for result in (self.launch("x11", wayland="synthetic-wayland"),
                       self.launch("wayland", display=":synthetic"),
                       self.launch("wayland", "x11", wayland="synthetic-wayland"),
                       self.launch("x11", "wayland", display=":synthetic")):
            self.assertEqual(result.returncode, 2)
            self.assertEqual(result.stdout, "")
            self.assertIn("backend has only", result.stderr)

    def test_both_displays_allow_explicit_transport_selection(self):
        for backend, platform in (("x11", "xcb"), ("wayland", "wayland")):
            result = self.launch("wayland", backend, ":synthetic", "synthetic-wayland")
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(json.loads(result.stdout)[0], platform)

    def test_required_python_is_available_to_no_check_package_builds(self):
        # CMake requires Python before test execution, including the pre-existing
        # performance subproject; checkdepends are absent in makepkg --nocheck.
        cmake = (ROOT / "CMakeLists.txt").read_text() + (ROOT / "scripts/perf/CMakeLists.txt").read_text()
        if "find_package(Python3 REQUIRED" in cmake:
            pkgbuild = (ROOT / "packaging/arch/PKGBUILD").read_text()
            make = re.search(r"^makedepends=\((.*?)\)", pkgbuild, re.M | re.S)
            self.assertIsNotNone(make)
            self.assertIn("python", shlex.split(make.group(1)))

    def test_missing_type_preserves_wayland_default(self):
        self.assertEqual(json.loads(self.launch().stdout)[0], "wayland")

    def test_unsupported_backend_does_not_launch(self):
        result = self.launch("tty")
        self.assertEqual(result.returncode, 2)
        self.assertEqual(result.stdout, "")


if __name__ == "__main__":
    unittest.main()
