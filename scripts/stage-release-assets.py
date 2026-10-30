#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Arinn contributors
# SPDX-License-Identifier: GPL-3.0-or-later

"""Copy distributable files from mixed CI artifacts into a fresh publication directory."""
import argparse
from pathlib import Path
import re
import shutil


PATTERNS = {
    "arch": re.compile(r"arinn-[0-9][A-Za-z0-9.+_-]*\.pkg\.tar\.zst"),
    "debug": re.compile(r"arinn-debug-[0-9][A-Za-z0-9.+_-]*\.pkg\.tar\.zst"),
    "source": re.compile(r"arinn-[0-9][A-Za-z0-9.+_-]*\.tar\.gz"),
    "debian": re.compile(r"arinn_[0-9][A-Za-z0-9.+_~-]*\.deb"),
}


def stage(source, output, require_debian=False):
    if output.exists() or output.is_symlink():
        raise ValueError("Publication directory must not already exist")
    selected = []
    found = set()
    for path in sorted(source.iterdir()):
        for category, pattern in PATTERNS.items():
            if pattern.fullmatch(path.name):
                if path.is_symlink() or not path.is_file():
                    raise ValueError("Distributable assets must be regular files")
                selected.append(path)
                found.add(category)
                break
    required = {"arch", "source"}
    if require_debian:
        required.add("debian")
    if not required <= found:
        raise ValueError("Missing required distributable assets: " + ", ".join(sorted(required - found)))
    output.mkdir()
    for path in selected:
        shutil.copyfile(path, output / path.name)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--require-debian", action="store_true")
    args = parser.parse_args()
    try:
        stage(args.source, args.output, args.require_debian)
    except (OSError, ValueError) as error:
        parser.exit(1, f"error: {error}\n")


if __name__ == "__main__":
    main()
