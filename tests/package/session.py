#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2026 Arinn contributors
#
# SPDX-License-Identifier: GPL-3.0-or-later

"""Check the packaged session units and the KWin adapter loader (ADR-0042).

    python tests/package/session.py

Runs against the installed package (ARINN_PREFIX, default /usr). Never talks
to a real systemd user manager or KWin: units are verified offline and the
loader runs against a fake busctl.
"""

import fnmatch
import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

PREFIX = Path(os.environ.get("ARINN_PREFIX", "/usr"))
UNITS = PREFIX / "lib/systemd/user"
ADAPTER = PREFIX / "lib/arinn/arinn-kwin-adapter"
SCRIPT = PREFIX / "share/arinn/kwin/phase0.js"
NAMES = ["arinn-session.target", "arinnd.service", "arinn-shell.service", "arinn-kwin.service"]

ALL_NAMES = NAMES + ["arinn-ewmh.service", "arinn-ewmh-session.target"]

for name in ALL_NAMES:
    assert (UNITS / name).is_file(), f"missing unit {name}"
assert ADAPTER.is_file() and os.access(ADAPTER, os.X_OK), "missing KWin adapter loader"
assert SCRIPT.is_file(), "missing KWin script"
assert (PREFIX / "lib/arinn/arinn-shell-launch").is_file(), "missing shell launcher"
XFCE = PREFIX / "lib/arinn/arinn-xfce-session"
assert XFCE.is_file() and os.access(XFCE, os.X_OK), "missing XFCE session helper"
template = PREFIX / "share/arinn/autostart/org.arinn.XfceSession.desktop"
assert template.is_file(), "missing XFCE autostart template"
xfce_entry = template.read_text()
assert "OnlyShowIn=XFCE;" in xfce_entry
assert "Exec=/usr/lib/arinn/arinn-xfce-session" in xfce_entry
for directory in (PREFIX / "../etc/xdg/autostart", PREFIX / "etc/xdg/autostart"):
    assert not (directory / template.name).exists(), "XFCE autostart enabled globally"
subprocess.run([sys.executable, str(Path(__file__).with_name("test_xfce_session.py"))],
               env={**os.environ, "ARINN_XFCE_SESSION": str(XFCE)}, check=True, timeout=60)
# Menu entry and icon (#289).
OPEN = PREFIX / "lib/arinn/arinn-open"
assert OPEN.is_file() and os.access(OPEN, os.X_OK), "missing arinn-open"
DESKTOP = PREFIX / "share/applications/org.arinn.Shell.desktop"
assert DESKTOP.is_file(), "missing desktop entry"
entry = DESKTOP.read_text()
assert "Exec=/usr/lib/arinn/arinn-open" in entry and "Icon=org.arinn.Shell" in entry, entry
assert (PREFIX / "share/icons/hicolor/scalable/apps/org.arinn.Shell.svg").is_file(), "missing icon"

# Running user services restart after an upgrade (#355).
RESTART = PREFIX / "lib/arinn/arinn-restart-user-services"
assert RESTART.is_file() and os.access(RESTART, os.X_OK), "missing arinn-restart-user-services"
subprocess.run([sys.executable, str(Path(__file__).with_name("test_restart_user_services.py"))],
               env={**os.environ, "ARINN_RESTART_SCRIPT": str(RESTART)}, check=True, timeout=60)
if shutil.which("pacman") and PREFIX == Path("/usr"):
    HOOK = PREFIX / "share/libalpm/hooks/arinn-restart-user-services.hook"
    assert HOOK.is_file(), "missing libalpm hook"
    targets = [line.split("=", 1)[1].strip() for line in HOOK.read_text().splitlines()
               if line.startswith("Target")]
    # Every installed long-running binary triggers the hook; libalpm matches
    # package-relative paths with fnmatch(3), where * also spans slashes.
    owned = subprocess.run(["pacman", "-Qlq", "arinn"], capture_output=True, text=True,
                           check=True, timeout=60).stdout.split()
    binaries = [path for path in owned
                if Path(path).is_file() and os.access(path, os.X_OK)
                and path.startswith(("/usr/bin/", "/usr/lib/arinn/")) and path != "/usr/bin/arinnctl"]
    assert "/usr/bin/arinnd" in binaries, binaries
    for path in binaries + ["/usr/share/arinn/kwin/phase0.js"]:
        assert any(fnmatch.fnmatchcase(path[1:], target) for target in targets), path
