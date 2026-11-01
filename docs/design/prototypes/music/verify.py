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
            page.goto(url + "&scene=yandex-wave")
            assert page.locator("#player button").count() == 0, "Browsing recommendations must not start radio"
            page.locator("#pin-wave").click()
            assert page.locator("#pinned-wave").count() == 0
            page.locator("#pin-wave").click()
            page.locator("#station-wave").click()
            assert "My Wave" in page.locator(".radio-label").inner_text()
            assert "no service connected" in page.locator(".now .eyebrow").inner_text().lower()
            for control in ["previous", "pause", "next", "queue", "seek", "rating-like", "rating-dislike", "track-radio"]:
                assert page.locator("#" + control + " svg").count() == 1
                assert page.locator("#" + control).get_attribute("aria-label")
            assert page.locator("#pause").get_attribute("aria-label") == "Pause"
            for width, height in [(1920, 1080), (3840, 2160)]:
                page.set_viewport_size({"width": width, "height": height})
                row = page.locator(".now .transport").bounding_box()
                buttons = page.locator(".now .transport button").all()
                assert len(buttons) == 8
                centers = [control.bounding_box()["y"] + control.bounding_box()["height"] / 2 for control in buttons]
                assert max(centers) - min(centers) < 2, "Player controls must occupy one horizontal row"
                assert all(control.bounding_box()["x"] >= row["x"] and control.bounding_box()["x"] + control.bounding_box()["width"] <= row["x"] + row["width"] + 1 for control in buttons), "Controls must fit the information column"
            page.set_viewport_size({"width": 1920, "height": 1080})
            page.locator("#pause").click()
            assert page.locator("#pause").get_attribute("aria-label") == "Play"
            assert page.locator("#mini-pause").get_attribute("aria-label") == "Play"
            page.locator("#pause").click()
            current = page.locator("#mini-title h3").inner_text()
            page.locator("#rating-like").click()
            assert page.locator("#rating-like").get_attribute("aria-pressed") == "true"
            page.locator("#rating-dislike").click()
            assert page.locator("#mini-title h3").inner_text() == current, "Dislike must not cut off the playing track"
            assert page.locator("#rating-like").get_attribute("aria-pressed") == "false"
            page.locator("#queue").click()
            assert page.locator(".track").count() == 4
            page.locator("#mini-next").click()
            page.locator("#mini-next").click()
            page.wait_for_function("document.querySelectorAll('.track').length > 4")
            assert "Continuous queue" in page.locator(".radio-status").inner_text()
            assert page.locator("#queue-up").count() == 0
            last = page.locator(".track").last
            key = last.get_attribute("data-entry-key")
            last.click()
            page.locator("#queue-play").click()
            page.wait_for_function("document.querySelector('.radio-status').textContent.includes('Continuous queue')")
            assert page.evaluate("document.activeElement.dataset.entryKey") == key, "Background trim/refill must retain focused track identity"
            page.goto(url + "&scene=radio-error")
            assert page.locator(".track").count() == 3
            page.locator("#radio-retry").click()
            page.wait_for_function("document.querySelectorAll('.track').length === 6")
            assert page.locator("#mini-title h3").inner_text() == "Distant Shore 01"
            page.locator("#radio-stop").click()
            assert page.locator("#player button").count() == 0
            page.goto(url + "&scene=radio-error")
            page.locator("#radio-retry").click()
            page.locator("#radio-stop").click()
            page.wait_for_timeout(800)
            assert page.locator("#player button").count() == 0, "Late refill must not revive a stopped station"
            page.goto(url + "&scene=radio-player")
            page.locator("#sources").click()
            page.locator("#source-local").click()
            page.locator("#search").click()
            page.locator("#query").fill("Distant Shore")
            page.keyboard.press("Enter")
            assert page.locator(".track").count() == 0, "Generated radio fixtures must not leak into local search"
            page.goto(url + "&scene=yandex-wave")
            page.locator("#station-daily").click()
            assert page.locator(".radio-label").count() == 0, "A personal playlist is finite, not automatically a radio"
            page.locator("#queue").click()
            assert page.locator(".radio-status").count() == 0
            page.goto(url + "&scene=track-radio")
            page.locator("#track-menu").click()
            page.locator("#seed-radio").click()
            assert "Radio from First Light" in page.locator(".radio-label").inner_text()
            page.goto(url + "&scene=youtube-mixes")
            page.locator("#station-supermix").click()
            assert "My Supermix" in page.locator(".radio-label").inner_text()
            page.goto(url + "&scene=radio-empty")
            assert page.locator(".track").count() == 0
            assert "No upcoming tracks" in page.locator(".radio-status").inner_text()
            page.locator("#radio-retry").click()
            page.wait_for_function("document.querySelectorAll('.track').length === 3")
            page.goto(url + "&scene=feedback-loading")
            assert "Saving your feedback" in page.locator(".now").inner_text()
            assert page.locator("#rating-like").count() == 0
            page.locator("#feedback-cancel").click()
            assert page.locator("#rating-like").count() == 1
            page.goto(url + "&scene=feedback-error")
            page.locator("#feedback-retry").click()
            assert page.locator("#rating-like").get_attribute("aria-pressed") == "true"
            page.goto(url + "&scene=yandex-wave")
            page.locator("#station-wave").click()
            for _ in range(22):
                page.locator("#rating-dislike").click()
                page.locator("#next").click()
            page.locator("#queue").click()
            assert page.locator(".track").count() <= 6, "Feedback replacement must also bound queue history"
            page.goto(url + "&scene=home-card")
            assert page.locator("#home-toggle").get_attribute("aria-label") == "Pause"
            assert "My Wave" in page.locator(".home-music-card").inner_text()
            page.locator("#home-toggle").click()
            assert page.locator("#home-toggle").get_attribute("aria-label") == "Play"
            assert "Paused" in page.locator(".home-status").inner_text()
            page.locator("#open-music-player").click()
            assert page.locator("#pause").get_attribute("aria-label") == "Play", "Opening a Home card preserves playback state"
            for variant in ["inline", "balanced", "minimal"]:
                for width, height in [(1920, 1080), (3840, 2160)]:
                    page.set_viewport_size({"width": width, "height": height})
                    page.goto(url + "&scene=controls-" + variant)
                    controls = page.locator(".now .transport button").all()
                    assert len(controls) == (5 if variant == "minimal" else 8)
                    centers = [el.bounding_box()["y"] + el.bounding_box()["height"] / 2 for el in controls]
                    assert max(centers) - min(centers) < 2
                    info = page.locator(".now .info").bounding_box()
                    assert all(el.bounding_box()["x"] >= info["x"] and el.bounding_box()["x"] + el.bounding_box()["width"] <= info["x"] + info["width"] + 1 for el in controls)
                    if variant == "balanced":
                        pause = page.locator("#pause").bounding_box()
                        assert abs(pause["x"] + pause["width"] / 2 - (info["x"] + info["width"] / 2)) < 2
                if variant != "minimal":
                    page.locator("#pause").focus()
                    target, direction = "rating-dislike", "ArrowLeft" if variant == "balanced" else "ArrowRight"
                    hops = 0
                    while page.evaluate("document.activeElement.id") != target and hops < 10:
                        page.keyboard.press(direction)
                        hops += 1
                    assert hops == (2 if variant == "balanced" else 5), (variant, hops)
            page.locator("#pause").focus()
            for _ in range(3):
                page.keyboard.press("ArrowRight")
            assert page.evaluate("document.activeElement.id") == "more"
            page.keyboard.press("Enter")
            page.keyboard.press("ArrowDown")
            assert page.evaluate("document.activeElement.id") == "more-dislike"
            page.keyboard.press("Escape")
            page.locator("#more").click()
            assert page.locator("#more-like").count() == 1
            page.keyboard.press("Escape")
            assert page.locator("#more").evaluate("el => el === document.activeElement")
            page.locator("#more").click()
            page.locator("#more-like").click()
            assert page.locator("#more").evaluate("el => el === document.activeElement")
            page.locator("#more").click()
            assert page.locator("#more-like").inner_text() == "Unlike"
            assert page.locator("#more-like").get_attribute("aria-pressed") == "true"
            page.locator("#more-dislike").click()
            page.locator("#more").click()
            assert page.locator("#more-dislike").inner_text() == "Undo dislike"
            assert page.locator("#more-dislike").get_attribute("aria-pressed") == "true"
            page.locator("#more-radio").click()
            page.locator("#seed-radio").click()
            assert page.locator("#replace-cancel").evaluate("el => el === document.activeElement")
            page.locator("#replace-cancel").click()
            assert page.locator("#more").evaluate("el => el === document.activeElement")
            if args.capture:
                (HERE / "screenshots").mkdir(exist_ok=True)
                for width, height, scenes_to_capture in [
                    (1920, 1080, ["catalogue", "sources", "detail", "player", "queue", "offline", "replace", "yandex-wave", "youtube-mixes", "radio-player", "radio-queue", "radio-loading", "radio-error", "radio-empty", "feedback-loading", "feedback-error"]),
                    (3840, 2160, ["catalogue", "player", "yandex-wave", "radio-player"]),
                ]:
                    page.set_viewport_size({"width": width, "height": height})
                    for scene in scenes_to_capture:
                        page.goto(url + "&scene=" + scene)
                        page.evaluate("document.fonts.ready")
                        page.screenshot(path=str(HERE / "screenshots" / f"{scene}-{height}.png"))
                page.set_viewport_size({"width": 1920, "height": 1080})
                for scene in ["home-card", "home-card-paused"]:
                    page.goto(url + "&scene=" + scene)
                    page.evaluate("document.fonts.ready")
                    box = page.locator(".home-music-card").bounding_box()
                    page.screenshot(path=str(HERE / "screenshots" / f"{scene}-1080.png"), clip={
                        "x": box["x"] - 12, "y": box["y"] - 12,
                        "width": box["width"] + 24, "height": box["height"] + 24,
                    })
                for width, height in [(1920, 1080), (3840, 2160)]:
                    page.set_viewport_size({"width": width, "height": height})
                    for variant in ["inline", "balanced", "minimal"]:
                        page.goto(url + "&scene=controls-" + variant)
                        page.evaluate("document.fonts.ready")
                        page.screenshot(path=str(HERE / "screenshots" / f"controls-{variant}-{height}.png"))
                        if width == 1920:
                            info = page.locator(".now .info").bounding_box()
                            row = page.locator(".now .transport").bounding_box()
                            page.screenshot(path=str(HERE / "screenshots" / f"controls-{variant}-strip.png"), clip={
                                "x": info["x"] - 10, "y": row["y"] - 12,
                                "width": info["width"] + 20, "height": row["height"] + 72,
                            })
            assert not errors, errors
            assert all(request.startswith(origin + "/") for request in requests), "External requests"
            browser.close()
            print(f"Passed: navigation, playback isolation, search identity, focus, queue, {len(scenes)} scenes, offline assets.")
    finally:
        server.shutdown()
        server.server_close()


if __name__ == "__main__":
    main()
