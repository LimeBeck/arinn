#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2026 Arinn contributors
#
# SPDX-License-Identifier: GPL-3.0-or-later

"""Decide which CI jobs a run needs (ADR-0044, ADR-0049).

    python scripts/ci-scope.py            classify the current CI event
    python scripts/ci-scope.py PATH...    classify the given changed paths

Only a pull request whose changes are all documentation is docs-only:
Markdown files anywhere and changelog fragments under changes/. A pull
request builds the Arch package only when it changes what the package
installs or how it is built (packaging, manifests, workspace members, the
KWin adapter, the browser extension, package tests). Every other event
(push, nightly, release, manual) and any doubt means a full run.
Writes docs_only=true|false and package=true|false to $GITHUB_OUTPUT.
"""

import os
import subprocess
import sys


def is_doc(path: str) -> bool:
    # Non-Markdown files under docs/ are inputs of the build (interface XML,
    # design assets), so they never count as documentation here.
    return path.endswith(".md") or path.startswith("changes/")


def docs_only(paths: list[str]) -> bool:
    return bool(paths) and all(is_doc(path) for path in paths)


PACKAGE_PREFIXES = (
    "packaging/",
    "tests/package/",
    "plugins/chrome/extension/",
    ".github/workflows/package.yml",
    "Cargo.lock",
    "CMakeLists.txt",
    "platform/kde/arinn-kwin-adapter",
    "platform/kde/phase0.js",
)


def affects_package(path: str) -> bool:
    return (
        path.startswith(PACKAGE_PREFIXES)
        or path.endswith("Cargo.toml")
        or (path.startswith("plugins/") and path.endswith(".toml"))
    )


def package(paths: list[str] | None) -> bool:
    # Unknown paths (not a pull request, or git failed) mean a full run.
    return paths is None or any(affects_package(path) for path in paths)


def pull_request_paths() -> list[str] | None:
    # actions/checkout checks out the synthetic merge commit of a pull
    # request; its first parent is the base branch, so the diff between them
    # is exactly what the pull request changes.
    result = subprocess.run(
        ["git", "diff", "--name-only", "--no-renames", "HEAD^1", "HEAD"],
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        print(result.stderr, file=sys.stderr)
        return None
    return [line for line in result.stdout.splitlines() if line]


def main() -> int:
    if len(sys.argv) > 1:
        paths = sys.argv[1:]
    elif os.environ.get("GITHUB_EVENT_NAME") == "pull_request":
        paths = pull_request_paths()
    else:
        paths = None
    result = docs_only(paths or [])
    build = not result and package(paths)
    lines = [f"docs_only={'true' if result else 'false'}", f"package={'true' if build else 'false'}"]
    print("\n".join(lines))
    output = os.environ.get("GITHUB_OUTPUT")
    if output:
        with open(output, "a", encoding="utf-8") as handle:
            handle.write("\n".join(lines) + "\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
