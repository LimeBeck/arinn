#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2026 Arinn contributors
#
# SPDX-License-Identifier: GPL-3.0-or-later

"""Compare fixture-driven QML screens with the design handoff.

Renders each manifest state offscreen at 1920×1080 and at 4K (logical
1920×1080, device pixel ratio 2) and writes an HTML report. The report is
evidence for layout review. It does not pass G-5.1, and it is not a CI gate.

    python scripts/ui-compare.py --check
    python scripts/ui-compare.py
    python scripts/ui-compare.py --build-dir build --output build/ui-compare
"""

from __future__ import annotations

import argparse
import html
import json
import os
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MANIFEST_PATH = ROOT / "tests/ui/visual/manifest.json"
SCREENSHOTS = ROOT / "docs/design/handoff/screenshots"
CAPTURE_QML = ROOT / "tests/ui/visual/capture.qml"
REQUEST = ROOT / "tests/ui/visual/.capture-request.json"
MARKER = "ARINN_CAPTURE "


def load_manifest() -> dict:
    data = json.loads(MANIFEST_PATH.read_text(encoding="utf-8"))
    if data.get("version") != 1 or not isinstance(data.get("states"), list):
        raise SystemExit(f"{MANIFEST_PATH}: expected version 1 and a states list")
    return data


def validate(manifest: dict) -> list[str]:
    errors: list[str] = []
    seen: set[str] = set()
    referenced: list[str] = []
    for state in manifest["states"]:
        ident = state.get("id")
        if not ident or ident in seen:
            errors.append(f"duplicate or empty state id: {ident!r}")
        seen.add(ident or "")
        if not isinstance(state.get("render"), bool):
            errors.append(f"{ident}: render must be a boolean")
        if state.get("render") is False and not state.get("reason"):
            errors.append(f"{ident}: a state that is not rendered needs a reason")
        reference = state.get("reference", None)
        if reference is None:
            continue
        if not isinstance(reference, str) or not reference:
            errors.append(f"{ident}: reference must be a file name or null")
            continue
        referenced.append(reference)
        if not (SCREENSHOTS / reference).is_file():
            errors.append(f"{ident}: missing handoff file {reference}")
    counts: dict[str, int] = {}
    for name in referenced:
        counts[name] = counts.get(name, 0) + 1
    for name, count in sorted(counts.items()):
        if count != 1:
            errors.append(f"{name} is mapped {count} times")
    on_disk = sorted(path.name for path in SCREENSHOTS.iterdir() if path.is_file())
    mapped = sorted(counts)
    if mapped != on_disk:
        missing = sorted(set(on_disk) - set(mapped))
        extra = sorted(set(mapped) - set(on_disk))
        if missing:
            errors.append("handoff files not in the manifest: " + ", ".join(missing))
        if extra:
            errors.append("manifest references missing on disk: " + ", ".join(extra))
    return errors


def parse_captures(output: str) -> list[dict]:
    rows = []
    for line in output.splitlines():
        index = line.find(MARKER)
        if index < 0:
            continue
        payload = line[index + len(MARKER) :].strip()
        if payload == "DONE" or payload.startswith("DONE"):
            continue
        rows.append(json.loads(payload))
    return rows


def run_capture(
    binary: Path, output: Path, scale: str, scale_factor: str | None
) -> tuple[int, str, list[dict]]:
    output.mkdir(parents=True, exist_ok=True)
    REQUEST.write_text(
        json.dumps({"output": str(output), "scale": scale}),
        encoding="utf-8",
    )
    env = os.environ.copy()
    env["QT_QPA_PLATFORM"] = "offscreen"
    env["QT_QUICK_BACKEND"] = "software"
    env.pop("QT_SCALE_FACTOR", None)
    if scale_factor:
        env["QT_SCALE_FACTOR"] = scale_factor
    command = [str(binary), "-input", str(CAPTURE_QML), "--", str(output), scale]
    try:
        result = subprocess.run(
            command, cwd=ROOT, env=env, text=True, capture_output=True, timeout=600
        )
    except subprocess.TimeoutExpired as exc:
        sys.stderr.write((exc.stdout or "") + "\n" + (exc.stderr or ""))
        raise SystemExit(f"capture timed out for {scale}") from exc
    combined = result.stdout + "\n" + result.stderr
    rows = parse_captures(combined) if "ARINN_CAPTURE_DONE" in combined else []
    return result.returncode, combined, rows


