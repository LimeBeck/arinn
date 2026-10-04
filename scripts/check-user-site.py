#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Arinn contributors
# SPDX-License-Identifier: GPL-3.0-or-later
"""Validate the dependency-free user guide before publishing to Pages."""

from html.parser import HTMLParser
from pathlib import Path
from urllib.parse import unquote, urlsplit

ROOT = Path(__file__).resolve().parents[1] / "docs/user"


class Page(HTMLParser):
    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.ids = set()
        self.links = []
        self.errors = []
        self.has_language = False
        self.has_title = False
        self.has_main = False
        self.has_viewport = False
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
        if tag == "img" and not attrs.get("alt"):
            self.errors.append("image requires descriptive alt text")
        if tag == "html":
            self.has_language = attrs.get("lang") == "en"
        if tag == "title":
            self.has_title = True
        if tag == "main":
            self.has_main = True
        if tag == "meta" and attrs.get("name") == "viewport":
            self.has_viewport = True
        if tag == "h1":
            self.h1_count += 1


def check():
    pages = {}
    errors = []
    for path in ROOT.rglob("*.html"):
        page = Page()
        page.feed(path.read_text())
        page.close()
        pages[path.resolve()] = page
        errors.extend(f"{path.name}: {error}" for error in page.errors)
        if not all((page.has_language, page.has_title, page.has_main, page.has_viewport)):
            errors.append(f"{path.name}: missing language, title, main or viewport")
        if page.h1_count != 1:
            errors.append(f"{path.name}: expected one h1")
    if not pages or not (ROOT / "index.html").is_file():
        errors.append("site must have index.html")
    for path, page in pages.items():
        for link in page.links:
            if not link:
                errors.append(f"{path.name}: empty link")
                continue
            url = urlsplit(link)
            if url.scheme or url.netloc:
                if url.scheme != "https" or not url.netloc:
                    errors.append(f"{path.name}: unsupported remote link: {link}")
                continue
            if url.path.startswith("/"):
                errors.append(f"{path.name}: root-relative link breaks project Pages: {link}")
                continue
            target = (path.parent / unquote(url.path)).resolve() if url.path else path
            if not target.is_relative_to(ROOT.resolve()) or not target.is_file():
                errors.append(f"{path.name}: missing or out-of-site target: {link}")
            elif url.fragment and (
                target not in pages or unquote(url.fragment) not in pages[target].ids
            ):
                errors.append(f"{path.name}: missing anchor: {link}")
        for section in page.ids:
            if not section:
                errors.append(f"{path.name}: empty anchor")
    if errors:
        raise SystemExit("\n".join(errors))
    print(f"User site: {len(pages)} page(s), links, anchors and structure valid")


if __name__ == "__main__":
    check()
