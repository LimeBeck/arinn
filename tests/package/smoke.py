#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2026 Arinn contributors
#
# SPDX-License-Identifier: GPL-3.0-or-later

"""Smoke test of an installed Arinn package.

Run on a private session bus after installing the package:
    dbus-run-session -- python tests/package/smoke.py

Checks that the packaged binaries start, talk to each other over D-Bus and
that the shell finds its Qt/QML runtime dependencies. It never touches a real
desktop session.
"""

import json
import os
import subprocess
import tempfile
import time
from pathlib import Path

PREFIX = Path(os.environ.get("ARINN_PREFIX", "/usr"))
BIN = PREFIX / "bin"
DEMO = PREFIX / "lib/arinn/plugins/arinn-plugin-demo"


def cli(*args):
    result = subprocess.run([str(BIN / "arinnctl"), *args], capture_output=True, text=True, timeout=9)
    if result.returncode:
        raise AssertionError(result.stderr)
    return result.stdout


def wait(predicate, timeout=10):
    end = time.monotonic() + timeout
    while time.monotonic() < end:
        try:
            if predicate():
                return
        except (AssertionError, json.JSONDecodeError):
            pass
        time.sleep(0.1)
    raise AssertionError("condition timed out")


for path in (BIN / "arinnd", BIN / "arinnctl", BIN / "arinn-shell", DEMO):
    assert path.is_file() and os.access(path, os.X_OK), f"missing executable {path}"

with tempfile.TemporaryDirectory(prefix="arinn-smoke-") as folder:
    token = Path(folder) / "token"
    token.write_text("package-smoke-test-token-0000000000")
    token.chmod(0o600)
    env = {**os.environ, "ARINN_SPIKE_TOKEN_FILE": str(token)}
    processes = []

    def start(command, **extra):
        process = subprocess.Popen([str(command)], env={**env, **extra},
                                   stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, text=True)
        processes.append(process)
        return process

    try:
        start(BIN / "arinnd")
        wait(lambda: json.loads(cli("status"))["epoch"])
        start(DEMO)
        wait(lambda: json.loads(cli("status"))["provider_available"])
        snapshot = json.loads(cli("snapshot"))
        assert snapshot["revision"] >= 1 and snapshot["items"], snapshot

        shell = start(BIN / "arinn-shell", QT_QPA_PLATFORM="offscreen", QT_QUICK_BACKEND="software")
        time.sleep(5)
        if shell.poll() is not None:
            raise AssertionError(f"arinn-shell exited early with {shell.returncode}: {shell.stderr.read()}")
        print("PASS: packaged daemon, demo provider, CLI and shell start and communicate")
    finally:
        for process in reversed(processes):
            if process.poll() is None:
                process.terminate()
        for process in processes:
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()
