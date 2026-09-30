#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2026 Arinn contributors
#
# SPDX-License-Identifier: GPL-3.0-or-later

"""Remove the installed Arch package and fail if anything is left behind (G-5.3).

The package job installs Arinn, starts it (`smoke.py`: the daemon and
providers register) and checks the installed units and manifests
(`session.py`). This script is the last step:

    python tests/package/uninstall.py --self-check
    python tests/package/uninstall.py

The self-check plants a systemd unit and a manifest, and requires the
check to fail. The real run records `pacman -Qlq arinn`, removes the
package and fails if any packaged file, session unit, enablement link or
manifest is still there. It removes the real `arinn` package; the
self-check does not.
"""

import os
import shlex
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

# Session units shipped by the package (ADR-0042). Enablement links use the
# same names; a unit added to the package file list is included as well.
UNIT_NAMES = {
    "arinn-session.target",
    "arinnd.service",
    "arinn-shell.service",
    "arinn-kwin.service",
}

SYSTEMD_DIRS = (
    "usr/lib/systemd/user",
    "etc/systemd/user",
    "usr/local/lib/systemd/user",
)

# Trees that exist only for this package. A file left in one of them is a
# leftover even when pacman does not list it.
PRIVATE_DIRS = (
    "usr/lib/arinn",
    "usr/share/arinn",
    "usr/share/doc/arinn",
    "usr/share/licenses/arinn",
)

HOST_MANIFESTS = (
    "etc/opt/chrome/native-messaging-hosts/org.arinn.chrome_host.json",
    "etc/chromium/native-messaging-hosts/org.arinn.chrome_host.json",
)

MANIFEST_DIRS = (
    "etc/opt/chrome/native-messaging-hosts",
    "etc/chromium/native-messaging-hosts",
)

# Representative package contents for the self-check. The real run uses
# `pacman -Qlq` instead of this list.
SAMPLE_FILES = [
    "usr/bin/arinnd",
    "usr/bin/arinnctl",
    "usr/bin/arinn-shell",
    "usr/lib/arinn/plugins/arinn-plugin-demo",
    "usr/lib/arinn/arinn-kwin-adapter",
    "usr/lib/arinn/arinn-chrome-host",
    "usr/lib/systemd/user/arinn-session.target",
    "usr/lib/systemd/user/arinnd.service",
    "usr/lib/systemd/user/arinn-shell.service",
    "usr/lib/systemd/user/arinn-kwin.service",
    "usr/share/arinn/plugins/demo.toml",
    "usr/share/arinn/plugins/mpris.toml",
    "usr/share/arinn/kwin/phase0.js",
    "usr/share/arinn/chrome-extension/manifest.json",
    "etc/opt/chrome/native-messaging-hosts/org.arinn.chrome_host.json",
    "etc/chromium/native-messaging-hosts/org.arinn.chrome_host.json",
]


def package_files(listing: str) -> list[str]:
    """Files from `pacman -Qlq` output. Directory lines keep their trailing slash and are skipped."""
    files = []
    for line in listing.splitlines():
        line = line.strip()
        if not line.startswith("/") or line.endswith("/"):
            continue
        relative = line.lstrip("/")
        if ".." in Path(relative).parts:
            continue
        files.append(relative)
    return files


def unit_names(owned_files: list[str]) -> set[str]:
    names = set(UNIT_NAMES)
    for relative in owned_files:
        path = Path(relative)
        if "systemd" in path.parts and path.suffix in {".service", ".target", ".socket", ".timer"}:
            names.add(path.name)
    return names


def exists(root: Path, relative: str) -> bool:
    path = root / relative
    return path.is_symlink() or path.exists()


def systemd_leftovers(root: Path, names: set[str]) -> list[str]:
    found = []
    for base_rel in SYSTEMD_DIRS:
        base = root / base_rel
        if not base.is_dir():
            continue
        for path in base.rglob("*"):
            if path.name not in names:
                continue
            if path.is_symlink() or path.is_file():
                found.append(path.relative_to(root).as_posix())
    return found


def manifest_leftovers(root: Path) -> list[str]:
    found = []
    for relative in HOST_MANIFESTS:
        if exists(root, relative):
            found.append(relative)
    base = root / "usr/share/arinn"
    if base.is_dir():
        for path in base.rglob("*"):
            if not (path.is_symlink() or path.is_file()):
                continue
            if path.suffix == ".toml" or path.name == "manifest.json":
                found.append(path.relative_to(root).as_posix())
    for relative in MANIFEST_DIRS:
        path = root / relative
        if path.is_dir() and not path.is_symlink() and not any(path.iterdir()):
            found.append(relative)
    return found