else:
    print("SKIP: the libalpm hook is checked only for the installed Arch package")

# The package installs units but never enables them for anyone.
for wants in UNITS.glob("*.wants"):
    assert not any(p.name in ALL_NAMES for p in wants.iterdir()), f"enabled by default: {wants}"


def unit(name):
    sections, section = {}, None
    for line in (UNITS / name).read_text().splitlines():
        line = line.strip()
        if line.startswith("[") and line.endswith("]"):
            section = sections.setdefault(line[1:-1], {})
        elif line and not line.startswith("#"):
            key, value = line.split("=", 1)
            section.setdefault(key, []).append(value)
    return sections


# Every service stops with the graphical session and with the Arinn target.
target = unit("arinn-session.target")
assert target["Install"]["WantedBy"] == ["graphical-session.target"]
assert set(target["Unit"]["Wants"][0].split()) == set(NAMES[1:])
for name in NAMES[1:]:
    part_of = unit(name)["Unit"]["PartOf"][0].split()
    expected = ["graphical-session.target", "arinn-session.target"]
    if name in ["arinnd.service", "arinn-shell.service"]:
        expected.append("arinn-ewmh-session.target")
    assert part_of == expected, name
assert (PREFIX / "bin/arinn-platform-x11").is_file(), "missing EWMH adapter"
ewmh = unit("arinn-ewmh-session.target")
assert set(ewmh["Unit"]["Wants"][0].split()) == {"graphical-session.target", "arinnd.service", "arinn-shell.service", "arinn-ewmh.service"}
# The graphical target refuses direct manual start; pull it as a dependency
# and order before services even on desktops without systemd session startup.
assert ewmh["Unit"]["After"] == ["graphical-session.target"]
assert ewmh["Unit"]["Requisite"] == ["graphical-session.target"]
assert set(ewmh["Unit"]["Conflicts"][0].split()) == {"arinn-session.target", "arinn-kwin.service"}
assert unit("arinn-ewmh.service")["Service"]["BusName"] == ["org.arinn.X11"]
assert unit("arinn-ewmh.service")["Unit"]["PartOf"] == ["graphical-session.target arinn-ewmh-session.target"]
# One daemon and one shell: readiness and uniqueness follow their bus names.
assert unit("arinnd.service")["Service"]["BusName"] == ["org.arinn.Daemon1"]
assert unit("arinn-shell.service")["Service"]["BusName"] == ["org.arinn.Shell1"]
assert "arinnd.service" in unit("arinn-shell.service")["Unit"]["After"][0].split()
# A shell that finds its name taken exits with 2; systemd must not restart it
# into the start limit (#75).
assert unit("arinn-shell.service")["Service"]["RestartPreventExitStatus"] == ["2"]

if PREFIX == Path("/usr"):
    # Offline verification; unrelated units of the host may still warn.
    with tempfile.TemporaryDirectory(prefix="sd-") as runtime:
        result = subprocess.run(
            ["systemd-analyze", "--user", "verify", *(str(UNITS / n) for n in ALL_NAMES)],
            env={**os.environ, "XDG_RUNTIME_DIR": runtime}, capture_output=True, text=True, timeout=60)
    problems = [line for line in (result.stdout + result.stderr).splitlines()
                if any(n in line for n in NAMES) or "arinn" in line]
    assert result.returncode == 0 and not problems, result.stdout + result.stderr
else:
    print(f"SKIP: systemd-analyze verify needs the /usr prefix, not {PREFIX}")

