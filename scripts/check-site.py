#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Arinn contributors
# SPDX-License-Identifier: GPL-3.0-or-later
"""Check the assembled public site, project-prefix links and publication boundary."""

import argparse
import importlib.util
import re
from pathlib import Path
from urllib.parse import unquote, urlsplit

from html.parser import HTMLParser


def load_builder():
    spec = importlib.util.spec_from_file_location("build_site", Path(__file__).with_name("build-site.py"))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class Page(HTMLParser):
    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.ids = set()
        self.links = []
        self.remote_assets = set()
        self.errors = []
        self.structure = set()
        self.h1_count = 0

    def handle_starttag(self, tag, attrs):
        attrs = dict(attrs)
        if "id" in attrs:
            value = attrs["id"]
            if not value or value in self.ids:
                self.errors.append(f"empty or duplicate id: {value!r}")
            self.ids.add(value)
        for name in ("href", "src"):
            if name in attrs:
                self.links.append(attrs[name])
        if tag in {"script", "img", "link"}:
            self.remote_assets.add(attrs.get("src") or attrs.get("href"))
        if tag == "img" and not attrs.get("alt"):
            self.errors.append("image needs descriptive alt text")
        if tag == "html" and attrs.get("lang") == "en":
            self.structure.add("language")
        if tag in {"title", "main"}:
            self.structure.add(tag)
        if tag == "meta" and attrs.get("name") == "viewport":
            self.structure.add("viewport")
        if tag == "h1":
            self.h1_count += 1
        if tag == "script" and not attrs.get("src"):
            self.errors.append("inline scripts are not expected")


def check(root: Path) -> list[str]:
    root = root.resolve()
    errors = []
    files = {path.relative_to(root).as_posix() for path in root.rglob("*") if path.is_file()}
    expected = load_builder().PUBLISHED_FILES
    if files != expected:
        errors.append(f"Publish allowlist differs: missing={sorted(expected - files)}, extra={sorted(files - expected)}")
    if any(path.is_symlink() for path in root.rglob("*")):
        errors.append("Public output must not contain symlinks")
    pages = {}
    for path in root.rglob("*.html"):
        page = Page()
        page.feed(path.read_text(encoding="utf-8"))
        page.close()
        pages[path.resolve()] = page
        errors.extend(f"{path.relative_to(root)}: {error}" for error in page.errors)
        if page.structure != {"language", "title", "main", "viewport"} or page.h1_count != 1:
            errors.append(f"{path.relative_to(root)}: missing document structure or unique h1")
    references = [(path, link) for path, page in pages.items() for link in page.links]
    for path in root.rglob("*.css"):
        references.extend((path, match[1].strip()) for match in re.findall(
            r"url\(\s*(['\"]?)(.*?)\1\s*\)", path.read_text(encoding="utf-8")))
    for path, link in references:
        prefix = str(path.relative_to(root))
        if not link:
            errors.append(f"{prefix}: empty reference")
            continue
        url = urlsplit(link)
        if url.scheme or url.netloc:
            if url.scheme != "https" or not url.netloc:
                errors.append(f"{prefix}: unsupported remote reference {link}")
            elif path.suffix == ".css" or link in pages[path].remote_assets:
                errors.append(f"{prefix}: third-party runtime asset {link}")
            continue
        decoded = unquote(url.path)
        if decoded.startswith("/"):
            errors.append(f"{prefix}: root-relative reference breaks project Pages: {link}")
            continue
        target = (path.parent / decoded).resolve() if decoded else path
        if not target.is_relative_to(root) or not target.is_file():
            errors.append(f"{prefix}: missing or out-of-site reference {link}")
        elif url.fragment and (target not in pages or unquote(url.fragment) not in pages[target].ids):
            errors.append(f"{prefix}: missing anchor {link}")
    return errors


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("site", type=Path, nargs="?", default=Path(__file__).resolve().parents[1] / "build/site")
    errors = check(parser.parse_args().site)
    if errors:
        raise SystemExit("\n".join(errors))
    print("Product site: publish allowlist, links, anchors, assets and structure valid")
