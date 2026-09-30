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
import signal
import os
import subprocess
import tempfile
import time
from pathlib import Path

PREFIX = Path(os.environ.get("ARINN_PREFIX", "/usr"))
BIN = PREFIX / "bin"
DEMO = PREFIX / "lib/arinn/plugins/arinn-plugin-demo"
DESKTOP = PREFIX / "lib/arinn/plugins/arinn-plugin-desktop"
MPRIS = PREFIX / "lib/arinn/plugins/arinn-plugin-mpris"
LIBRARY = PREFIX / "lib/arinn/plugins/arinn-plugin-library"
MPV = PREFIX / "lib/arinn/plugins/arinn-plugin-mpv"
TRANSMISSION = PREFIX / "lib/arinn/plugins/arinn-plugin-transmission"
assert (PREFIX / "share/arinn/plugins/desktop.toml").is_file()
mpris_manifest = (PREFIX / "share/arinn/plugins/mpris.toml").read_text()
for line in (
    'id = "mpris"',
    'capabilities = ["media"]',
    'executable = "/usr/lib/arinn/plugins/arinn-plugin-mpris"',
):
    assert line in mpris_manifest, mpris_manifest
library_manifest = (PREFIX / "share/arinn/plugins/library.toml").read_text()
for line in (
    'id = "library"',
    'capabilities = ["library"]',
    'executable = "/usr/lib/arinn/plugins/arinn-plugin-library"',
):
    assert line in library_manifest, library_manifest
mpv_manifest = (PREFIX / "share/arinn/plugins/mpv.toml").read_text()
for line in (
    'id = "mpv"',
    'capabilities = ["media"]',
    'executable = "/usr/lib/arinn/plugins/arinn-plugin-mpv"',
):
    assert line in mpv_manifest, mpv_manifest
transmission_manifest = (PREFIX / "share/arinn/plugins/transmission.toml").read_text()
for line in (
    'id = "transmission"',
    'capabilities = ["download"]',
    'executable = "/usr/lib/arinn/plugins/arinn-plugin-transmission"',
):
    assert line in transmission_manifest, transmission_manifest
assert (PREFIX / "share/arinn/web-shortcuts.toml").is_file()
assert (PREFIX / "share/arinn/browser-applications.toml").is_file()
assert (PREFIX / "share/arinn/search-engines.toml").is_file()


def cli(*args):
    result = subprocess.run([str(BIN / "arinnctl"), "--json", *args], env=env, capture_output=True, text=True, timeout=9)
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


for path in (BIN / "arinnd", BIN / "arinnctl", BIN / "arinn-shell", DEMO, DESKTOP, MPRIS, LIBRARY, MPV, TRANSMISSION):
    assert path.is_file() and os.access(path, os.X_OK), f"missing executable {path}"

with tempfile.TemporaryDirectory(prefix="arinn-smoke-") as folder:
    root = Path(folder)
    env = {**os.environ, "HOME": folder, "XDG_CONFIG_HOME": str(root / "config"),
           "XDG_DATA_HOME": str(root / "data"), "XDG_DATA_DIRS": str(root / "system"),
           "XDG_STATE_HOME": str(root / "state"), "XDG_RUNTIME_DIR": str(root / "run"),
           "ARINN_PRIVATE_TEST_BUS": "1"}
    (root / "run").mkdir(mode=0o700)
    manifests = root / "data/arinn/plugins"
    manifests.mkdir(parents=True)
    (manifests / "demo.toml").write_text(
        f'id = "demo"\nname = "Demo"\nversion = "0.0.1"\napi_version = 1\n'
        f'executable = "{DEMO}"\ncapabilities = ["application"]\n')
    (manifests / "desktop.toml").write_text(
        f'id="desktop"\nname="Desktop"\nversion="0.0.1"\napi_version=1\n'
        f'executable="{DESKTOP}"\ncapabilities=["application"]\n')
    applications = root / "data/applications"
    applications.mkdir()
    (applications / "fixture.desktop").write_text(
        '[Desktop Entry]\nType=Application\nName=Packaged fixture\nExec=/usr/bin/true\n')
    manager = Path(os.environ.get("ARINN_TEST_MANAGER", "target/debug/examples/private-systemd"))
    assert manager.is_file(), "build the private-systemd example or set ARINN_TEST_MANAGER"
    processes = []

    def start(command, **extra):
        process = subprocess.Popen([str(command)], env={**env, **extra},
                                   stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, text=True)
        processes.append(process)
        return process

    try:
        start(manager)
        start(BIN / "arinnd")
        wait(lambda: json.loads(cli("status"))["epoch"])
        wait(lambda: json.loads(cli("status"))["provider_available"])
        snapshot = json.loads(cli("snapshot", "demo"))[0]
        assert snapshot["revision"] >= 1 and snapshot["applications"], snapshot

        wait(lambda: any(p["id"] == "desktop" and p["runtime_state"] == "ready" for p in json.loads(cli("providers"))))
        desktop = json.loads(cli("snapshot", "desktop"))[0]
        assert desktop["applications"][0]["id"] == "fixture.desktop"
        assert desktop["running"] == []

        shell = start(BIN / "arinn-shell", QT_QPA_PLATFORM="offscreen", QT_QUICK_BACKEND="software")
        time.sleep(5)
        if shell.poll() is not None:
            raise AssertionError(f"arinn-shell exited early with {shell.returncode}: {shell.stderr.read()}")
        print("PASS: packaged daemon, demo/desktop providers, CLI and shell start and communicate")
    finally:
        for process in reversed(processes):
            if process.poll() is None:
                process.send_signal(signal.SIGINT)
        for process in processes:
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()
