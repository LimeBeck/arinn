# SPDX-FileCopyrightText: 2026 Arinn contributors
# SPDX-License-Identifier: CC-BY-SA-4.0
"""Offline Chromium interaction checks and captures; no desktop session involved."""
import argparse
from functools import partial
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from threading import Thread

from playwright.sync_api import sync_playwright

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[3]


class QuietHandler(SimpleHTTPRequestHandler):
    def log_message(self, *_args):
        pass


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--chromium", default="/usr/bin/chromium")
    parser.add_argument("--capture", action="store_true")
    args = parser.parse_args()
    server = ThreadingHTTPServer(("127.0.0.1", 0), partial(QuietHandler, directory=str(ROOT)))
    Thread(target=server.serve_forever, daemon=True).start()
    origin = f"http://127.0.0.1:{server.server_port}"
    url = origin + "/docs/design/prototypes/music/index.html?controls=0"
    try:
        with sync_playwright() as p:
            browser = p.chromium.launch(executable_path=args.chromium, args=["--no-sandbox"])
            page = browser.new_page(viewport={"width": 1920, "height": 1080})
            errors, requests = [], []
            page.on("pageerror", lambda error: errors.append(str(error)))
            page.on("request", lambda request: requests.append(request.url))
            page.goto(url)
            page.locator("#album-2").click()
            assert page.locator("#play-album").count() == 1
            assert page.locator("#player button").count() == 0, "Opening detail must not play"
            page.locator("#play-album").click()
            assert "Parallel Coast" in page.locator("#mini-title").inner_text()
            page.keyboard.press("Escape")
            assert page.locator("#album-2").evaluate("el => el === document.activeElement")
            page.locator("#search").click()
            page.locator("#query").fill("Glass")
            page.locator("#keyboard").click()
            assert page.locator("#typed").inner_text() == "Glass", "Blur must not swallow a keyboard click"
            page.keyboard.press("Escape")
            page.locator("#query").focus()
            page.keyboard.press("Enter")
            page.locator("#track-0").click()
            # Search returns original track 1, not row 0. Replacement is explicit.
            page.locator("#replace-confirm").click()
            assert "Glass Rivers" in page.locator("#mini-title h3").inner_text()
            page.keyboard.press("h")
            page.locator("#music").click()
            assert page.locator("#query").input_value() == "Glass"
            page.locator("#keyboard").click()
            page.keyboard.press("Escape")
            assert page.locator("#keyboard").evaluate("el => el === document.activeElement")
            page.locator("#mini-queue").click()
            before = page.locator(".track").count()
            page.locator("#track-2").click()
            page.locator("#queue-remove").click()
            assert page.locator(".track").count() == before - 1
            page.locator("#mini-next").click()
            assert "Soft Edges" in page.locator("#mini-title").inner_text(), "Transport must skip removed queue entries"
            page.locator("#sources").click()
            page.locator("#source-youtube").click()
            assert "not ready" in page.locator(".state h2").inner_text()
            assert page.locator("#search").count() == 0, "Blocked sources must not search local fixtures"
            assert "Soft Edges" in page.locator("#mini-title").inner_text(), "Browsing preserves session"
            page.locator("#sources").click()
            for _ in range(12):
                page.keyboard.press("Tab")
                assert page.locator("#overlay").evaluate("el => el.contains(document.activeElement)")
            page.keyboard.press("Escape")
            assert page.locator("#sources").evaluate("el => el === document.activeElement")
            page.keyboard.press("ArrowDown")
            assert page.evaluate("document.activeElement.tagName") == "BUTTON"
            page.goto(url)
            page.locator("#tab-Playlists").click()
            page.locator("#album-2").click()
            assert "Quiet hours" in page.locator(".detail h1").inner_text()
            page.goto(url)
            page.locator("#album-0").click()
            page.locator("#play-album").click()
            page.keyboard.press("Escape")
            page.locator("#album-1").click()
            page.locator("#add-album").click()
            page.keyboard.press("Escape")
            page.locator("#mini-queue").click()
            assert "Mira Vale" in page.locator("#track-6").inner_text(), "Queue entries retain album identity"
            page.locator("#track-6").click()
            page.locator("#queue-play").click()
            assert "Mira Vale" in page.locator("#mini-title").inner_text()
            scenes = page.locator("#scene option").all_text_contents()
            for scene in scenes:
                page.goto(url + "&scene=" + scene)
                assert page.locator("header h1").count() == 1, scene
                assert not page.locator("body").evaluate("el => el.scrollWidth > innerWidth"), scene
            page.goto(url + "&scene=replace")
            assert page.locator("#replace-cancel").evaluate("el => el === document.activeElement")
            page.locator("#replace-cancel").click()
            assert "Glass Rivers" in page.locator("#mini-title").inner_text()
            if args.capture:
                (HERE / "screenshots").mkdir(exist_ok=True)
                for width, height, scenes_to_capture in [
                    (1920, 1080, ["catalogue", "sources", "detail", "player", "queue", "offline", "replace"]),
                    (3840, 2160, ["catalogue", "player"]),
                ]:
                    page.set_viewport_size({"width": width, "height": height})
                    for scene in scenes_to_capture:
                        page.goto(url + "&scene=" + scene)
                        page.evaluate("document.fonts.ready")
                        page.screenshot(path=str(HERE / "screenshots" / f"{scene}-{height}.png"))
            assert not errors, errors
            assert all(request.startswith(origin + "/") for request in requests), "External requests"
            browser.close()
            print(f"Passed: navigation, playback isolation, search identity, focus, queue, {len(scenes)} scenes, offline assets.")
    finally:
        server.shutdown()
        server.server_close()


if __name__ == "__main__":
    main()
