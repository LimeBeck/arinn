#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2026 Arinn contributors
#
# SPDX-License-Identifier: GPL-3.0-or-later

"""Release and changelog helpers (ADR-0033, ADR-0034).

    python scripts/release.py version                 print the workspace version
    python scripts/release.py check [TAG]             verify version consistency (and the tag, if given)
    python scripts/release.py notes VERSION           print the CHANGELOG section of VERSION
    python scripts/release.py unreleased              print the pending changelog fragments
    python scripts/release.py collect VERSION [DATE]  move fragments into a CHANGELOG section
    python scripts/release.py check-fragment BASE     fail if this branch adds no changelog fragment

Unreleased changes live as one file per pull request in changes/<category>/,
so parallel branches never edit the same lines of CHANGELOG.md.
"""

import re
import subprocess
import sys
import tomllib
from datetime import date
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CHANGES = ROOT / "changes"
# Keep a Changelog categories, in the order they appear in a release section.
CATEGORIES = ["added", "changed", "deprecated", "removed", "fixed", "security"]


def version() -> str:
    with open(ROOT / "Cargo.toml", "rb") as file:
        return tomllib.load(file)["workspace"]["package"]["version"]


def changelog_section(ver: str) -> str | None:
    text = (ROOT / "CHANGELOG.md").read_text(encoding="utf-8")
    match = re.search(rf"^## \[{re.escape(ver)}\][^\n]*\n(.*?)(?=^## |\Z)", text, re.MULTILINE | re.DOTALL)
    return match.group(1).strip() if match else None


def fragments() -> dict[str, list[Path]]:
    found = {}
    for category in CATEGORIES:
        files = sorted(p for p in (CHANGES / category).glob("*.md")) if (CHANGES / category).is_dir() else []
        if files:
            found[category] = files
    return found


def fragment_problems() -> list[str]:
    problems = []
    if CHANGES.is_dir():
        for path in CHANGES.rglob("*.md"):
            if path.parent == CHANGES:
                continue  # changes/README.md
            if path.parent.parent != CHANGES or path.parent.name not in CATEGORIES:
                problems.append(f"{path.relative_to(ROOT)}: fragments go into changes/<{'|'.join(CATEGORIES)}>/")
            elif not re.search(r"^- \S", path.read_text(encoding="utf-8"), re.MULTILINE):
                problems.append(f"{path.relative_to(ROOT)}: a fragment must contain at least one '- ' bullet")
    return problems


def render_fragments() -> str:
    parts = []
    for category, files in fragments().items():
        bullets = "\n".join(f.read_text(encoding="utf-8").strip() for f in files)
        parts.append(f"### {category.capitalize()}\n\n{bullets}")
    return "\n\n".join(parts)


def collect(ver: str, day: str) -> list[Path]:
    body = render_fragments()
    if not body:
        raise SystemExit("error: no changelog fragments to collect")
    path = ROOT / "CHANGELOG.md"
    text = path.read_text(encoding="utf-8")
    marker = "## [Unreleased]\n"
    if marker not in text:
        raise SystemExit("error: CHANGELOG.md has no '## [Unreleased]' heading")
    head, rest = text.split(marker, 1)
    manual, _, tail = rest.partition("\n## ")
    if manual.strip():
        raise SystemExit("error: move the entries under '## [Unreleased]' into changes/ fragments first")
    section = f"## [{ver}] - {day}\n\n{body}"
    text = f"{head}{marker}\n{section}\n\n## {tail}" if tail else f"{head}{marker}\n{section}\n"
    path.write_text(text, encoding="utf-8")
    collected = [f for files in fragments().values() for f in files]
    for f in collected:
        f.unlink()
    return collected


def check_fragment(base: str) -> list[str]:
    diff = subprocess.run(["git", "diff", "--name-only", "--diff-filter=A", f"{base}...HEAD"],
                          cwd=ROOT, capture_output=True, text=True, check=True).stdout.split()
    added = [f for f in diff if re.fullmatch(rf"changes/({'|'.join(CATEGORIES)})/[^/]+\.md", f)]
    if added:
        return []
    return ["no changelog fragment added under changes/<category>/ "
            "(add one, or label the pull request 'skip-changelog' for changes users never notice)"]


def check(tag: str | None) -> list[str]:
    ver = version()
    problems = fragment_problems()
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
            problems.append(f"CHANGELOG.md has no non-empty '## [{ver}]' section (run: release.py collect {ver})")
        if fragments():
            problems.append("uncollected changelog fragments remain in changes/")
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
    if argv == ["unreleased"]:
        print(render_fragments() or "(no pending fragments)")
        return 0
    if argv[:1] == ["collect"] and len(argv) in (2, 3):
        for f in collect(argv[1], argv[2] if len(argv) == 3 else date.today().isoformat()):
            print(f"collected {f.relative_to(ROOT)}")
        return 0
    if argv[:1] == ["check-fragment"] and len(argv) == 2:
        problems = check_fragment(argv[1])
        for problem in problems:
            print(f"error: {problem}")
        return 1 if problems else 0
    print(__doc__, file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
