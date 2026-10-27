<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Showcase screenshots

The README (EN/RU), product website and handbook use this reviewed subset of
fixture-driven Qt/QML captures from `tests/ui/visual`. The retired component
gallery is not a screen renderer. The wider, earlier comparison baseline lives
in [current-ui](../current-ui/README.md); historical handoff/archive assets are
not changed by this refresh.

Captured on 2026-10-27 from source commit `7782954` with Qt 6.12.0,
the offscreen platform and the software Qt Quick renderer, at 1920 × 1080.
Only the four states selected in [#569](https://github.com/LimeBeck/arinn/issues/569)
are published. The owner accepted Qt 6.8 or later; a Qt 6.10-only environment is
not required. The production views and fixtures include the #572 icon/clock
fixes and the #574 Home favorite ellipsis fix.

| File | Manifest state | Contents |
| --- | --- | --- |
| `home.png` | `home-populated` | Continue cards, fictional favorite tiles and focused Home mark |
| `open-services.png` | `browser-list` | Fictional .example services, drawn bookmark icons, active/sound states |
| `settings.png` | `settings-rows` | Appearance: light field, ambient timeout, text size and source lines |
| `library-folders.png` | `library-folders-list` | Inline fictional folder paths with Add path / Remove controls |

The capture uses a fixed 22:57 clock, synthetic titles and paths, neutral service
monograms and the original fixture artwork. It connects to no daemon or live
desktop. Existing handoff differences remain product follow-ups; this refresh
does not pass G-5.1, qualify a desktop/TV or prove every integration complete.
Launch states remain excluded until #573 is resolved. No image shows a user's
media, browser activity or credentials. Captions describe these exact states;
the folder-list frame does not depict an unavailable folder or a graphical picker.

From the repository root:

```sh
cmake -S . -B build -G Ninja
cmake --build build --target arinn-ui-test
python scripts/ui-compare.py --build-dir build --output build/ui-compare
cp build/ui-compare/1080/home-populated.png docs/design/showcase/home.png
cp build/ui-compare/1080/browser-list.png docs/design/showcase/open-services.png
cp build/ui-compare/1080/settings-rows.png docs/design/showcase/settings.png
cp build/ui-compare/1080/library-folders-list.png docs/design/showcase/library-folders.png
cp docs/design/showcase/home.png docs/user/assets/home.png
```

The comparison tool also attempts 4K renders; published files use only its
1080p output. Inspect the four frames and their captions before updating the
public set. Their independent design review found no empty icon squares or
clipped Media showcase row. The approved Arinn mark and all historical design
sources stay unchanged. Documentation and captures are CC-BY-SA-4.0 under the
existing REUSE annotations; bundled Onest retains OFL-1.1.
