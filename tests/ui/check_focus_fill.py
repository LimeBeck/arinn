# SPDX-FileCopyrightText: 2026 Arinn contributors
#
# SPDX-License-Identifier: GPL-3.0-or-later

"""Fail when a shell screen hardcodes the day focus plate (#722).

Theme.plateFocus is the only focus fill. White at alpha 0.2 or 0.20, chosen
because a control is focused, is the day plate and misses the night contrast
bar. The scan covers HomeView and the screen QML under ui/screens. Theme.qml
defines the 20% alpha and is not scanned. A fill that is not selected by
focus, such as the Settings switch track, is not a focus plate.
"""

import pathlib
import re
import sys

_WHITE_20 = re.compile(
    r"Qt\.rgba\(\s*1(?:\.0+)?\s*,\s*1(?:\.0+)?\s*,\s*1(?:\.0+)?\s*,"
    r"\s*(?:0\.20?|\.20?)(?!\d)\s*\)"
)
_FOCUS = re.compile(r"\b(?:activeFocus|osdFocus|focused)\b")
_CONTINUATION_END = ("?", ":", "&&", "||", "+", ",", "(", "[", "{", "=")


def screen_files(root: pathlib.Path) -> list[pathlib.Path]:
    ui = root / "apps" / "arinn-shell" / "ui"
    files = [ui / "HomeView.qml"]
    screens = ui / "screens"
    if screens.is_dir():
        files.extend(sorted(screens.rglob("*.qml")))
    return [path for path in files if path.is_file()]


def strip_comments(text: str) -> str:
    out: list[str] = []
    i = 0
    n = len(text)
    while i < n:
        if text.startswith("//", i):
            while i < n and text[i] != "\n":
                out.append(" ")
                i += 1
        elif text.startswith("/*", i):
            out.append(" ")
            out.append(" ")
            i += 2
            while i < n and not text.startswith("*/", i):
                out.append("\n" if text[i] == "\n" else " ")
                i += 1
            if text.startswith("*/", i):
                out.append(" ")
                out.append(" ")
                i += 2
        elif text[i] in "\"'`":
            quote = text[i]
            out.append(text[i])
            i += 1
            while i < n and text[i] != quote and text[i] != "\n":
                if text[i] == "\\":
                    out.append(text[i])
                    i += 1
                    if i < n and text[i] != "\n":
                        out.append(text[i])
                        i += 1
                    continue
                out.append(text[i])
                i += 1
            if i < n and text[i] == quote:
                out.append(text[i])
                i += 1
        else:
            out.append(text[i])
            i += 1
    return "".join(out)


def statements(text: str) -> list[tuple[int, str]]:
    lines = text.splitlines(keepends=True)
    buf: list[str] = []
    start = 1
    paren = 0
    bracket = 0
    found: list[tuple[int, str]] = []
    for number, line in enumerate(lines, 1):
        if not buf:
            start = number
        buf.append(line)
        for ch in line:
            if ch == "(":
                paren += 1
            elif ch == ")":
                paren = max(0, paren - 1)
            elif ch == "[":
                bracket += 1
            elif ch == "]":
                bracket = max(0, bracket - 1)
        stripped = line.strip()
        nxt = lines[number].strip() if number < len(lines) else ""
        continues = (
            paren > 0
            or bracket > 0
            or stripped.endswith(_CONTINUATION_END)
            or nxt.startswith(("?", ":", "&&", "||", "+", ","))
        )
        if not continues:
            found.append((start, "".join(buf)))
            buf = []
            paren = 0
            bracket = 0
    if buf:
        found.append((start, "".join(buf)))
    return found


def _focus_condition(statement: str) -> bool:
    stripped = statement.strip()
    return stripped.startswith("if") and _FOCUS.search(stripped) is not None


def offenders_in(text: str) -> list[tuple[int, str]]:
    found: list[tuple[int, str]] = []
    previous = ""
    for line_no, statement in statements(strip_comments(text)):
        if _WHITE_20.search(statement) and (
            _FOCUS.search(statement) or _focus_condition(previous)
        ):
            snippet = " ".join(statement.split())
            found.append((line_no, snippet))
        previous = statement
    return found


def scan(root: pathlib.Path) -> list[str]:
    found: list[str] = []
    for path in screen_files(root):
        text = path.read_text(encoding="utf-8")
        for line_no, snippet in offenders_in(text):
            found.append(f"{path.relative_to(root)}:{line_no}: {snippet}")
    return found


def main(argv: list[str]) -> int:
    root = pathlib.Path(argv[1]) if len(argv) > 1 else pathlib.Path(__file__).resolve().parents[2]
    found = scan(root)
    if found:
        print("hardcoded focus fill; use Theme.plateFocus:", file=sys.stderr)
        for item in found:
            print(item, file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
