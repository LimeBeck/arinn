#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2026 Arinn contributors
#
# SPDX-License-Identifier: GPL-3.0-or-later

"""Release helpers used by CI (ADR-0033).

    python scripts/release.py version          print the workspace version
    python scripts/release.py check [TAG]      verify version consistency (and the tag, if given)
    python scripts/release.py notes VERSION    print the CHANGELOG section of VERSION
"""

import re
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def version() -> str:
    with open(ROOT / "Cargo.toml", "rb") as file:
        return tomllib.load(file)["workspace"]["package"]["version"]


def changelog_section(ver: str) -> str | None:
    text = (ROOT / "CHANGELOG.md").read_text(encoding="utf-8")
    match = re.search(rf"^## \[{re.escape(ver)}\][^\n]*\n(.*?)(?=^## |\Z)", text, re.MULTILINE | re.DOTALL)
    return match.group(1).strip() if match else None


def check(tag: str | None) -> list[str]:
    ver = version()
    problems = []
    if not re.fullmatch(r"\d+\.\d+\.\d+", ver):
        problems.append(f"workspace version {ver!r} is not MAJOR.MINOR.PATCH")
    pkgbuild = (ROOT / "packaging/arch/PKGBUILD").read_text(encoding="utf-8")
    pkgver = re.search(r"^pkgver=(.+)$", pkgbuild, re.MULTILINE)
    if not pkgver or pkgver.group(1) != ver:
        problems.append(f"packaging/arch/PKGBUILD pkgver must equal {ver}")
    if tag is not None:
        if tag != f"v{ver}":
            problems.append(f"tag {tag} does not match workspace version v{ver}")
        if not changelog_section(ver):
            problems.append(f"CHANGELOG.md has no non-empty '## [{ver}]' section")
    return problems


def main(argv: list[str]) -> int:
    if argv[:1] == ["version"]:
        print(version())
        return 0
    if argv[:1] == ["check"] and len(argv) <= 2:
        problems = check(argv[1] if len(argv) == 2 else None)
        for problem in problems:
            print(f"error: {problem}")
        if not problems:
            print(f"Version {version()} is consistent.")
        return 1 if problems else 0
    if argv[:1] == ["notes"] and len(argv) == 2:
        section = changelog_section(argv[1])
        if section is None:
            print(f"error: no CHANGELOG section for {argv[1]}", file=sys.stderr)
            return 1
        print(section)
        return 0
    print(__doc__, file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
