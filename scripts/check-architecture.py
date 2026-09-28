#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Arinn contributors
# SPDX-License-Identifier: GPL-3.0-or-later

"""Narrow regression guards for G-1.4 / AC-12; not a substitute for review."""
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[1]
errors = []
for folder in ("crates", "apps", "plugins", "tools", "platform"):
    for path in (ROOT / folder).rglob("*"):
        if path.suffix not in (".rs", ".cpp", ".h", ".qml", ".js"):
            continue
        text = path.read_text()
        if "org.arinn.Spike" in text:
            errors.append(f"{path.relative_to(ROOT)}: obsolete wire interface")
        if path.parent == ROOT / "crates/arinnd/src":
            if re.search(r"org\.(?:kde|mpris)|demo\.activity|arinn-plugin-(?:demo|chrome|desktop|mpv)", text):
                errors.append(f"{path.relative_to(ROOT)}: product/platform routing in core")
        if path.suffix == ".qml":
            if re.search(r"\bif\s*\([^\n]*(?:Chrome|Steam|demo\.activity|mpv)", text):
                errors.append(f"{path.relative_to(ROOT)}: product-specific QML condition")
if errors:
    raise SystemExit("\n".join(errors))
print("Architecture guards passed (G-1.4, AC-12).")
