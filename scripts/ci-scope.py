#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2026 Arinn contributors
#
# SPDX-License-Identifier: GPL-3.0-or-later

"""Decide whether a CI run may skip the build jobs (ADR-0044).

    python scripts/ci-scope.py            classify the current CI event
    python scripts/ci-scope.py PATH...    classify the given changed paths

Only a pull request whose changes are all documentation is docs-only:
Markdown files anywhere and changelog fragments under changes/. Every other
event (push, nightly, release, manual) and any doubt means a full run.
Writes docs_only=true|false to $GITHUB_OUTPUT when it is set.
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
    print(f"docs_only={'true' if result else 'false'}")
    output = os.environ.get("GITHUB_OUTPUT")
    if output:
        with open(output, "a", encoding="utf-8") as handle:
            handle.write(f"docs_only={'true' if result else 'false'}\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