def private_leftovers(root: Path) -> list[str]:
    found = []
    for relative in PRIVATE_DIRS:
        path = root / relative
        if path.is_symlink():
            found.append(relative)
            continue
        if not path.exists():
            continue
        found.append(relative)
        if path.is_dir():
            for child in path.rglob("*"):
                if child.is_symlink() or child.is_file():
                    found.append(child.relative_to(root).as_posix())
    return found


def find_leftovers(root: Path, owned_files: list[str]) -> list[str]:
    found = set()
    for relative in owned_files:
        path = root / relative
        # A directory listed without a trailing slash (shared locations such
        # as usr/bin) may remain. Only files and links are leftovers.
        if path.is_symlink() or (path.exists() and not path.is_dir()):
            found.add(relative)
    found.update(systemd_leftovers(root, unit_names(owned_files)))
    found.update(manifest_leftovers(root))
    found.update(private_leftovers(root))
    return sorted(found)


def format_leftovers(leftovers: list[str]) -> str:
    lines = "\n".join(f"  {item}" for item in leftovers)
    return f"leftovers after removing arinn:\n{lines}\n"


def run_pacman(args: list[str]) -> subprocess.CompletedProcess[str]:
    try:
        result = subprocess.run(["pacman", *args], capture_output=True, text=True, timeout=180)
    except FileNotFoundError as error:
        raise SystemExit("pacman is not available") from error
    if args[:1] != ["-Qq"] and result.returncode != 0:
        raise SystemExit(f"pacman {' '.join(args)} failed:\n{result.stderr}")
    return result


def uninstall_installed_package(root: Path) -> list[str]:
    query = run_pacman(["-Qlq", "arinn"])
    owned = package_files(query.stdout)
    if not owned:
        raise SystemExit("pacman listed no files for arinn")
    run_pacman(["-R", "--noconfirm", "arinn"])
    still = run_pacman(["-Qq", "arinn"])
    if still.returncode == 0:
        raise SystemExit("arinn is still installed after pacman -R")
    return find_leftovers(root, owned)


def purge_installed_tree(root: Path) -> None:
    """What a successful `pacman -R` does to private trees and empty manifest directories."""
    for relative in PRIVATE_DIRS:
        path = root / relative
        if path.is_symlink():
            path.unlink()
        elif path.is_dir():
            shutil.rmtree(path)
    for relative in MANIFEST_DIRS:
        path = root / relative
        if path.is_dir() and not path.is_symlink() and not any(path.iterdir()):
            path.rmdir()
        parent = (root / relative).parent
        while parent != root and parent.is_dir() and not any(parent.iterdir()) and parent != root / "etc":
            parent.rmdir()
            parent = parent.parent


def fake_pacman(argv: list[str]) -> int:
    args = argv[argv.index("--fake-pacman") + 1 :]
    action = args[0]
    root = Path(os.environ["FAKE_ROOT"])
    listing = Path(os.environ["FAKE_FILES"]).read_text(encoding="utf-8")
    keep = os.environ.get("FAKE_KEEP", "")
    if action == "-Qlq":
        sys.stdout.write(listing if listing.endswith("\n") else listing + "\n")
        return 0
    if action == "-Qq":
        return 1
    if action == "-R":
        for relative in package_files(listing):
            if relative == keep:
                continue
            path = root / relative
            if path.is_symlink() or path.is_file():
                path.unlink()
        if not keep and os.environ.get("FAKE_PURGE", "1") == "1":
            purge_installed_tree(root)
        return 0
    print(f"unexpected pacman arguments: {args}", file=sys.stderr)
    return 9


def write_listing(directory: Path, owned: list[str]) -> Path:
    lines = []
    seen = set()
    for relative in owned:
        parts = Path(relative).parts
        for index in range(1, len(parts)):
            directory_line = "/" + "/".join(parts[:index]) + "/"
            if directory_line not in seen:
                seen.add(directory_line)
                lines.append(directory_line)
        lines.append("/" + relative)
    path = directory / "files"
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")
    return path


def plant(root: Path, relative: str, text: str = "packaged\n") -> None:
    path = root / relative
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8")


def run_scenario(owned: list[str], keep: str = "", purge: str = "1", extras: tuple[str, ...] = (),
                 symlink: tuple[str, str] | None = None) -> tuple[subprocess.CompletedProcess[str], Path]:
    folder = Path(tempfile.mkdtemp(prefix="arinn-uninstall-"))
    root = folder / "root"
    root.mkdir()
    bin_dir = folder / "bin"
    bin_dir.mkdir()
    for relative in list(owned) + list(extras):
        plant(root, relative)
    if symlink is not None:
        link, target = symlink
        path = root / link
        path.parent.mkdir(parents=True, exist_ok=True)
        path.symlink_to(target)
    listing = write_listing(folder, owned)
    wrapper = bin_dir / "pacman"
    script = shlex.quote(str(Path(__file__).resolve()))
    interpreter = shlex.quote(sys.executable)
    wrapper.write_text(f"#!/bin/sh\nexec {interpreter} {script} --fake-pacman \"$@\"\n", encoding="utf-8")
    wrapper.chmod(0o755)
    env = {
        "PATH": str(bin_dir),
        "FAKE_ROOT": str(root),
        "FAKE_FILES": str(listing),
        "FAKE_KEEP": keep,
        "FAKE_PURGE": purge,
        "LANG": "C.UTF-8",
    }
    result = subprocess.run(
        [sys.executable, __file__, "--root", str(root)],
        env=env, capture_output=True, text=True, timeout=30, check=False)
    return result, root


