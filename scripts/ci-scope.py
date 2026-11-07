#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2026 Arinn contributors
#
# SPDX-License-Identifier: GPL-3.0-or-later

"""Decide which CI jobs a run needs (ADR-0044, ADR-0102, proposed ADR-0145).

    python scripts/ci-scope.py            classify the current CI event
    python scripts/ci-scope.py PATH...    classify the given changed paths

Pull requests use one of four scopes: docs, website, visual or full. Only
explicit public-site and visual-fixture paths qualify for the two scoped
checks. Native build inputs, unknown/mixed paths and failed path discovery
use the full pipeline. Every non-PR event remains full, including packages
(ADR-0102). Command-line paths simulate a pull request.
Writes the scope and boolean job flags to $GITHUB_OUTPUT.
"""

import os
import subprocess
import sys
from pathlib import PurePosixPath


SITE_TOOLS = frozenset({
    "scripts/build-site.py", "scripts/check-site.py", "scripts/check-user-site.py",
    "tests/ci/test_site.py",
})
VISUAL_TOOLS = frozenset({"scripts/ui-compare.py", "tests/ci/test_ui_compare.py"})
SITE_EXTENSIONS = frozenset({".html", ".css", ".js", ".svg", ".png"})


def known_path(path: str) -> bool:
    return bool(path) and not path.startswith("/") and ".." not in PurePosixPath(path).parts


def is_doc(path: str) -> bool:
    return known_path(path) and path.endswith(".md")


def is_site(path: str) -> bool:
    if not known_path(path):
        return False
    suffix = PurePosixPath(path).suffix
    return path in SITE_TOOLS or (
        path.startswith(("docs/site/", "docs/user/")) and suffix in SITE_EXTENSIONS
    ) or (path.startswith("docs/design/showcase/") and suffix == ".png")


def is_visual(path: str) -> bool:
    return known_path(path) and (path in VISUAL_TOOLS or (
        path.startswith("tests/ui/visual/") and PurePosixPath(path).suffix in {".qml", ".json"}
    ))


def docs_only(paths: list[str]) -> bool:
    return bool(paths) and all(is_doc(path) for path in paths)


def scope(paths: list[str] | None, pull_request: bool) -> str:
    if not pull_request or not paths:
        return "full"
    if docs_only(paths):
        return "docs"
    if all(is_doc(path) or is_site(path) for path in paths):
        return "website"
    if all(is_doc(path) or is_site(path) or is_visual(path) for path in paths):
        return "visual"
    return "full"


def package(pull_request: bool) -> bool:
    # The packages build for master, the nightly, releases and manual runs;
    # a manual run on a branch checks a packaging change before it merges.
    return not pull_request


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
    # Paths given on the command line are classified as a pull request.
    pull_request = len(sys.argv) > 1 or os.environ.get("GITHUB_EVENT_NAME") == "pull_request"
    if len(sys.argv) > 1:
        paths = sys.argv[1:]
    elif pull_request:
        paths = pull_request_paths()
    else:
        paths = None
    selected = scope(paths, pull_request)
    unknown_diff = pull_request and not paths
    flags = {
        "docs_only": selected == "docs",
        "native": selected == "full",
        # Mixed native changes retain the relevant site/render checks too.
        "website": unknown_diff or (pull_request and any(is_site(path) or is_visual(path) for path in paths or [])),
        "visual": unknown_diff or (pull_request and any(is_visual(path) for path in paths or [])),
        "package": selected == "full" and package(pull_request),
    }
    lines = [f"scope={selected}", *(f"{key}={str(value).lower()}" for key, value in flags.items())]
    print("\n".join(lines))
    output = os.environ.get("GITHUB_OUTPUT")
    if output:
        with open(output, "a", encoding="utf-8") as handle:
            handle.write("\n".join(lines) + "\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
