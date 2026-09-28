<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# QML component foundation

The first component subset of the [UI workstream](../../ROADMAP.md#parallel-ui-work-alongside-phase-1)
implements presentation primitives from [specification §15](../specifications/arinn-specification.md#15-design-contract).
Tracked in [issue #10](https://github.com/LimeBeck/arinn/issues/10). This is not a
complete Home screen or Phase 5 acceptance (AC-02, AC-14, G-5.1).

## Ownership and boundaries

`apps/arinn-shell/ui` owns reusable QML presentation components. The gallery and
`tests/ui` supply explicit local fixtures. The existing shell and its C++ transport
are unchanged. Components make no IPC calls, know no providers, and execute no
commands (R-01–R-05, AC-12). Screen integration owns mapping typed C++ presentation
models into these properties and routing emitted actions to the daemon.

| Component | Presentation contract |
| --- | --- |
| `Theme`, `UiText` | Shared colours, radii, timing, font and named typography roles; Onest with tabular numerals |
| `LogicalCanvas` | Centred 1920×1080 stage, uniform `min(width/1920, height/1080)` scale |
| `Surface` | Focus, press, disabled and busy treatment; focus ring above all artwork; `activated()` intent |
| `ActionButton` | Content-sized pill, `text`, optional `destructive` tint |
| `ContentCard` | `text`, `subtitle`, `artwork` URL supplied by trusted shell models, `running`, optional `showProgress` and normalized `progress` |
| `AppTile` | `text`, neutral `monogram`, `running` |
| `ProgressTrack` | Clamped finite `value` from 0 to 1; Ember represents progress only |
| `Skeleton` | Fixed-size pulsing placeholder; same default geometry as a content card |
| `StatusPane` | Explicit `kind` (empty/loading/error/unavailable), `title`, `description`, `actionText`; `actionRequested()` intent |

Interactive components inherit `enabled` and add `busy`. Consumers connect to
`activated`, not the underlying Qt `clicked` signal: busy controls retain focus
but do not emit action intents. Disabled controls cannot activate. There are no
success timers. Status panes hide their action during loading.

Screens own focus scopes, explicit neighbours, restoration and input-mode state.
They pass `pointerMode` and `hoverFocusEnabled` to controls; the latter must be
false during the 500 ms hover suppression after a scroll. Components never decide
global navigation or call providers for a focus change. The gallery demonstrates
only a small local focus graph, not the complete screen/modal stack.
`dpadRequested()` lets the containing screen switch input mode even when
`KeyNavigation` consumes the arrow event locally.

## Development and verification

```sh
cmake -S . -B build -G Ninja
cmake --build build
ctest --test-dir build --output-on-failure
build/apps/arinn-shell/arinn-ui-gallery
build/apps/arinn-shell/arinn-ui-gallery --4k
QT_QPA_PLATFORM=offscreen build/apps/arinn-shell/arinn-ui-gallery --capture /tmp/arinn-ui.png
```

The gallery opens a regular window and requires no daemon or session bus. It uses
original geometric artwork and neutral monograms. Its actions update explanatory
fixture text only. `--capture` saves a PNG and exits; `--4k` uses 3840×2160.
The test runner uses the offscreen platform and software rendering in CTest/CI;
it does not touch KWin, real applications, screen lock or session services.

Tests cover keyboard/pointer activation, disabled and busy action guards, focus
ring layering, pointer lift, hover suppression, long Cyrillic button labels,
font loading, progress bounds, all four status kinds and logical canvas scaling.

Local verification on 2026-09-28 (Qt 6.11.2): CMake build passed; the
[Qt Quick suite](../../tests/ui/tst_components.qml) passed all 18 cases, including
artwork pixel rendering and navigation during loading. The Phase 1 private-bus
integration suite, documentation/architecture/version checks and REUSE passed.
Gallery captures at 1920×1080 and 3840×2160 were inspected for clipping, focus
ring placement, typography and layout. This was a component layout check, not a
comparison of all 22 handoff screens. GPU rendering, target-TV usability and
operator tests were not run; CI has not run for this change.

Remaining design work includes artwork-dependent glow/elevation, the light field,
Material Symbols, poster/row variants, shimmer, Home/global navigation and full
focus-scope restoration. The current subset uses a pulse for loading and neutral
status glyphs. Full handoff fidelity and 3-metre TV readability are not claimed.

## Font provenance

Onest is bundled unchanged from the Google Fonts `ofl/onest` directory:
[font source](https://github.com/google/fonts/tree/main/ofl/onest),
[upstream project](https://github.com/googlefonts/onest).
Copyright 2021 The Onest Project Authors. License: SIL Open Font License 1.1,
in `LICENSES/OFL-1.1.txt`, with the binary annotated in `REUSE.toml`.