def expect_fail(result: subprocess.CompletedProcess[str], needle: str) -> None:
    combined = result.stdout + result.stderr
    if result.returncode == 0 or needle not in combined:
        raise AssertionError(f"expected failure naming {needle!r}, got {result.returncode}:\n{combined}")


def expect_pass(result: subprocess.CompletedProcess[str]) -> None:
    if result.returncode != 0:
        raise AssertionError(f"expected a clean removal to pass:\n{result.stdout}{result.stderr}")


def self_check() -> None:
    # Clean removal leaves unrelated files alone, including another unit.
    result, root = run_scenario(
        SAMPLE_FILES,
        extras=("usr/bin/other-tool", "usr/lib/systemd/user/dbus.service", "usr/share/doc/other/README"),
        symlink=("usr/lib/systemd/user/graphical-session.target.wants/dbus.service", "../dbus.service"))
    try:
        expect_pass(result)
        for relative in ("usr/bin/other-tool", "usr/lib/systemd/user/dbus.service"):
            if not (root / relative).is_file():
                raise AssertionError(f"unrelated file was removed: {relative}")
        if not (root / "usr/lib/systemd/user/graphical-session.target.wants/dbus.service").is_symlink():
            raise AssertionError("unrelated enablement link was removed")
    finally:
        shutil.rmtree(root.parent)

    # A packaged unit left on disk fails the check.
    unit = "usr/lib/systemd/user/arinnd.service"
    result, root = run_scenario(SAMPLE_FILES, keep=unit)
    try:
        expect_fail(result, unit)
    finally:
        shutil.rmtree(root.parent)

    # A packaged manifest left on disk fails the check.
    manifest = "usr/share/arinn/plugins/demo.toml"
    result, root = run_scenario(SAMPLE_FILES, keep=manifest)
    try:
        expect_fail(result, manifest)
    finally:
        shutil.rmtree(root.parent)

    # A session unit that pacman does not list is still a leftover.
    hidden_unit = "usr/lib/systemd/user/arinn-kwin.service"
    owned = [item for item in SAMPLE_FILES if item != hidden_unit]
    result, root = run_scenario(owned, extras=(hidden_unit,))
    try:
        expect_fail(result, hidden_unit)
    finally:
        shutil.rmtree(root.parent)

    # A native-messaging manifest that pacman does not list is still a leftover.
    hidden_manifest = "etc/opt/chrome/native-messaging-hosts/org.arinn.chrome_host.json"
    owned = [item for item in SAMPLE_FILES if item != hidden_manifest]
    result, root = run_scenario(owned, extras=(hidden_manifest,))
    try:
        expect_fail(result, hidden_manifest)
    finally:
        shutil.rmtree(root.parent)

    # An enablement link is not in the package file list.
    wants = "usr/lib/systemd/user/graphical-session.target.wants/arinn-session.target"
    result, root = run_scenario(SAMPLE_FILES, symlink=(wants, "../arinn-session.target"))
    try:
        expect_fail(result, wants)
    finally:
        shutil.rmtree(root.parent)

    # A file dropped into a package-private directory survives a file-list removal.
    stray = "usr/lib/arinn/plugins/stray"
    result, root = run_scenario(SAMPLE_FILES, purge="0", extras=(stray,))
    try:
        expect_fail(result, stray)
    finally:
        shutil.rmtree(root.parent)

    # An empty native-messaging directory left behind fails the check.
    manifest_dir = "etc/opt/chrome/native-messaging-hosts"
    owned = ["etc/opt/chrome/native-messaging-hosts/org.arinn.chrome_host.json"]
    result, root = run_scenario(owned, purge="0")
    try:
        expect_fail(result, manifest_dir)
    finally:
        shutil.rmtree(root.parent)

    print("PASS: a deliberate leftover fails the uninstall check")


def main(argv: list[str]) -> int:
    if "--fake-pacman" in argv:
        return fake_pacman(argv)
    if "--self-check" in argv:
        self_check()
        return 0
    root = Path("/")
    if "--root" in argv:
        root = Path(argv[argv.index("--root") + 1])
    leftovers = uninstall_installed_package(root)
    if leftovers:
        print(format_leftovers(leftovers), end="", file=sys.stderr)
        return 1
    print("PASS: package removed; no systemd units, manifests or other packaged files remain")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
