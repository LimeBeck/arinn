#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2026 Arinn contributors
#
# SPDX-License-Identifier: GPL-3.0-or-later

"""Consistency checks for living documentation (ADR-0028).

Checks:
  * relative links and #anchors in living Markdown files resolve;
  * every ADR has a well-formed header and is listed in the ADR register;
  * Superseded/Deprecated ADRs name their successor;
  * every acceptance criterion of the specification is assigned in ROADMAP.md;
  * living documents are written in English (no Cyrillic outside allowed files).

Run from anywhere: python scripts/check-docs.py
"""

import re
import sys
import unicodedata
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SPEC = ROOT / "docs/specifications/arinn-specification.md"
ROADMAP = ROOT / "ROADMAP.md"
ADR_DIR = ROOT / "docs/adr"

# Historical sources are kept as delivered and never checked or edited.
HISTORICAL = (
    "docs/specifications/developer-specification-v0.3.md",
    "docs/design/handoff/",
    "docs/design/archive/",
)
# Living files that may contain Cyrillic text.
CYRILLIC_ALLOWED = ("README.ru.md",)

LINK = re.compile(r"(?<!!)\[[^\]]*\]\(<?([^)\s>]+)>?(?:\s+\"[^\"]*\")?\)|!\[[^\]]*\]\(<?([^)\s>]+)>?\)|<img [^>]*src=\"([^\"]+)\"")
FENCE = re.compile(r"^(```|~~~)")
CYRILLIC = re.compile("[\\u0400-\\u04ff]")
STATUSES = {"Proposed", "Accepted", "Rejected", "Deprecated", "Superseded"}

errors: list[str] = []


def error(path: Path, message: str) -> None:
    errors.append(f"{path.relative_to(ROOT)}: {message}")


def living_markdown() -> list[Path]:
    files = []
    for path in sorted(ROOT.rglob("*.md")):
        rel = path.relative_to(ROOT).as_posix()
        if rel.startswith(("target/", "build/", ".git/", ".phase0/")):
            continue
        if rel.startswith(HISTORICAL):
            continue
        files.append(path)
    return files


def strip_code(text: str) -> str:
    """Remove fenced code blocks and inline code so examples are not checked."""
    out, fenced = [], False
    for line in text.splitlines():
        if FENCE.match(line.strip()):
            fenced = not fenced
            out.append("")
            continue
        out.append("" if fenced else re.sub(r"`[^`]*`", "", line))
    return "\n".join(out)


def slug(heading: str) -> str:
    """GitHub-compatible heading anchor."""
    text = re.sub(r"<[^>]+>", "", heading).strip().lower()
    text = text.replace("`", "")
    text = re.sub(r"\[([^\]]*)\]\([^)]*\)", r"\1", text)
    kept = []
    for char in text:
        if char in " -_" or char.isalnum() or unicodedata.category(char).startswith("L"):
            kept.append(char)
    return "".join(kept).replace(" ", "-")


def anchors(path: Path) -> set[str]:
    result, seen = set(), {}
    for line in strip_code(path.read_text(encoding="utf-8")).splitlines():
        match = re.match(r"^#{1,6}\s+(.*)$", line)
        if not match:
            continue
        base = slug(match.group(1))
        count = seen.get(base, 0)
        seen[base] = count + 1
        result.add(base if count == 0 else f"{base}-{count}")
    return result


def check_links(path: Path) -> None:
    text = strip_code(path.read_text(encoding="utf-8"))
    for match in LINK.finditer(text):
        target = next(group for group in match.groups() if group)
        if re.match(r"^[a-z][a-z0-9+.-]*:", target) or target.startswith("//"):
            continue  # external URL or mailto
        file_part, _, anchor = target.partition("#")
        # Changelog fragments are moved into CHANGELOG.md, so their links are relative to the root.
        base = ROOT if path.parent.parent == ROOT / "changes" else path.parent
        resolved = path if not file_part else (base / file_part).resolve()
        if not resolved.exists():
            error(path, f"broken link {target}")
            continue
        if anchor and resolved.suffix == ".md" and anchor not in anchors(resolved):
            error(path, f"missing anchor #{anchor} in {resolved.relative_to(ROOT)}")


def check_language(path: Path) -> None:
    if path.relative_to(ROOT).as_posix() in CYRILLIC_ALLOWED:
        return
    for number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        if "README.ru.md" in line:
            continue  # the link to the Russian README may be labelled in Russian
        if CYRILLIC.search(line):
            error(path, f"line {number}: Cyrillic text in a living English document")


def check_adrs() -> None:
    register = (ADR_DIR / "README.md").read_text(encoding="utf-8")
    for path in sorted(ADR_DIR.glob("[0-9][0-9][0-9][0-9]-*.md")):
        number = path.name[:4]
        text = path.read_text(encoding="utf-8")
        if not text.startswith(f"# ADR-{number}: "):
            error(path, f"title must start with '# ADR-{number}: '")
        status = re.search(r"^- \*\*Status:\*\* (\w+)", text, re.MULTILINE)
        if not status or status.group(1) not in STATUSES:
            error(path, "missing or invalid '- **Status:**' line")
        for field in ("Date", "Deciders", "Amends / Supersedes", "Amended by / Superseded by", "Specification"):
            if f"- **{field}:**" not in text:
                error(path, f"missing header field '{field}'")
        if status and status.group(1) in {"Superseded", "Deprecated"}:
            successor = re.search(r"^- \*\*Amended by / Superseded by:\*\* (.*)$", text, re.MULTILINE)
            if not successor or "ADR-" not in successor.group(1):
                error(path, f"{status.group(1)} ADR must name its successor")
        if f"({path.name})" not in register:
            error(ADR_DIR / "README.md", f"ADR register does not list {path.name}")


def check_acceptance_criteria() -> None:
    spec = SPEC.read_text(encoding="utf-8")
    section = spec.split("## 19. Acceptance criteria", 1)[-1].split("\n## ", 1)[0]
    defined = set(re.findall(r"^\| (AC-\d{2}) \|", section, re.MULTILINE))
    if not defined:
        error(SPEC, "no acceptance criteria found in §19")
    roadmap = ROADMAP.read_text(encoding="utf-8")
    assigned = re.findall(r"^\| (AC-\d{2})\b", roadmap, re.MULTILINE)
    for criterion in sorted(defined):
        count = assigned.count(criterion)
        if count != 1:
            error(ROADMAP, f"{criterion} must be an exit criterion of exactly one phase (found {count})")
    for criterion in sorted(set(assigned) - defined):
        error(ROADMAP, f"{criterion} is not defined in the specification")


def check_tools() -> None:
    """ADR register/numbering and changelog fragments (scripts/adr.py, scripts/release.py)."""
    sys.path.insert(0, str(ROOT / "scripts"))
    import adr  # noqa: E402
    import release  # noqa: E402

    for problem in adr.check():
        error(ADR_DIR / "README.md", problem)
    for problem in release.fragment_problems():
        error(ROOT / "changes", problem)


def main() -> int:
    for path in living_markdown():
        check_links(path)
        check_language(path)
    check_adrs()
    check_acceptance_criteria()
    check_tools()
    if errors:
        print("\n".join(errors))
        print(f"\n{len(errors)} documentation problem(s) found.")
        return 1
    print("Documentation checks passed.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
