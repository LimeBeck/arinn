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
| `ContentCard` | `text`, `subtitle`, `artwork` URL supplied by trusted shell models (a file or `image://arinn-assets/`), `running`, optional `showProgress` and normalized `progress`, optional `stack` of up to three `{letter, icon}` chips drawn overlapping instead of the monogram. The plate stays until the image is ready; artwork gets the handoff scrim (`rgba(10,11,13)` from .9 to 0 at 70% height). Bytes from Assets1 are decoded in C++ |
| `AppTile` | `text`, neutral `monogram`, `running` |
| `ProgressTrack` | Clamped finite `value` from 0 to 1; Ember represents progress only |
| `Skeleton` | Fixed-size pulsing placeholder; same default geometry as a content card |
| `AmbientGlow` | Home's light field: `source` artwork or `tint` identity colour, `lit`, `strength`. The dominant hue of a 24×24 decode of the artwork keys the handoff's three radial layers, painted once per hue at one-fifth resolution and scaled up; a 900 ms cross-fade follows focus after a 200 ms settle. No full-resolution art is blurred per frame |
| `StatusPane` | Explicit `kind` (empty/loading/error/unavailable), `title`, `description`, `actionText`; `actionRequested()` intent |

Interactive components inherit `enabled` and add `busy`. Consumers connect to
`activated`, not the underlying Qt `clicked` signal: busy controls retain focus
but do not emit action intents. Disabled controls cannot activate. There are no
success timers. Status panes hide their action during loading.

