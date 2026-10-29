<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Current Qt interface captures

The README, website and handbook use the separately reviewed public captures in
[showcase](../showcase/README.md), refreshed under
[#569](https://github.com/LimeBeck/arinn/issues/569). This directory retains the
earlier comparison baseline described below; it is not the public showcase.

These images come from the fixture render of
[`tests/ui/visual`](../../../tests/ui/visual/) ([manifest](../../../tests/ui/visual/manifest.json),
[`capture.qml`](../../../tests/ui/visual/capture.qml)), the same render that is
compared with the design handoff ([UI components](../../architecture/ui-components.md#handoff-visual-comparison)).
The development gallery `arinn-ui-gallery` is a component catalogue only and
no longer renders screens (#567).

Captured on 2026-10-27 from `master` 9987ba0 at 1920 × 1080 with Qt 6.8.2, the
offscreen platform and the software Qt Quick renderer. Each file is the 1080p
render of one manifest state:

| File | Manifest state |
| --- | --- |
| `home.png` | `home-populated` |
| `navigation.png` | `home-nav` |
| `library.png` | `library-grid` |
| `applications.png` | `applications` |
| `open-services.png` | `browser-list` |
| `keyboard.png` | `keyboard-browse` |
| `now-playing.png` | `session-ready` |
| `settings.png` | `settings` |

The fixtures are synthetic: fictional titles, one original placeholder image
(`apps/arinn-shell/gallery/fixture-artwork.svg`), neutral application
monograms and plain plates where a fixture sets no artwork. The render does
not connect to a daemon or control the desktop. Favorites, services and
settings rows are fixture data, not evidence of completed integrations.
The production shell loads real posters and fanart through `Assets1`.
These baseline images were rendered on Qt 6.8.2 before the fictional-service,
bookmark-icon and deterministic-clock fixes in #572. They can contain old glyph
boxes and fixture labels; the refreshed public subset has its own environment
and provenance in [showcase](../showcase/README.md).

The mark is the unchanged [approved SVG](../handoff/assets/arinn-mark.svg),
embedded as a Qt resource. The screenshots are documentation under CC-BY-SA-4.0,
as covered by the repository's existing REUSE annotations.

From the repository root:

```sh
cmake -S . -B build -G Ninja
cmake --build build --target arinn-ui-test
python scripts/ui-compare.py
cp build/ui-compare/1080/home-populated.png docs/design/current-ui/home.png
cp build/ui-compare/1080/home-nav.png docs/design/current-ui/navigation.png
cp build/ui-compare/1080/library-grid.png docs/design/current-ui/library.png
cp build/ui-compare/1080/applications.png docs/design/current-ui/applications.png
cp build/ui-compare/1080/browser-list.png docs/design/current-ui/open-services.png
cp build/ui-compare/1080/keyboard-browse.png docs/design/current-ui/keyboard.png
cp build/ui-compare/1080/session-ready.png docs/design/current-ui/now-playing.png
cp build/ui-compare/1080/settings.png docs/design/current-ui/settings.png
```

Inspect comparison images before committing regenerated baselines. Public
README/site images are selected and reviewed separately in `showcase`; the
handbook Home copy follows that public source. Historical handoff and archive
screenshots remain unchanged.
