<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Showcase screenshots

The README (EN/RU), product website and handbook share these six fixture-driven
captures of the production Qt/QML components. Refreshed on 2026-11-07 in
[#866](https://github.com/LimeBeck/arinn/issues/866), from application source
`611e067` plus this task's capture fixtures: Qt 6.12.0 on Arch Linux,
offscreen platform, software Qt Quick renderer, 1920 × 1080, DPR 1.
The matching 4K render is checked by the comparison tool but is not published.

| File | Capture state | Content |
| --- | --- | --- |
| `home.png` | `home-populated` | Continue and favorite tiles, Playfair content title, Hearth light and remote focus |
| `open-services.png` | `browser-list` | Fictional .example services, on-screen/sound marks, bookmark and close controls |
| `settings.png` | `settings-rows` | Current Appearance controls: light field, time-of-day lighting and ambient timeout |
| `library-folders.png` | `folder-picker-home` | Graphical folder picker with fictional places and subfolders |
| `music.png` | `music-room` | Production Music room with fictional station and album shelves from two sources |
| `now-playing.png` | `music-now-playing` | Production player with fictional artwork, track information and transport controls |

All clocks are fixed at 12:00. Services, artists, tracks and paths are synthetic;
monograms are neutral and placeholder artwork is original. No capture connects
to a daemon, provider, account, filesystem browser or live desktop. The folder
fixture implements the browser model without reading the disk. Music fixtures
supply data to the real room/player components without producing audio.

These images document implemented layout. They do not pass G-5.1, qualify
hardware, approve the design at TV distance or prove live integrations work.
The [design sources](../README.md) remain the visual contract. The wider,
earlier [current-ui baseline](../current-ui/README.md) and historical handoff/archive
assets retain their dates and are not changed by this refresh.

From the repository root:

```sh
cmake -S . -B build -G Ninja
cmake --build build --target arinn-ui-test
python scripts/ui-compare.py --build-dir build --output build/showcase-refresh \
  --states home-populated browser-list settings-rows folder-picker-home music-room music-now-playing
cp build/showcase-refresh/1080/home-populated.png docs/design/showcase/home.png
cp build/showcase-refresh/1080/browser-list.png docs/design/showcase/open-services.png
cp build/showcase-refresh/1080/settings-rows.png docs/design/showcase/settings.png
cp build/showcase-refresh/1080/folder-picker-home.png docs/design/showcase/library-folders.png
cp build/showcase-refresh/1080/music-room.png docs/design/showcase/music.png
cp build/showcase-refresh/1080/music-now-playing.png docs/design/showcase/now-playing.png
cp docs/design/showcase/home.png docs/user/assets/home.png
python scripts/build-site.py
python scripts/check-site.py
```

Without `--states`, the tool renders the full manifest. Inspect every refreshed
frame and its caption before committing. The website builder copies these PNGs
directly; the standalone handbook's Home copy must also stay synchronized.
Approved marks and historical sources remain unchanged. Documentation/captures
use CC-BY-SA-4.0 under existing REUSE annotations; bundled Onest and Playfair
Display retain OFL-1.1.
