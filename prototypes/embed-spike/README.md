<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Embedded player spike (#433)

A standalone fullscreen Qt window that hosts a media player through a private
nested Wayland compositor (`QtWaylandCompositor`, socket `arinn-player-0`) and
draws a QML overlay over it. It is a measurement prototype, not product code.

```sh
cmake -S prototypes/embed-spike -B build/embed-spike -G Ninja -DCMAKE_BUILD_TYPE=Release
cmake --build build/embed-spike
python3 prototypes/embed-spike/measure.py build/embed-spike/arinn-embed-spike CLIP baseline|embedded [SECONDS]
```

`results.jsonl` holds the HTPC measurements of 2026-10-10 (AMD Renoir, Mesa 26.2.4,
Qt 6.12, mpv 0.41, synthetic 4K HEVC clips).
