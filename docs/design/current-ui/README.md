<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Current Qt interface captures

Captured on 2026-09-30 at 1920 × 1080 using Qt 6.11.2, the offscreen platform
and software Qt Quick renderer. These images show the screens the production
shell mounts: Home, navigation, Library, Applications, Open services, Now
playing and Settings. They are not the historical HTML prototype.

The gallery uses fixed fictional content, one original placeholder image
(`fixture-artwork.svg`) and neutral application monograms. Continue cards,
Library posters and the Now playing plate paint that image with the same
views the shell uses when an artwork URL is set. Late light has no artwork,
so Library draws the generic plate. The production shell loads real posters
and fanart through `Assets1`; a missing image stays that plate.
Timeline thumbnails are not part of these screens. The gallery does not
connect to a daemon or control the desktop. Favorites and the settings rows
are fixture data, not evidence of completed integrations. The production Home
does not bind a New row, so the Home capture does not show one. See
[UI components](../../architecture/ui-components.md).

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
QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software \
  build/apps/arinn-shell/arinn-ui-gallery --library \
  --capture docs/design/current-ui/library.png
QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software \
  build/apps/arinn-shell/arinn-ui-gallery --applications \
  --capture docs/design/current-ui/applications.png
QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software \
  build/apps/arinn-shell/arinn-ui-gallery --browser \
  --capture docs/design/current-ui/open-services.png
QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software \
  build/apps/arinn-shell/arinn-ui-gallery --now-playing \
  --capture docs/design/current-ui/now-playing.png
QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software \
  build/apps/arinn-shell/arinn-ui-gallery --settings \
  --capture docs/design/current-ui/settings.png
```

`--now-playing` is the flag name because Qt treats `--session` as its own
session-restore option. Inspect the images before committing regenerated
captures. Historical handoff and archive screenshots remain unchanged.