def require_capture(binary: Path, output: Path, scale: str, scale_factor: str | None) -> list[dict]:
    code, combined, rows = run_capture(binary, output, scale, scale_factor)
    if code != 0 or not rows:
        sys.stderr.write(combined)
        raise SystemExit(f"capture failed for {scale} (exit {code})")
    return rows


def four_k_captures(binary: Path, output: Path) -> tuple[list[dict], str]:
    """Prefer a real device pixel ratio of 2. Fall back to a 2× canvas scale."""
    _code, _combined, rows = run_capture(binary, output, "4k", "2")
    if rows and rows[0].get("width") == 3840 and rows[0].get("height") == 2160:
        return rows, "device pixel ratio 2 (3840×2160 framebuffer, logical 1920×1080)"
    rows = require_capture(binary, output, "4k-layout", None)
    if rows[0].get("width") != 3840 or rows[0].get("height") != 2160:
        raise SystemExit(
            f"4K capture was {rows[0].get('width')}×{rows[0].get('height')}, expected 3840×2160"
        )
    return rows, "2× scale of the logical 1920×1080 canvas (3840×2160)"


def rel_reference(report_dir: Path, name: str) -> str:
    return Path(os.path.relpath(SCREENSHOTS / name, report_dir)).as_posix()


def write_reports(report_dir: Path, manifest: dict, captures: dict[str, list[dict]], method: str) -> None:
    by_scale = {scale: {row["id"]: row for row in rows} for scale, rows in captures.items()}
    rendered = [state for state in manifest["states"] if state.get("render")]
    skipped = [state for state in manifest["states"] if not state.get("render")]
    no_reference = [state for state in rendered if not state.get("reference")]

    def cell(scale: str, state: dict) -> str:
        row = by_scale.get(scale, {}).get(state["id"])
        if not row:
            return "<p>Missing capture.</p>"
        image = html.escape(f"{scale}/{row['png']}")
        parts = [
            f'<img src="{image}" alt="">',
            f"<p>{row['width']}×{row['height']}</p>",
        ]
        if row.get("mae") is not None:
            parts.append(f"<p>Mean absolute difference {row['mae']:.3f}</p>")
        return "\n".join(parts)

    blocks = []
    for state in rendered:
        title = html.escape(f"{state['screen']} — {state['summary']}")
        note = html.escape(state.get("note") or "")
        reference = state.get("reference")
        if reference:
            ref = html.escape(rel_reference(report_dir, reference))
            ref_cell = f'<img src="{ref}" alt=""><p>{html.escape(reference)}</p>'
            diff = (
                '<div class="diff">'
                f'<img src="{html.escape("1080/" + state["id"] + ".png")}" alt="">'
                f'<img src="{ref}" alt="">'
                "</div><p>1080p over the handoff, difference blend.</p>"
            )
        else:
            ref_cell = "<p>No handoff reference.</p>"
            diff = ""
        blocks.append(
            "<section>"
            f"<h2>{title}</h2>"
            + (f"<p class=\"note\">{note}</p>" if note else "")
            + "<div class=\"row\">"
            f"<figure><figcaption>1920×1080</figcaption>{cell('1080', state)}</figure>"
            f"<figure><figcaption>4K</figcaption>{cell('4k', state)}</figure>"
            f"<figure><figcaption>Handoff</figcaption>{ref_cell}</figure>"
            f"<figure><figcaption>Difference</figcaption>{diff}</figure>"
            "</div></section>"
        )
    skipped_items = []
    for state in skipped:
        ref = state.get("reference")
        thumb = ""
        if ref:
            thumb = f'<img src="{html.escape(rel_reference(report_dir, ref))}" alt="">'
        skipped_items.append(
            "<li>"
            f"<strong>{html.escape(state['id'])}</strong> "
            f"({html.escape(state.get('reference') or 'no file')}) — "
            f"{html.escape(state.get('reason') or '')}{thumb}</li>"
        )
    bare = "".join(
        f"<li><strong>{html.escape(state['id'])}</strong> — {html.escape(state['summary'])}</li>"
        for state in no_reference
    )
    document = f"""<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>Arinn UI comparison</title>
<style>
body {{ font: 16px/1.4 sans-serif; margin: 24px; background: #111; color: #eee; }}
h1 {{ font-size: 28px; }}
.note {{ color: #ccc; }}
.row {{ display: flex; gap: 16px; flex-wrap: wrap; align-items: flex-start; }}
figure {{ margin: 0; width: 360px; }}
img {{ width: 100%; height: auto; background: #000; }}
.diff {{ position: relative; }}
.diff img {{ display: block; }}
.diff img + img {{ position: absolute; inset: 0; mix-blend-mode: difference; }}
</style>
</head>
<body>
<h1>Fixture screens and the design handoff</h1>
<p>1080p is a 1920×1080 framebuffer. 4K is {html.escape(method)}.</p>
<p>The mean absolute difference samples a 64×36 grid and averages the channel
error from 0 to 1. It spots layout drift. It is not a pass/fail gate: handoff
artwork is placeholder, service marks are monograms, and several references
are a different size from the render.</p>
<p>This report does not pass G-5.1. It does not show a television, a real
desktop, or production data.</p>
{''.join(blocks)}
<h2>Rendered states with no handoff reference</h2>
<ul>{bare}</ul>
<h2>Handoff states that were not rendered</h2>
<ul>{''.join(skipped_items)}</ul>
</body>
</html>
"""
    (report_dir / "report.html").write_text(document, encoding="utf-8")

    lines = [
        "# UI comparison summary",
        "",
        "Evidence for layout review of the fixture-driven screens. This does not pass G-5.1.",
        "",
        f"- 1080p: 1920×1080.",
        f"- 4K: {method}.",
        "- Difference: mean absolute channel error on a 64×36 grid, 0 to 1. Not a threshold.",
        "",
        "| State | Reference | 1080p difference | 4K difference |",
        "| --- | --- | --- | --- |",
    ]
    for state in manifest["states"]:
        if not state.get("render"):
            lines.append(
                f"| `{state['id']}` | `{state.get('reference')}` | not rendered | not rendered |"
            )
            continue
        left = by_scale.get("1080", {}).get(state["id"], {})
        right = by_scale.get("4k", {}).get(state["id"], {})
        def metric(row: dict) -> str:
            if not row:
                return "missing"
            if row.get("mae") is None:
                return "no reference"
            return f"{row['mae']:.3f}"
        ref = state.get("reference") or "—"
        lines.append(f"| `{state['id']}` | `{ref}` | {metric(left)} | {metric(right)} |")
    lines.append("")
    (report_dir / "summary.md").write_text("\n".join(lines), encoding="utf-8")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="validate the manifest and exit")
    parser.add_argument("--build-dir", type=Path, default=ROOT / "build")
    parser.add_argument("--output", type=Path, default=None)
    args = parser.parse_args()
    manifest = load_manifest()
    errors = validate(manifest)
    if errors:
        for error in errors:
            print(error, file=sys.stderr)
        return 1
    if args.check:
        print(f"manifest matches {len(list(SCREENSHOTS.iterdir()))} handoff files")
        return 0
    output = args.output or (args.build_dir / "ui-compare")
    binary = args.build_dir / "apps/arinn-shell/arinn-ui-test"
    if not binary.is_file():
        print(f"missing {binary}; build the arinn-ui-test target first", file=sys.stderr)
        return 1
    try:
        rows_1080 = require_capture(binary, output / "1080", "1080", None)
        if rows_1080 and (rows_1080[0].get("width") != 1920 or rows_1080[0].get("height") != 1080):
            raise SystemExit(
                f"1080p capture was {rows_1080[0].get('width')}×{rows_1080[0].get('height')}"
            )
        rows_4k, method = four_k_captures(binary, output / "4k")
        expected = [state["id"] for state in manifest["states"] if state.get("render")]
        for label, rows in ("1080", rows_1080), ("4k", rows_4k):
            got = [row["id"] for row in rows]
            if got != expected:
                raise SystemExit(f"{label} rendered {got} but the manifest lists {expected}")
        write_reports(output, manifest, {"1080": rows_1080, "4k": rows_4k}, method)
    finally:
        REQUEST.unlink(missing_ok=True)
    print(f"wrote {output / 'report.html'}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
