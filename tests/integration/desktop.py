#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Arinn contributors
# SPDX-License-Identifier: GPL-3.0-or-later

"""Desktop-provider integration. Requires a private dbus-run-session, never KWin."""
import json
import os
from pathlib import Path
import signal
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parents[2]
BIN = ROOT / "target/debug"

def wait(predicate, timeout=15):
    end = time.monotonic() + timeout
    while time.monotonic() < end:
        value = predicate()
        if value:
            return value
        time.sleep(0.05)
    raise AssertionError("desktop fixture condition timed out")

with tempfile.TemporaryDirectory(prefix="arinn-desktop-integration-") as folder:
    root = Path(folder)
    env = {**os.environ, "HOME": folder, "XDG_CONFIG_HOME": str(root / "config"),
           "XDG_DATA_HOME": str(root / "data"), "XDG_DATA_DIRS": str(root / "system"),
           "XDG_STATE_HOME": str(root / "state"), "XDG_RUNTIME_DIR": str(root / "run"),
           "ARINN_PRIVATE_TEST_BUS": "1", "LD_PRELOAD": ""}
    (root / "run").mkdir(mode=0o700)
    manifests = root / "data/arinn/plugins"
    manifests.mkdir(parents=True)
    applications = root / "data/applications"
    applications.mkdir()
    manifest = manifests / "desktop.toml"
    manifest.write_text(f'id="desktop"\nname="Desktop fixture"\nversion="0.0.1"\napi_version=1\n'
                        f'executable="{BIN / "arinn-plugin-desktop"}"\ncapabilities=["application"]\n')
    # The temporary application exits on explicit release or fixture-directory
    # cleanup. No process-name kill, user application or host session is involved.
    helper = root / "sentinel.sh"
    helper.write_text(f'#!/bin/sh\nprintf "%s" "$$" > "{root / "pid"}"\n'
                      f'printf x >> "{root / "launches"}"\n'
                      f'while test -d "{root}" && ! test -e "{root / "release"}"; do sleep 0.05; done\n')
    helper.chmod(0o700)
    (applications / "fixture.desktop").write_text(f'[Desktop Entry]\nType=Application\nName=Fixture\nExec={helper} %U\n')
    (applications / "broken.desktop").write_text('[Desktop Entry]\nType=Application\nName=Broken\nExec=/arinn-test-no-such-program\n')
    processes, logs = [], []
    def start(binary):
        log = tempfile.TemporaryFile(mode="w+")
        logs.append(log)
        process = subprocess.Popen([str(binary)], env=env, stdout=log, stderr=log)
        processes.append(process)
        return process
    def cli(*args):
        result = subprocess.run([str(BIN / "arinnctl"), *args], env=env, capture_output=True, text=True, timeout=45)
        assert result.returncode == 0, result.stderr
        return result
    def providers():
        result = subprocess.run([str(BIN / "arinnctl"), "--json", "providers"], env=env, capture_output=True, text=True, timeout=4)
        return json.loads(result.stdout) if result.returncode == 0 else []
    def ready(generation=""):
        return next((p for p in providers() if p["id"] == "desktop" and p["runtime_state"] == "ready" and p["generation"] != generation), None)
    def alive():
        try:
            os.kill(int((root / "pid").read_text()), 0)
            return True
        except (OSError, ValueError):
            return False
    try:
        start(BIN / "examples/private-systemd")
        daemon = start(BIN / "arinnd")
        first = wait(ready)
        result = subprocess.run([str(BIN / "examples/desktop-client")], env=env, capture_output=True, text=True, timeout=35)
        assert result.returncode == 0, result.stdout + result.stderr
        wait(alive)
        assert (root / "launches").read_text() == "x", "deduplicated launch started twice"
        cli("provider", "restart", "desktop")
        second = wait(lambda: ready(first["generation"]))
        assert alive(), "provider restart killed the fixture application"
        daemon.send_signal(signal.SIGINT)
        daemon.wait(timeout=5)
        daemon = start(BIN / "arinnd")
        wait(lambda: ready(second["generation"]))
        assert alive(), "daemon restart killed the fixture application"
        manifest.unlink()
        cli("providers", "reload")
        wait(lambda: providers() == [])
        assert alive(), "provider removal killed the fixture application"
        print("PASS: desktop provider registration, launch, restart/removal and independent fixture lifetime (mock manager; not cgroup qualification)")
    finally:
        (root / "release").touch()
        for process in reversed(processes):
            if process.poll() is None:
                process.send_signal(signal.SIGINT)
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()
        for log in logs:
            log.close()