FAKE_BUSCTL = """#!/bin/sh
echo "$*" >> "$FAKE_LOG"
case "$*" in
*"isScriptLoaded s arinn-phase0"*) echo "b ${FAKE_PHASE0:-false}" ;;
*"isScriptLoaded s arinn"*) echo "b ${FAKE_LOADED:-false}" ;;
*loadScript*) echo "${FAKE_LOAD:-i 7}" ;;
*"org.kde.kwin.Script run"*) ;;
*unloadScript*) exit "${FAKE_UNLOAD_RC:-0}" ;;
*) exit 9 ;;
esac
"""

with tempfile.TemporaryDirectory(prefix="arinn-kwin-") as folder:
    root = Path(folder)
    (root / "busctl").write_text(FAKE_BUSCTL)
    (root / "busctl").chmod(0o755)
    log = root / "log"

    def adapter(action, **fake):
        log.write_text("")
        env = {**os.environ, "PATH": f"{root}:{os.environ['PATH']}", "FAKE_LOG": str(log),
               "ARINN_KWIN_SCRIPT": str(SCRIPT), **fake}
        result = subprocess.run([str(ADAPTER), action], env=env, capture_output=True, text=True, timeout=30)
        return result.returncode, log.read_text().splitlines()

    code, calls = adapter("start")
    assert code == 0, calls
    assert calls[-2] == (f"--user call org.kde.KWin /Scripting org.kde.kwin.Scripting "
                         f"loadScript ss {SCRIPT} arinn"), calls
    assert calls[-1] == "--user call org.kde.KWin /Scripting/Script7 org.kde.kwin.Script run", calls

    # Already loaded (unit restart): nothing is loaded twice.
    code, calls = adapter("start", FAKE_LOADED="true")
    assert code == 0 and not any("loadScript" in c for c in calls), calls

    # KWin refusal and the competing developer adapter both fail the unit.
    code, calls = adapter("start", FAKE_LOAD="i -1")
    assert code == 1 and not any("Script run" in c for c in calls), calls
    code, calls = adapter("start", FAKE_PHASE0="true")
    assert code == 1 and not any("loadScript" in c for c in calls), calls

    # Stopping never fails, even when KWin has already gone away.
    code, calls = adapter("stop")
    assert code == 0 and calls == ["--user call org.kde.KWin /Scripting org.kde.kwin.Scripting unloadScript s arinn"]
    assert adapter("stop", FAKE_UNLOAD_RC="1")[0] == 0
    assert adapter("bogus")[0] == 2

# Chrome integration (ADR-0048): one host manifest for both browsers, pointing
# at the installed host and allowing only the packaged extension.
EXTENSION = PREFIX / "share/arinn/chrome-extension"
HOST = PREFIX / "lib/arinn/arinn-chrome-host"
assert (PREFIX / "lib/arinn/plugins/arinn-plugin-chrome").is_file()
assert (PREFIX / "share/arinn/plugins/chrome.toml").is_file()
assert HOST.is_file() and os.access(HOST, os.X_OK), "missing Chrome native host"
manifest = json.loads((EXTENSION / "manifest.json").read_text())
assert sorted(manifest["permissions"]) == ["nativeMessaging", "tabs"], manifest["permissions"]
assert (EXTENSION / "background.js").is_file()
if PREFIX == Path("/usr"):
    for folder in ("/etc/opt/chrome", "/etc/chromium"):
        host_manifest = json.loads(Path(folder, "native-messaging-hosts/org.arinn.chrome_host.json").read_text())
        assert host_manifest["path"] == str(HOST), host_manifest["path"]
        assert host_manifest["type"] == "stdio"
        [origin] = host_manifest["allowed_origins"]
        assert origin.startswith("chrome-extension://") and origin.endswith("/"), origin
    # Any other extension is refused before the host touches the bus.
    refused = subprocess.run([str(HOST), "chrome-extension://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/"],
                             capture_output=True, timeout=10)
    assert refused.returncode == 2, refused
else:
    print(f"SKIP: native messaging manifests are checked only for the /usr prefix, not {PREFIX}")

print("PASS: session units are installed, disabled by default and valid; KWin adapter loader behaves; Chrome integration files are installed")
