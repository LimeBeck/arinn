#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Arinn contributors
# SPDX-License-Identifier: GPL-3.0-or-later
"""Retain upstream notices for statically compiled native SDK dependencies."""
from pathlib import Path
import re
import sys

sdk, toml, destination = map(Path, sys.argv[1:])
notices = {
    "sentry-native-LICENSE": (sdk / "LICENSE").read_text(),
    "tomlplusplus-LICENSE": (toml / "LICENSE").read_text(),
}
for name, author in (("mpack.c", "Nicholas Fraser"), ("jsmn.h", "Serge Zaitsev"),
                     ("stb_sprintf.h", "Sean Barrett")):
    source = (sdk / "vendor" / name).read_text()
    blocks = re.findall(r"/\*.*?\*/", source, re.DOTALL)
    matching = [block for block in blocks if author in block and "Permission is hereby granted" in block]
    if len(matching) != 1:
        raise SystemExit(f"Unsupported upstream notice in {name}")
    text = matching[0][2:-2].strip()
    text = re.sub(r"(?m)^\s*\* ?", "", text)
    notices[f"sentry-native-{Path(name).stem}-LICENSE"] = text.strip() + "\n"
destination.mkdir(parents=True, exist_ok=True)
for name, text in notices.items():
    path = destination / name
    if not path.exists() or path.read_text() != text:
        path.write_text(text)
