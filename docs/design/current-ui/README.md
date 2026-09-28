<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Current Qt interface captures

Captured on 2026-09-28 at 1920 × 1080 using Qt 6.11.2, the offscreen platform
and software Qt Quick renderer. These images show the shared Home/navigation
components after the Arinn brand-mark fix, not the historical HTML prototype.
The gallery uses fixed fictional content, original placeholder artwork and neutral
application monograms. It does not connect to a daemon or control the desktop.
Favorites, media and service entries here are fixture data, not evidence of
completed integrations. See [UI status](../../architecture/ui-components.md).

The mark is the unchanged [approved SVG](../handoff/assets/arinn-mark.svg),
embedded as a Qt resource. The screenshots are documentation under CC-BY-SA-4.0,
as covered by the repository's existing REUSE annotations.

From the repository root:

```sh
cmake -S . -B build -G Ninja
cmake --build build --target arinn-ui-gallery
QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software \
  build/apps/arinn-shell/arinn-ui-gallery --home \
  --capture docs/design/current-ui/home.png
QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software \
  build/apps/arinn-shell/arinn-ui-gallery --home --nav \
  --capture docs/design/current-ui/navigation.png
```

Inspect both images before committing regenerated captures. Historical handoff
and archive screenshots remain unchanged.
