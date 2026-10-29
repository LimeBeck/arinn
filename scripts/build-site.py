#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Arinn contributors
# SPDX-License-Identifier: GPL-3.0-or-later
"""Assemble the product website and handbook from an explicit publish allowlist."""

import argparse
import shutil
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SCREENS = (
    "home", "open-services", "settings", "library-folders",
)
PUBLISHED_FILES = frozenset({
    "index.html", "assets/site.css", "assets/site.js", "assets/guide.css",
    "assets/arinn-mark.svg", "guide/index.html", "guide/assets/site.css",
    "guide/assets/home.png", "guide/assets/arinn-mark.svg", "guide/assets/arinn-icon.svg",
    *(f"assets/screenshots/{name}.png" for name in SCREENS),
})


def build(output: Path) -> None:
    output = output.resolve()
    # Never write generated files over source or repository metadata.
    if output == ROOT or output in ROOT.parents or (
        output.is_relative_to(ROOT) and not output.is_relative_to(ROOT / "build")
    ):
        raise ValueError("Use build/site or a directory outside the repository")
    if output.exists():
        unexpected = {
            path.relative_to(output).as_posix()
            for path in output.rglob("*") if path.is_file() or path.is_symlink()
        } - PUBLISHED_FILES
        if unexpected:
            raise ValueError(f"Output contains unexpected files: {sorted(unexpected)}")
    sources = {
        "index.html": ROOT / "docs/site/index.html",
        **{f"assets/{name}": ROOT / f"docs/site/assets/{name}"
           for name in ("site.css", "site.js", "guide.css")},
        "assets/arinn-mark.svg": ROOT / "docs/design/handoff/assets/arinn-mark.svg",
        "guide/assets/site.css": ROOT / "docs/user/assets/site.css",
        "guide/assets/arinn-mark.svg": ROOT / "docs/user/assets/arinn-mark.svg",
        "guide/assets/arinn-icon.svg": ROOT / "docs/user/assets/arinn-icon.svg",
        "guide/assets/home.png": ROOT / "docs/design/showcase/home.png",
        **{f"assets/screenshots/{name}.png": ROOT / f"docs/design/showcase/{name}.png"
           for name in SCREENS},
    }
    # Validate all inputs before writing. The handbook is the #335 dependency.
    guide = (ROOT / "docs/user/index.html").read_text(encoding="utf-8")
    for source in sources.values():
        if not source.is_file():
            raise FileNotFoundError(source)
    for name in PUBLISHED_FILES:
        target = output / name
        if target.is_symlink() or not target.resolve().is_relative_to(output):
            raise ValueError(f"Output path escapes the site: {name}")
    for name, source in sources.items():
        target = output / name
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, target)
    # Link the assembled guide back to the product site without editing its source.
    guide = guide.replace('class="brand" href="#overview"',
                          'class="brand" href="../index.html"')
    guide = guide.replace('aria-label="Arinn user guide"',
                          'aria-label="Arinn home"')
    guide = guide.replace('</head>',
                          '  <link rel="stylesheet" href="../assets/guide.css">\n</head>')
    (output / "guide/index.html").write_text(guide, encoding="utf-8")
    print(f"Built {len(PUBLISHED_FILES)} public files in {output}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=ROOT / "build/site")
    build(parser.parse_args().output)