Focus ownership follows [ADR-0076](../adr/0076-shell-focus-ownership.md):
`Main.qml` decides which screen is the focused scope (`focusedScreen`), screens
move focus only inside themselves, and every move goes through
`Focus.take(item)`, which refuses hidden or disabled items. Home stays painted
beneath secondary screens, but Main disables it whenever another screen owns
focus, so catalog refresh callbacks and hover cannot steal that screen's input.
The production browser regression covers Home state changes while Open services
is displayed: Escape and Backspace return locally to Home without activating
the external Return target. The `focus-ownership`
test rejects any other `forceActiveFocus()` call.

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
`webShortcut`. Media entries (`kind: "media"`) may carry the provider's reported
`positionUs` and `durationUs`; Home derives the time left and the card progress
from them. An entry that stands for an open web service carries its `serviceId`.
`HomeView` limits Continue to one Activity plus four further cards (spec §5.1,
handoff 02); New availability and provider state stay with the caller. Optional
`currentServiceTitle`, `openServices` (`{id, title, letter, icon}`, the service on
screen first), `openShortcutIds` and `browserUnavailable` add the Open services
Continue cards, the navigation item and the unavailable shortcut treatment; they
default so a fixture that omits them is unchanged. The open-services summary
lists only services without a card of their own, matched by `serviceId`, never by
title ([#358](https://github.com/LimeBeck/arinn/issues/358)): "N more open" once
a service has a card, "Open: N" otherwise, hidden with one open service or none
left over. The hero describes the focused item
([#364](https://github.com/LimeBeck/arinn/issues/364)). `downloadActive` and
`downloadProgress` drive the status-area download chip; `ambientSuppressed` turns
the light field off while an Activity background is shown. A media entry whose
`serviceId` names an open service is that service's card
([ADR-0096](../adr/0096-browser-media-session-association.md)): it merges into the
current-service card, or becomes a `tab` card that emits `tabRequested`.
Play/pause media keys on it emit `mediaPlayPauseRequested` with the session ID. QML neither aggregates providers nor persists these arrays.
`returnActivityId` identifies the return target independently of focus.
`homeState` is ready/empty/loading/error/unavailable, accompanied by
`statusTitle` and `statusDescription`; `clockText` is display-only.

In production, `Main.qml` passes the browser catalog's tabs as `openServices` and
`DownloadCatalog.downloading`/`downloadProgress` (bytes when every active download
knows its size, else the mean of the known fractions, else unknown). Continue media
without session artwork take the backdrop of the library item the session plays
through `MediaCatalog.libraryContent` and `LibraryCatalog.artworkFor`. Mute and
Wi-Fi need platform state that no current interface exposes
([#367](https://github.com/LimeBeck/arinn/issues/367)).

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

`Backend` constructs the Return card from the daemon's opaque return target.
The optional [Return association consumer](ui-web-shortcuts.md#home-activity-identity-and-visual-treatment)
replaces it with the current-service card only while one validated browser match
agrees with the live Home epoch/target. Missing or stale evidence restores Return;
independent media sessions remain visible. Both presentations use daemon Back. It
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
Application tiles use existing local/theme icons from `Application1`
([ADR-0059](../adr/0059-application-icon-references.md)). C++ keeps references in
an in-process registry (4096 entries) and exposes opaque `image://arinn-icons/`
handles. A four-worker image pool opens only regular local files, with 2 MiB
compressed and 4096×4096 source-size limits; decoded icons are at most 256×256.
Theme search is bounded (16 themes, 16 roots, 512 directories per index list,
4096 candidates and 4 MiB candidate image bytes), prefers scalable/128px entries, follows declared inheritance and falls back to hicolor and
local pixmaps. SVG external references, embedded images, CSS and DTDs are rejected.
Unreadable, unsupported or missing icons retain a monogram on a deterministic
muted color chip. Loaded icons occupy the full chip bounds with preserved aspect
ratio and a rounded software-compatible mask; transparent marks reveal a neutral
white-10% backing, never the fallback identity color. The circular rounded-rect
clip matches the chip geometry. The fallback remains until the Canvas request
has decoded pixels for the current source (the separate Image metadata request
is insufficient); load failures retain it. Source changes unload the old Canvas
image and reject obsolete readiness. Application color identity
excludes the provider generation.
No logos are bundled and no icon request goes to a provider or network.

Focused surfaces retain their white overlay ring and existing lift/scale. Layered
rounded plates add a bounded soft shadow and low-opacity identity-colored glow,
including the software renderer; fallback content cards keep the focused light
plate visible. Actual artwork remains required for artwork-backed cards.

## Handoff visual comparison

[Issue #120](https://github.com/LimeBeck/arinn/issues/120) adds a local comparison
of the fixture-driven screens with the design handoff. It is evidence for the
[first UI increment](../../ROADMAP.md#first-ui-increment) and, later, for a design
review of G-5.1. It does not pass G-5.1, AC-14, or any Phase 5 gate.

The manifest is [`tests/ui/visual/manifest.json`](../../tests/ui/visual/manifest.json).
Each entry is one screen state. `reference` names a file in
[`docs/design/handoff/screenshots/`](../design/handoff/screenshots/): the 22 main
`*.jpg` files, the `os-*.png` Open services states, and the `in-*.png` address
states. A state with no handoff file has `reference` null. A handoff file whose
screen is not implemented has `render` false and a reason. The address and
search states are in that last group. The comparison does not edit those
screens.

```sh
cmake -S . -B build -G Ninja
cmake --build build --target arinn-ui-test
python scripts/ui-compare.py --check
python scripts/ui-compare.py
```

`--check` only confirms that the manifest and the handoff directory name the
same files. The full command renders every `render: true` state with
`arinn-ui-test` and [`tests/ui/visual/capture.qml`](../../tests/ui/visual/capture.qml)
on the offscreen platform and the software scene graph. It writes PNGs and
`report.html` under `build/ui-compare/`. The capture request is generated in
that output directory. The tool does not rewrite a tracked file. Those files
are generated and are not committed. Nothing is added to CI.

1080p is a 1920×1080 framebuffer. The tool then sets `QT_SCALE_FACTOR=2` and
renders again. The offscreen platform ignores that variable, so the grab stays
1920×1080. The tool scales the logical 1920×1080 canvas by 2 and saves a
3840×2160 image. That image is the 1080p layout scaled up. It is not a device
pixel ratio of 2, and it does not exercise pixel-ratio effects. The report
places each render next to its handoff file and overlays them with a difference
blend. A mean absolute channel error, sampled on a 64×36 grid, is a drift hint.
It is not a threshold: handoff artwork is placeholder, service marks in the
product are monograms, and some references are not 1920×1080.

The capture uses the same fixture screens as the Qt Quick tests. It does not
launch applications, talk to the daemon, or drive a session. Operator tests
were not run. A sample of the generated summary belongs in the pull request;
the images themselves stay local.

### Home icon/focus evidence (#207)

The shared gallery and `tests/ui/visual/home-icons.qml` were captured and inspected
at 1920×1080 and 3840×2160 (logical canvas scaled 2×): current service, local mark,
missing icon, focused app/fallback and two independent same-title media sessions.
The optional capture uses the same `CaptureRequest` module generated by
`scripts/ui-compare.py`; pass its directory with `arinn-ui-test -import` and use
`-input tests/ui/visual/home-icons.qml`. It saves `home-service.png`,
`home-icon-focus.png` and `home-fallback-focus.png` in the requested output folder.
Fixtures are synthetic; the local mark is Arinn's existing approved asset.

Comparison used the approved `os-h1-home-over-service.png` reference. The user's
separate Library screenshot could not be materialized in this executor, so no
pixel comparison with that image is claimed. Activity capture/blur (#149),
full KDE/Wayland integration and physical TV readability were not tested here.

## Shell close action

Global navigation emits `closeShellRequested()` from its final Close Arinn row.
`Main.qml` handles it with `Window.close()`, the same graceful path as Alt+F4.
The action is local and available in degraded Home states; it does not call a
provider, a power API or the daemon. Navigation preserves Back and focus traps.

### Home handoff increment evidence (#358, #364–#368)

[`evidence/home-handoff/`](evidence/home-handoff/) holds 1920×1080 offscreen
captures (software renderer, Qt 6.8.2, synthetic fixtures and the gallery's
fixture artwork) in Russian (`ru-*`) and English (`en-*`): a focused Continue
media card with the hero time line, progress, backdrop scrim, download chip and
light field; the focused activity; a focused favorite; and the open-services card
with stacked chips. They were captured from `HomeView` alone, not the production
window. No KDE session, TV or physical readability check was performed.
