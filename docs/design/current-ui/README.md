<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Current Qt interface captures

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
These images are rendered on Qt 6.8.2. Known artefacts, such as empty glyph
boxes in the Open services action column and placeholder labels from the
fixtures, are tracked in [#569](https://github.com/LimeBeck/arinn/issues/569)
(fixtures in `capture.qml` and a re-shoot on Qt 6.10).

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

Inspect the images before committing regenerated captures, then refresh the
copy in [`docs/user/assets/home.png`](../../user/assets/home.png). The site
build takes these files as they are. Historical handoff and archive
screenshots remain unchanged.
