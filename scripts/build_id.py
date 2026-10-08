#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2026 Arinn contributors
#
# SPDX-License-Identifier: GPL-3.0-or-later

"""Print the build id of this source tree (ADR-0097).

    python scripts/build_id.py            print the build id
    python scripts/build_id.py --watch    print the files a build should watch
    python scripts/build_id.py --pkgver   print the snapshot package version

The build id is `<version>+g<12 hex>` with `-dirty` for a modified checkout,
or the bare workspace version when the commit is unknown. The commit comes
from, in order:

1. `ARINN_BUILD_ID`, when a build sets it explicitly;
2. `scripts/build-commit`, which `git archive` fills in (`export-subst`);
3. `git` in a checkout.

Cargo build scripts and CMake both call this script, so every binary of one
build reports the same id.
"""

import os
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
import release  # noqa: E402

ARCHIVED = ROOT / "scripts" / "build-commit"
FORMAT = re.compile(r"^(?P<version>\d+\.\d+\.\d+)(\+g[0-9a-f]{12}(-dirty)?)?$")


def valid(build_id: str, version: str) -> bool:
    match = FORMAT.match(build_id)
    return bool(match) and match.group("version") == version


def compose(version: str, commit: str | None, dirty: bool) -> str:
    if not commit:
        return version
    return f"{version}+g{commit[:12]}" + ("-dirty" if dirty else "")


def archived_commit() -> str | None:
    try:
        text = ARCHIVED.read_text(encoding="utf-8")
    except OSError:
        return None
    # Unsubstituted in a checkout: the placeholder is not a commit.
    found = re.findall(r"^([0-9a-f]{40})$", text, re.MULTILINE)
    return found[0] if found else None


def git(*args: str) -> str | None:
    try:
        result = subprocess.run(
            ["git", "-C", str(ROOT), *args],
            capture_output=True, text=True, timeout=10, check=False,
        )
    except (OSError, subprocess.SubprocessError):
        return None
    return result.stdout.strip() if result.returncode == 0 else None


def build_id() -> str:
    version = release.version()
    explicit = os.environ.get("ARINN_BUILD_ID", "").strip()
    if explicit:
        if not valid(explicit, version):
            raise SystemExit(f"ARINN_BUILD_ID {explicit!r} is not {version}[+g<12 hex>[-dirty]]")
        return explicit
    commit = archived_commit()
    if commit:
        return compose(version, commit, False)
    head = git("rev-parse", "--verify", "HEAD")
    if head and re.fullmatch(r"[0-9a-f]{40}", head):
        # Tracked changes only: build output and untracked notes are not a
        # different build.
        status = git("--no-optional-locks", "status", "--porcelain", "--untracked-files=no")
        return compose(version, head, bool(status))
    return version


def watched() -> list[Path]:
    """Files whose change can change the build id."""
    paths = [ROOT / "Cargo.toml", ARCHIVED]
    git_dir = git("rev-parse", "--absolute-git-dir")
    if git_dir:
        base = Path(git_dir)
        paths += [base / "HEAD", base / "index", base / "packed-refs"]
        head = git("symbolic-ref", "-q", "HEAD")
        if head:
            paths.append(base / head)
    return [p for p in paths if p.exists()]


def snapshot_pkgver() -> str:
    """`<version>.r<commits>.g<12 hex>` for a master snapshot package (#357).

    pacman orders it after `<version>` and before the next release, and
    orders snapshots by their commit count, so every snapshot upgrades the
    one before it. The count needs the full history: a shallow clone fails.
    """
    if git("rev-parse", "--is-shallow-repository") != "false":
        raise SystemExit("--pkgver needs a full clone (fetch-depth: 0)")
    head = git("rev-parse", "--verify", "HEAD")
    count = git("rev-list", "--count", "HEAD")
    if not head or not re.fullmatch(r"[0-9a-f]{40}", head) or not count or not count.isdigit():
        raise SystemExit("--pkgver needs a git checkout")
    return f"{release.version()}.r{count}.g{head[:12]}"


def main(argv: list[str]) -> int:
    if argv == ["--pkgver"]:
        print(snapshot_pkgver())
        return 0
    if argv[:1] == ["--watch"]:
        for path in watched():
            print(path)
        return 0
    if argv:
        print(__doc__, file=sys.stderr)
        return 2
    print(build_id())
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
