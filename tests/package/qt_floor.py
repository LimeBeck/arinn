#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2026 Arinn contributors
#
# SPDX-License-Identifier: GPL-3.0-or-later

"""Check that packaged binaries run on the minimum supported Qt (#416, ADR-0103).

    python tests/package/qt_floor.py [--floor 6.8] [--qt-lib-dir DIR] ROOT

ROOT is an extracted package or any directory tree. Every ELF file under it:

- imports no Qt version tag above the floor (``qt_version_tag@Qt_6.12``) and
  no ``Qt_6_PRIVATE_API`` symbol;
- imports only Qt symbols that the floor Qt libraries in DIR export, when
  ``--qt-lib-dir`` is given;
- carries no RPATH or RUNPATH, so the build's Qt prefix never leaks into the
  package.

Needs ``readelf`` from binutils.
"""

import argparse
import os
import re
import subprocess
import sys
from pathlib import Path

# The minimum supported Qt (ADR-0103). CMakeLists.txt, packaging/arch/PKGBUILD
# and ci/images/arch/Dockerfile follow it; tests/ci/test_qt_floor.py keeps
# them in step.
FLOOR = "6.8"

SYMBOL = re.compile(
    r"^\s*\d+:\s+[0-9a-fA-F]+\s+\S+\s+\S+\s+\S+\s+\S+\s+(?P<ndx>\S+)\s+(?P<name>[^\s@]+)"
    r"(?:@{1,2}(?P<version>[^\s(]+))?")
TAG = re.compile(r"^Qt_(\d+)\.(\d+)$")


def is_elf(path: Path) -> bool:
    if path.is_symlink() or not path.is_file():
        return False
    with path.open("rb") as file:
        return file.read(4) == b"\x7fELF"


def readelf(path: Path, *options: str) -> str:
    result = subprocess.run(["readelf", "-W", *options, str(path)],
                            capture_output=True, text=True, check=True)
    return result.stdout


def parse_symbols(text: str) -> list[tuple[str, str | None, bool]]:
    """(name, version, defined) for each dynamic symbol in readelf output."""
    symbols = []
    for line in text.splitlines():
        match = SYMBOL.match(line)
        if match and match["name"]:
            symbols.append((match["name"], match["version"], match["ndx"] != "UND"))
    return symbols


def parse_paths(text: str) -> list[str]:
    """RPATH and RUNPATH entries in ``readelf -d`` output."""
    return re.findall(r"\((?:RPATH|RUNPATH)\)\s+Library (?:rpath|runpath): \[([^\]]*)\]", text)


def is_qt_version(version: str | None) -> bool:
    return bool(version) and (version.startswith("Qt_") or version == "NonQt")


def tag_problem(version: str, floor: tuple[int, int]) -> str | None:
    if version == "Qt_6_PRIVATE_API":
        return "private Qt API"
    match = TAG.match(version)
    if match and (int(match[1]), int(match[2])) > floor:
        return f"Qt version tag above the floor Qt_{floor[0]}.{floor[1]}"
    return None


def floor_exports(directory: Path) -> set[tuple[str, str]]:
    exports = set()
    seen = set()
    for path in sorted(directory.glob("libQt6*.so*")):
        real = path.resolve()
        if real in seen or not is_elf(real):
            continue
        seen.add(real)
        for name, version, defined in parse_symbols(readelf(real, "--dyn-syms")):
            if defined and is_qt_version(version):
                exports.add((name, version))
    if not exports:
        raise SystemExit(f"qt_floor: no Qt libraries in {directory}")
    return exports


def check(root: Path, floor: tuple[int, int], exports: set[tuple[str, str]] | None) -> tuple[list[str], int, int]:
    """Problems found, ELF files checked and Qt symbols imported."""
    problems = []
    files = imports = 0
    for directory, _, names in os.walk(root):
        for path in sorted(Path(directory) / name for name in names):
            if not is_elf(path):
                continue
            files += 1
            relative = path.relative_to(root)
            for entry in parse_paths(readelf(path, "-d")):
                problems.append(f"{relative}: RPATH/RUNPATH {entry}")
            for name, version, defined in parse_symbols(readelf(path, "--dyn-syms")):
                if defined or not is_qt_version(version):
                    continue
                imports += 1
                problem = tag_problem(version, floor)
                if problem:
                    problems.append(f"{relative}: {name}@{version}: {problem}")
                elif exports is not None and (name, version) not in exports:
                    problems.append(f"{relative}: {name}@{version}: not exported by the floor Qt libraries")
    return problems, files, imports


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--floor", default=FLOOR, help=f"minimum supported Qt, default {FLOOR}")
    parser.add_argument("--qt-lib-dir", type=Path, help="lib directory of the floor Qt")
    parser.add_argument("root", type=Path)
    args = parser.parse_args(argv)
    major, minor = (int(part) for part in args.floor.split("."))
    exports = floor_exports(args.qt_lib_dir) if args.qt_lib_dir else None
    problems, files, imports = check(args.root, (major, minor), exports)
    for problem in problems:
        print(f"FAIL: {problem}", file=sys.stderr)
    if not files:
        print(f"FAIL: no ELF files under {args.root}", file=sys.stderr)
        return 1
    if problems:
        return 1
    against = f"; all exported by {args.qt_lib_dir}" if exports is not None else ""
    print(f"PASS: {files} ELF files, {imports} Qt symbol imports, none above Qt {args.floor} "
          f"or private{against}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
