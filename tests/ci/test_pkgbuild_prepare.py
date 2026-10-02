# SPDX-FileCopyrightText: 2026 Arinn contributors
#
# SPDX-License-Identifier: GPL-3.0-or-later

"""prepare() makes extracted sources newer than a cached build (#238, ADR-0047)."""

import os
import pathlib
import subprocess
import tempfile
import time
import unittest

PKGBUILD = pathlib.Path(__file__).resolve().parents[2] / "packaging" / "arch" / "PKGBUILD"


class PrepareTest(unittest.TestCase):
    def test_prepare_refreshes_source_mtimes(self):
        with tempfile.TemporaryDirectory() as folder:
            root = pathlib.Path(folder)
            source = root / "arinn-0.1.0" / "plugins" / "mpv" / "src"
            source.mkdir(parents=True)
            old = time.time() - 86400 * 365
            for name in ("lib.rs", "main.rs"):
                path = source / name
                path.write_text("// fixture\n")
                os.utime(path, (old, old))
            stubs = root / "bin"
            stubs.mkdir()
            for tool in ("cargo", "rustc"):
                stub = stubs / tool
                stub.write_text("#!/bin/sh\nexit 0\n")
                stub.chmod(0o755)
            started = time.time() - 1
            script = f'source "{PKGBUILD}"; cd "{root}"; prepare'
            env = {**os.environ, "PATH": f"{stubs}:{os.environ['PATH']}"}
            result = subprocess.run(["bash", "-c", script], env=env,
                                    capture_output=True, text=True, timeout=30)
            self.assertEqual(result.returncode, 0, result.stderr)
            for path in source.iterdir():
                self.assertGreaterEqual(path.stat().st_mtime, started, path.name)


if __name__ == "__main__":
    unittest.main()
