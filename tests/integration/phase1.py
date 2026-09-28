#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Arinn contributors
# SPDX-License-Identifier: GPL-3.0-or-later

"""Production runtime contract on a private bus and a test-only systemd double."""
import json
import os
from pathlib import Path
import signal
import subprocess
import tempfile
import time
import sys
import uuid
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parents[2]
BIN = ROOT / "target/debug"

def wait(predicate, timeout=12):
    end = time.monotonic() + timeout
    while time.monotonic() < end:
        try:
            value = predicate()
            if value:
                return value
        except (AssertionError, KeyError, IndexError, json.JSONDecodeError):
            pass
        time.sleep(0.05)
    raise AssertionError("condition timed out")

with tempfile.TemporaryDirectory(prefix="arinn-production-") as folder:
    root = Path(folder)
    env = {**os.environ, "HOME": folder, "XDG_CONFIG_HOME": str(root / "config"),
           "XDG_DATA_HOME": str(root / "data"), "XDG_DATA_DIRS": str(root / "system"),
           "XDG_STATE_HOME": str(root / "state"), "XDG_RUNTIME_DIR": str(root / "run"),
           "ARINN_PRIVATE_TEST_BUS": "1", "LD_PRELOAD": ""}
    (root / "run").mkdir(mode=0o700)
    manifests = root / "data/arinn/plugins"
    manifests.mkdir(parents=True)
    processes = []
    logs = []

    def start(binary):
        log = tempfile.TemporaryFile(mode="w+")
        logs.append(log)
        child = subprocess.Popen([str(binary)], env=env, stdout=log, stderr=log)
        processes.append(child)
        return child

    def cli(*args, ok=True):
        result = subprocess.run([str(BIN / "arinnctl"), *args], env=env,
                                capture_output=True, text=True, timeout=45)
        if ok:
            assert result.returncode == 0, result.stderr
        return result

    def providers():
        return json.loads(cli("--json", "providers").stdout)

    def ready():
        return next((p for p in providers() if p["id"] == "test" and p["runtime_state"] == "ready"), None)

    def manifest(fixture="populated", name="Test"):
        (manifests / "test.toml").write_text(
            f'id = "test"\nname = "{name}"\nversion = "0.0.1"\napi_version = 1\n'
            f'executable = "{BIN / "arinn-plugin-demo"}"\ncapabilities = ["application"]\n'
            f'args = ["--fixture", "{fixture}"]\n')

    def contract(mode="basic"):
        result = subprocess.run([str(BIN / "examples/contract-client"), "test", mode], env=env,
                                capture_output=True, text=True, timeout=25)
        assert result.returncode == 0, result.stdout + result.stderr

    def wire_contract(service, path, interface, filename):
        actual = subprocess.run(["busctl", "--user", "--xml-interface", "introspect", service, path],
                                env=env, capture_output=True, text=True, timeout=3)
        assert actual.returncode == 0, actual.stderr
        element = ET.fromstring(actual.stdout).find(f"interface[@name='{interface}']")
        assert element is not None
        document = ROOT / "docs/plugin-api" / filename
        if "--update-xml" in sys.argv:
            node = ET.Element("node")
            node.append(element)
            ET.indent(node)
            # REUSE-IgnoreStart (header of the generated XML, not of this file)
            document.write_text('<!-- SPDX-FileCopyrightText: 2026 Arinn contributors\nSPDX-License-Identifier: MIT OR Apache-2.0 -->\n' + ET.tostring(node, encoding="unicode") + "\n")
            # REUSE-IgnoreEnd
        expected = ET.parse(document).getroot().find(f"interface[@name='{interface}']")
        def normalized(node):
            children = [normalized(child) for child in node]
            if node.tag in ("node", "interface"):
                children.sort()
            return node.tag, tuple(sorted(node.attrib.items())), tuple(children)
        assert normalized(expected) == normalized(element), f"wire contract drift: {filename}"

    try:
        manifest()
        daemon = start(BIN / "arinnd")
        # A missing manager must leave a diagnostic, not prevent recovery by an
        # explicit restart once the manager becomes available (no unit exists).
        wait(lambda: providers() and providers()[0]["runtime_state"] == "failed")
        manager = start(BIN / "examples/private-systemd")
        def manager_ready():
            result = subprocess.run(["busctl", "--user", "status", "org.freedesktop.systemd1"],
                                    env=env, capture_output=True, timeout=3)
            return result.returncode == 0
        wait(manager_ready)
        cli("provider", "restart", "test")
        first = wait(ready)
        wire_contract("org.arinn.Daemon1", "/org/arinn/Daemon1", "org.arinn.Daemon1", "daemon1.xml")
        wire_contract("org.arinn.Daemon1", first["path"], "org.arinn.Application1", "application1.xml")
        created = subprocess.run(["busctl", "--user", "--json=short", "call", "org.arinn.Daemon1", first["path"],
                                  "org.arinn.Application1", "Activate", "sss", first["generation"], "demo.activity", str(uuid.uuid4())],
                                 env=env, capture_output=True, text=True, timeout=3)
        assert created.returncode == 0, created.stderr
        operation_path = json.loads(created.stdout)["data"][0]
        wire_contract("org.arinn.Daemon1", operation_path, "org.arinn.Operation1", "operation1.xml")
        names = subprocess.run(["busctl", "--user", "--json=short", "call", "org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus", "ListNames"], env=env, capture_output=True, text=True, timeout=3)
        for owner in json.loads(names.stdout)["data"][0]:
            if not owner.startswith(":"):
                continue
            introspection = subprocess.run(["busctl", "--user", "--xml-interface", "introspect", owner, "/org/arinn/Provider1"], env=env, capture_output=True, text=True, timeout=3)
            if introspection.returncode == 0 and 'interface name="org.arinn.Provider1"' in introspection.stdout:
                wire_contract(owner, "/org/arinn/Provider1", "org.arinn.Provider1", "provider1.xml")
                break
        else:
            raise AssertionError("provider introspection missing")
        pages = json.loads(cli("--json", "snapshot", "test").stdout)
        assert pages[0]["revision"] == 1
        assert pages[0]["running"][0]["id"] == "demo.activity"
        cli("activate", "test", "demo.activity")
        cli("launch", "test", "demo.application.0")
        assert cli("activate", "test", "unknown", ok=False).returncode == 1
        assert cli("nonsense", ok=False).returncode == 2
        manifest(name="Renamed")
        cli("providers", "reload")
        assert ready()["generation"] == first["generation"]
        assert ready()["name"] == "Renamed"
        cli("provider", "restart", "test")
        second = wait(lambda: (p := ready()) and p["generation"] != first["generation"] and p)
        assert second["path"] == first["path"]
        daemon.send_signal(signal.SIGINT)
        daemon.wait(timeout=5)
        assert cli("status", ok=False).returncode == 3
        daemon = start(BIN / "arinnd")
        third = wait(lambda: (p := ready()) and p["generation"] != second["generation"] and p)
        assert third["path"] == first["path"]
        contract()
        manifest("large")
        cli("providers", "reload")
        wait(lambda: (p := ready()) and p["generation"] != third["generation"])
        pages = json.loads(cli("--json", "snapshot", "test").stdout)
        assert sum(len(p["applications"]) for p in pages) == 401
        assert len(pages) == 3
        contract()
        previous = ready()["generation"]
        manifest("revision-race")
        cli("providers", "reload")
        wait(lambda: (p := ready()) and p["generation"] != previous and p["revision"] > 1)
        contract()
        previous = ready()["generation"]
        manifest("reconnect")
        cli("providers", "reload")
        reconnected = wait(lambda: (p := ready()) and p["generation"] != previous and p)
        wait(lambda: (p := ready()) and p["generation"] != reconnected["generation"])
        previous = ready()["generation"]
        manifest("crash")
        cli("providers", "reload")
        crashed = wait(lambda: (p := ready()) and p["generation"] != previous and p)
        assert cli("activate", "test", "demo.activity", ok=False).returncode == 1
        wait(lambda: (p := ready()) and p["generation"] != crashed["generation"])
        manifest("invalid-id")
        cli("providers", "reload")
        wait(lambda: providers()[0]["runtime_state"] == "failed", timeout=20)
        assert not ready()
        manifest("unavailable")
        cli("providers", "reload")
        wait(ready)
        assert ready()["backend_state"] == "unavailable"
        assert cli("activate", "test", "demo.activity", ok=False).returncode == 1
        (manifests / "test.toml").unlink()
        cli("providers", "reload")
        assert providers() == []
        manifest("delayed")
        cli("providers", "reload")
        wait(ready)
        contract("limits")
        wait(ready)
        before = time.monotonic()
        action = subprocess.Popen([str(BIN / "arinnctl"), "activate", "test", "demo.activity"], env=env,
                                  stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        cli("status")
        assert time.monotonic() - before < 1
        _, error = action.communicate(timeout=9)
        assert action.returncode == 1 and "Timeout" in error
        gui = subprocess.run([str(ROOT / "build/apps/arinn-shell/arinn-shell-test")],
                             env={**env, "QT_QPA_PLATFORM": "offscreen", "QT_QUICK_BACKEND": "software"},
                             capture_output=True, text=True, timeout=20)
        assert gui.returncode == 0, gui.stdout + gui.stderr
        # A second shell must say why it exits instead of failing silently (#75).
        shell_env = {**env, "QT_QPA_PLATFORM": "offscreen", "QT_QUICK_BACKEND": "software"}
        first_shell = subprocess.Popen([str(ROOT / "build/apps/arinn-shell/arinn-shell")], env=shell_env,
                                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        processes.append(first_shell)
        wait(lambda: subprocess.run(["busctl", "--user", "status", "org.arinn.Shell1"], env=env,
                                    capture_output=True, timeout=3).returncode == 0)
        second_shell = subprocess.run([str(ROOT / "build/apps/arinn-shell/arinn-shell")], env=shell_env,
                                      capture_output=True, text=True, timeout=20)
        assert second_shell.returncode == 2, second_shell.stderr
        assert f"already owned by PID {first_shell.pid}" in second_shell.stderr, second_shell.stderr
        print("PASS: production registration, typed pages, operations, metadata reload, manual restart, daemon recovery, paging, manifest removal/reinstall, timeout and local Qt focus")
    except Exception:
        for log in logs:
            log.seek(0)
            print(log.read())
        raise
    finally:
        for child in reversed(processes):
            if child.poll() is None:
                child.send_signal(signal.SIGINT)
                try:
                    child.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    child.kill()
                    child.wait()
        for log in logs:
            log.close()

# A separate private bus prevents the desktop suite from inheriting this suite's
# daemon, provider owners or systemd fixture state.
subprocess.run(["dbus-run-session", "--", sys.executable, str(ROOT / "tests/integration/desktop.py")], check=True)
