<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# QML component foundation

The first component subset of the [UI workstream](../../ROADMAP.md#parallel-ui-work-alongside-phase-1)
implements presentation primitives from [specification §15](../specifications/arinn-specification.md#15-design-contract).
Tracked in [issue #10](https://github.com/LimeBeck/arinn/issues/10), extended by
the Home/navigation preview below and the [production binding](#production-home-and-applications).
These increments do not constitute full Phase 5 acceptance (AC-02, AC-14, G-5.1).

## Ownership and boundaries

`apps/arinn-shell/ui` owns reusable QML presentation components. The gallery and
`tests/ui` supply explicit local fixtures. The production shell binds the components
to its C++ transport as described below. Components make no IPC calls, know no providers, and execute no
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
Material Symbols, full poster/row styling, shimmer and the complete modal/sheet
focus stack. The Home/navigation subset is described below. The current subset uses a pulse for loading and neutral
status glyphs. Full handoff fidelity and 3-metre TV readability are not claimed.

## Font provenance

Onest is bundled unchanged from the Google Fonts `ofl/onest` directory:
[font source](https://github.com/google/fonts/tree/main/ofl/onest),
[upstream project](https://github.com/googlefonts/onest).
Copyright 2021 The Onest Project Authors. License: SIL Open Font License 1.1,
in `LICENSES/OFL-1.1.txt`, with the binary annotated in `REUSE.toml`.

## Home and global navigation preview

[Issue #15](https://github.com/LimeBeck/arinn/issues/15) adds `HomeView` and
`HomeRow`, exercised by the standalone `HomeGallery.qml`. That fixture preview
is independent of the production binding below. Other top-level routes show explicit preview
placeholders; no Library, Downloads, Applications or Settings backend is implied.
Power emits an intent only; it never opens a destructive confirmation or performs
a session action.

`HomeView` accepts immutable `continueEntries`, `favoriteEntries` and `newEntries`
arrays. Each entry has a unique opaque `id`, `title`, and optional `subtitle`,
`artwork`, `monogram`, `running`, `progress`, `enabled`, `busy` and
`webShortcut`. The caller owns the Continue projection (one Activity plus up to
four resumable entries), New availability and provider state. Optional
`currentServiceTitle`, `openServiceCount`, `openShortcutIds` and
`browserUnavailable` add the Open services Continue card, the navigation item
and the unavailable shortcut treatment; they default so a fixture that omits
them is unchanged. QML neither aggregates providers nor persists these arrays.
`returnActivityId` identifies the return target independently of focus.
`homeState` is ready/empty/loading/error/unavailable, accompanied by
`statusTitle` and `statusDescription`; `clockText` is display-only.

Actions emit `entryRequested(id)`, `returnRequested(id)`, `retryRequested()`,
`sectionRequested(route)` and `powerRequested()`. The gallery displays intent
feedback; no timeout or animation changes backend state. Production model binding
and operation feedback now use the existing daemon contracts as described below.

Local input uses arrows, Enter, Menu, Home, Esc and Backspace. Home closes
navigation and focuses the return Activity when present. Back closes navigation,
returns a section to Home, requests Return from Home with an Activity, or opens
navigation without one. Editable text consumes Backspace normally. No global
key registration is added. Navigation traps focus and restores its opener; if
that item disappears it restores the nearest surviving row item. Snapshots keep
selection by ID across reordering and choose the previous index after removal.
Empty rows fall back to a neighbouring row, then the menu button. New scrolls
into view locally; horizontal rows reveal the focused item and suppress hover
focus for 500 ms after scrolling. Pointer movement and arrows switch input modes.

```sh
build/apps/arinn-shell/arinn-ui-gallery --home
build/apps/arinn-shell/arinn-ui-gallery --home --nav
build/apps/arinn-shell/arinn-ui-gallery --home --state unavailable
QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software build/apps/arinn-shell/arinn-ui-gallery --home --4k --capture /tmp/home-4k.png
```

`--state` accepts populated (default), empty, loading, error and unavailable.
Captures use original geometric fixture artwork and neutral application names.
The comparison target is the handoff Home/nav layout, not all 22 screens:
logical safe area, hero, Continue/Favorites/New positioning, navigation panel,
focus visibility and uniform scaling. Activity backdrop capture, material icons,
artwork lighting, real TV readability and the full Phase 5 design gate remain
outside this increment. Operator tests were not run: this preview has no platform
binding and permission to drive the real desktop was not requested.

Local verification on 2026-09-28 (Qt 6.11.2): CMake build and CTest passed
(42 Qt Quick results, including the existing components). Rust fmt, clippy,
tests and example builds, the Phase 1 private-bus suite, KWin JavaScript tests,
documentation/architecture/ADR/version checks, REUSE and dependency license
checks passed. Populated Home and navigation were compared with the handoff;
all five fixture states were captured at 1080p and 4K, with representative captures
inspected for scaling, readable status text and clipping. This validates the
presentation subset, not full design fidelity or production navigation.

The pointer regression test also checks first-move hover after D-pad input and
hover within the same card after scroll suppression expires. It failed before
deferring the component hover check until the parent input-mode update; it passes
with that fix. No pointer click is needed to move focus.

The Home test fixture parks the synthetic pointer in a non-interactive area
before creating each view. Qt Quick Test retains pointer coordinates across
fixtures; otherwise a queued hover can override the initial keyboard focus.
Two regression cases recreate Home after hovering different cards and check
initial focus, arrow navigation and previous-item focus after removal. Both
failed without pointer reset and passed with it (Qt 6.11.2, 2026-09-28,
[issue #31](https://github.com/LimeBeck/arinn/issues/31)). This changes test
isolation only; production input behaviour and acceptance gates are unchanged.
The complete offscreen Qt Quick suite passed 20 consecutive runs after the fix.

## Production Home and Applications

[Issue #42](https://github.com/LimeBeck/arinn/issues/42) binds the shared logical
canvas, Home and navigation to `Main.qml`. The shell bundles the same presentation
resources and Onest font as the gallery. The independent session lifecycle guard
still controls window visibility. Home starts in a degraded state when the daemon
is absent and reconnects asynchronously.

`Backend` constructs the Return card from the daemon's opaque return target. It
reads all `Providers` and `Application1.GetApplications` pages through the daemon,
checks snapshot identity, generation, revision and pagination cycles, and exposes
launchable/disabled application entries. The Home shortcut row is labelled
Applications; it does not claim these entries are persisted favorites. Applications
also has a dedicated route, including the full paged list of open windows and
`Activate` actions; this list is not truncated by the five-item `HomeItems` projection.
Applications are grouped into Online, Games, Media and Other rows, in that order
(spec §5.5, [#63](https://github.com/LimeBeck/arinn/issues/63)). The group comes
from the provider's application record; `Backend` maps unknown groups to Other
before QML, which only filters by the known group names. Empty groups are hidden.
Up/Down moves between visible group rows and the open-windows row, keeping the
column position where possible; each screen remembers the focused row, and the
section scrolls so the focused row stays on screen.
No provider-specific routing or QML business logic is
introduced. Favorites persistence, web shortcuts, New, Library, Downloads, Settings
and power actions remain separate work; unsupported sections explicitly say they
are unavailable.

Launch intents call `Application1.Launch` and observe `Operation1.Get`. A successful launch closes Home: the daemon asks the platform adapter to
dismiss Home without returning to the previous Activity, so the new window is not
opened underneath the shell. Pending
entries retain focus, show the busy treatment and suppress duplicate launch
requests. Completion and errors come from the operation, never from animation
timers. A failed provider snapshot does not remove other providers' valid entries.
Unchanged poll results do not emit list replacement signals, preserving delegate
identity and local focus. Daemon owner loss invalidates return and application
entries; stale replies cannot replace a newer connection's state.

The private-bus catalog tests cover multiple application pages, disabled entries,
unchanged snapshots, generation/revision mismatches, cyclic tokens, running-window paging, partial
provider failure, empty/error states and daemon disappearance/reappearance.
Qt Quick tests cover Applications navigation, activation intents, removal and
focus restoration. The production-shell private-bus test checks the actual Home
binding and local navigation while a demo operation times out.

Verification is recorded in the pull request. Real-desktop operator tests, target
TV readability, global Home repetition while already visible, the 22-screen design
gate and full AC-02/AC-18 acceptance are not claimed by this increment. The current
Status contract does not distinguish a repeated global Home event from an ordinary
state refresh; local Home is covered by the QML navigation suite.

Local verification on 2026-09-28 (Qt 6.11.2): CMake and all three CTest suites
passed, as did the complete Phase 1 private-bus suite (including the desktop
fixtures), Rust fmt/clippy/tests/builds, JavaScript adapter tests,
documentation/architecture/ADR/version checks, REUSE and dependency licenses.
The production Home capture at 1080p and shared Home gallery at 4K were inspected.
Hosted CI results are recorded in the pull request; no operator or TV result is
implied by these isolated checks.

### Brand mark

`BrandLockup` combines the existing approved Arinn SVG mark with the Onest
wordmark. `BrandButton` keeps a dark focus plate and the shared focus ring so the
white mark remains visible. Home and the navigation panel use this lockup.
The shell, gallery and UI test targets embed the original
[`arinn-mark.svg`](../design/handoff/assets/arinn-mark.svg) unchanged under
`qrc:/brand/`; its existing CC-BY-SA-4.0 attribution remains applicable.
Application tiles retain neutral placeholders: `Application1` currently provides
no application icon field, and this change adds no third-party logos.
