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
| `Theme`, `UiText` | Shared colours (the Hearth light palette: Charcoal `#0F1115`, warm day text, `chip`, `mat`, `rim`, `hearth` and the pool centre, plus `nightText1..3` and `eveningText3`), radii, timing, fonts and named typography roles. Onest for system text and Ysabeau for `hero` (96/600), `detailTitle` (104/600) and `display` (220/600, the Ambient clock face), with tabular numerals and, on content roles, lining figures. Each weight is applied to the font's `wght` axis (see *Font weights*). Eyebrows are sentence case (26/500). Page titles are Onest 56/600 with no negative tracking. `fitSteps` (`[[size, lines], …]`) fits a content title: the first step whose rendered text is not elided wins, and `fitStep` reports it ([ADR-0136](../adr/0136-hearth-light-visual-direction.md), [#722](https://github.com/LimeBeck/arinn/issues/722)) |
| `FontWeightAxis` | Put inside a plain `Text` or `TextInput` in Onest that sets `font.weight`; applies the weight to the `wght` axis as `UiText` does. Draws nothing |
| `LogicalCanvas` | Centred 1920×1080 stage, uniform `min(width/1920, height/1080)` scale |
| `Surface` | Focus, press, disabled and busy treatment; focus ring above all artwork; `activated()` intent |
| `ActionButton` | Content-sized pill, `text`, optional `destructive` tint |
| `ContentCard` | `text`, `subtitle`, `artwork` URL supplied by trusted shell models (a file or `image://arinn-assets/`), `running`, optional `showProgress` and normalized `progress`, optional `stack` of up to three `{letter, icon}` chips drawn overlapping instead of the monogram. The plate stays until the image is ready; the artwork is centre-cropped to the card like `Image.PreserveAspectCrop` (the canvas helper `ui/artwork.js`, shared with the Downloads poster), the previous image is released when the URL changes, and the artwork gets the handoff scrim (`rgba(10,11,13)` from .9 to 0 at 70% height). Title (two lines) and subtitle (one line) wrap with `Text.Wrap`, so a word wider than the column breaks and elides instead of running past the card; a narrow card (under 300 px, the New row's posters) sets the title at 24 px with 18 px insets ([#655](https://github.com/LimeBeck/arinn/issues/655)). Bytes from Assets1 are decoded in C++ at the card's size: the Image `sourceSize` and the Canvas copy both request it, the decoder covers it without upscaling, and no image exceeds 4096 pixels a side. A request cancelled before a pool thread takes it, such as a poster scrolled away, finishes without a daemon call ([#611](https://github.com/LimeBeck/arinn/issues/611)) |
| `AppTile` | `text`, neutral `monogram`, `running`. The name is 26 px on up to two lines, wrapped with `Text.Wrap` and then elided, so "Site or search" shows whole; this departs from handoff 01, which elides to one line (owner decision on [#656](https://github.com/LimeBeck/arinn/issues/656), 2026-11-01) |
| `ProgressTrack` | Clamped finite `value` from 0 to 1; Ember represents progress only |
| `Skeleton` | Fixed-size pulsing placeholder; same default geometry as a content card |
| `AmbientGlow` | Hearth light (ADR-0136): a black ceiling over the top 44% (50% at the top edge), a warm floor ellipse and a pool under the focused item. `source` is artwork, else `tint` is the identity colour; `lit` and `strength` (1 Home, 0.6 Applications, 0.6 Library, 0.5 detail) fade the field. The pool's hue is the dominant hue of a 24×24 artwork decode, chroma 0.10 reduced to 0.065 at hue 240°. Floor and pool are painted once into a one-fifth-resolution canvas and scaled up; a 900 ms cross-fade follows focus after a 200 ms settle. No full-resolution art is blurred. `followsTime` with `clockText` or `hour` selects day, evening (19:00, light 85%, veil 6%) and night (23:00, light 60%, veil 22%). Day text is `oklch(0.95/0.80/0.66 0.008 75)` on Charcoal. At night, Theme text becomes text-1 `oklch(0.97 0.006 75)`, text-2 `0.84` and text-3 `0.761`. Evening keeps the day titles and raises hints to text-3 `oklch(0.778 0.008 75)`. The brief's night text-3 `0.71` is about 4.07:1 at the brightest floor-and-pool pixel under the 22% veil; `0.761` is the smallest thousandth of OKLCH L that holds 4.8:1 there, and that step above the brief is not confirmed by the owner. Day text-3 is about 2.95:1 at the same point in the evening, so hints rise to `0.778`, the smallest thousandth that holds 4.5:1. Floor and pool centres are the `Theme` tokens `hearthLightness` 0.44, `hearthChroma` 0.07, `hearthHue` 55, `poolLightness` 0.50, `poolChromaWarm` 0.10 and `poolChromaCool` 0.065. This light does not choose text colours; Theme does, from the period the light publishes. Settings and Downloads host the warm veil only: the floor and the pool stay off, and the veil is not removed when those screens open. The player does not host it, so video stays uncovered. Applications use strength 0.6, with the Library; the prototype's `updateLight()` value 0.8 is not used. The cases live in `tests/ui/hearth` (`ctest -R '^hearth-light$'`), a separate process from `ui-components`, which aborts on `Qt.Key_MediaPause` in `tst_home.qml` ([#733](https://github.com/LimeBeck/arinn/issues/733)). Light under text stays at or below L 0.48, measured by compositing those same stops |
| `LineIcon` | The single shared line-icon component: `name` (a Material Symbols name from the handoff), `color`, size from `width`/`height` on a 24-unit grid. Icons are drawn in code on a `Canvas`; the shell ships no icon font, so rendering does not depend on installed fonts or the Qt version. Today: the on-screen keyboard icons and Open services `bookmark_add`/`bookmark_added`. A new handoff icon (such as the Close icon of [#565](https://github.com/LimeBeck/arinn/issues/565)) is added here, not as a glyph or a second component |
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
the external Return target. Library and Downloads stay visible and enabled under
Home's navigation menu, so `Focus.take` alone cannot protect the menu: Main binds
their `ownsFocus` to `focusedScreen`, and their data-change handlers move focus
only while it is true. When it turns true again they restore the selected row or
the initial item (#608, `dataUpdatesUnderTheMenuKeepItsFocus`). The `focus-ownership`
test rejects any other `forceActiveFocus()` call.

Screens own focus scopes, explicit neighbours and restoration. The input mode is
shell-wide ([ADR-0095](../adr/0095-shell-input-mode-ownership.md)): the
`InputMode` singleton holds pointer/D-pad mode and the 2 s key cooldown, screens
bind `pointerMode` to it and report keys and hover positions, and
`InputModeSource` in `Main.qml` reports every key press and pointer movement in
the shell window and hides the cursor in D-pad mode. Screens pass `pointerMode`
and `hoverFocusEnabled` to controls; the latter must be false during the 500 ms
hover suppression after a scroll. Components never decide
global navigation or call providers for a focus change. The gallery demonstrates
only a small local focus graph, not the complete screen/modal stack.
`dpadRequested()` reports the key to `InputMode` even when `KeyNavigation`
consumes the arrow event locally; in the shell `InputModeSource` sees it first.

## Development and verification

```sh
cmake -S . -B build -G Ninja
cmake --build build
ctest --test-dir build --output-on-failure
build/apps/arinn-shell/arinn-ui-gallery
build/apps/arinn-shell/arinn-ui-gallery --4k
QT_QPA_PLATFORM=offscreen build/apps/arinn-shell/arinn-ui-gallery --capture /tmp/arinn-ui.png
```

The gallery is a component catalogue: `Gallery.qml` and, with `--keyboard`, the
on-screen keyboard over Site or search. It does not render screens or their
states; those come only from the `tests/ui/visual` fixtures (see
[Handoff visual comparison](#handoff-visual-comparison), #567), so there is one
set of screen fixtures.
The gallery opens a regular window and requires no daemon or session bus. It uses
original geometric artwork and neutral monograms. Its actions update explanatory
fixture text only. `--capture` saves a PNG and exits; `--4k` uses 3840×2160.
Any other option, including the removed screen flags, prints an error and
exits with status 2.
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

Ysabeau, the content-title face of the Hearth light direction
([ADR-0136](../adr/0136-hearth-light-visual-direction.md), D-2), is bundled
unchanged as `ui/fonts/Ysabeau.ttf` from the Google Fonts `ofl/ysabeau`
directory (`Ysabeau[wght].ttf`, SHA-256
`aa6df9f1f6cb1180f7c00fffb5ae2734ba66aa5f58defd173a9247ca11dffcf4`):
[font source](https://github.com/google/fonts/tree/main/ofl/ysabeau),
[upstream project](https://github.com/CatharsisFonts/Ysabeau).
Copyright 2023 The Ysabeau Project Authors. License: SIL Open Font License 1.1,
in `LICENSES/OFL-1.1.txt`, with the binary annotated in `REUSE.toml`. The owner
approved bundling this face; the pull request that adds the file still needs
the owner's review. It is a variable font with a `wght` axis and is weighted
through the same `Theme.applyWeight` path as Onest. Its default figures are
old-style, so the content roles turn on the `lnum` feature. The `display`
role is this face at 220/600 for the Ambient clock. The Ambient screen still
draws its handoff clock at 160/500 until that screen's own task; the settings
integer readout keeps Onest 112/700 and does not use `display`.

## Font weights

Onest is a variable font: one face with a `wght` axis from 100 to 900 and
named instances. Ysabeau, used by the content roles, is weighted the same way. Qt does not match those instances by `font.weight` for an
application font. In the Qt 6.10.2 dev container and in CI on Arch with
Qt 6.12.0 (offscreen, fontconfig), every `font.weight` and
`font.styleName` rendered as the default 400, while
`font.variableAxes` (Qt 6.7 and newer, within the Qt 6.8 floor of
[ADR-0104](../adr/0104-qt-floor-and-prebuilt-ci-images.md)) does change the
rendering ([#546](https://github.com/LimeBeck/arinn/issues/546)).

So the shell keeps `font.weight` as the way to set a weight and copies it to
the axis: `Theme.applyWeight(item)` sets `font.variableAxes.wght` to
`font.weight` whenever they differ. `UiText` calls it whenever its font
changes, so a role's weight, a weight set on an instance and a later change
all render. A plain `Text` or `TextInput` in Onest with a weight carries a
`FontWeightAxis` child that does the same. A 400 text gets no axis. On a Qt that matches the
named instances itself, the axis names the same design coordinates as the
matched instance; that case has not been observed yet. Text in the system font (such as the
library's "!" glyphs) is not affected.

[`tst_font_weights.qml`](../../tests/ui/tst_font_weights.qml) draws the same
string at 400 and at 500, 600 and 700 through `UiText` and `FontWeightAxis`
and asserts a wider advance and more ink. It also logs whether Qt itself
matches the family by `font.weight`.

## Home and global navigation preview

[Issue #15](https://github.com/LimeBeck/arinn/issues/15) adds `HomeView` and
`HomeRow`, exercised by the `tests/ui/visual` fixtures (the standalone
`HomeGallery.qml` preview was removed in #567). That fixture preview
is independent of the production binding below. Other top-level routes show explicit preview
placeholders; no Library, Downloads, Applications or Settings backend is implied.
Power emits an intent only; it never opens a destructive confirmation or performs
a session action.

`HomeView` accepts immutable `continueEntries`, `favoriteEntries` and `newEntries`
arrays. Activating a New card emits `libraryEntryRequested(id)` for the existing library detail; other rows retain their established intents. Focusing the New row moves the rows up (`homeOffset`, at most 360 px) and fades the hero; a row fades out as its on-screen top (the animated translation, not the target offset) comes within 60 px of y 120, below the menu button and the status area, is gone at that line and, while not fully shown, takes no pointer input (`HomeRow.pointerBlocked` swallows clicks, hover and wheel; D-pad focus still reaches it). The menu button and the status area are stacked above the rows, so a click on the header always reaches them and no card slides under the header during or after the transition ([#659](https://github.com/LimeBeck/arinn/issues/659)). The caller supplies the bounded, sorted recent-library projection (ADR-0120). Each entry has a unique opaque `id`, `title`, and optional `subtitle`,
`artwork`, `monogram`, `running`, `progress`, `enabled`, `busy` and
`webShortcut`. Media entries (`kind: "media"`) may carry the provider's reported
`positionUs` and `durationUs`; Home derives the time left and the card progress
from them. An entry that stands for an open web service carries its `serviceId`.
A `HomeRow` takes over a new `entries` array only when its content differs, and
keeps its cards by position, updating them in place; a changed entry, such as
a playing session's position, does not recreate the row's cards (#441).
A row paints only between its own edges, which are Home's safe edges (x120 to
x1824, handoff 01). The viewport keeps a 48 px margin on each side for the
focus glow, lift and scale, and each card is clipped at the safe edges. Only
the focused card, while its body is inside the safe area, paints into that
margin, and only its focus treatment (glow and focus scale) ends up there. A neighbour, or a focused card flicked
across an edge, is cut at the edge. A card wholly outside the safe area is not
painted and takes no pointer focus or clicks, though D-pad focus still reaches
it. Scrolling to a focused card keeps its body inside the safe right edge. So
no card peeks past x1824, and the focus glow is not cut (#564).
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
([#364](https://github.com/LimeBeck/arinn/issues/364)). It sits in a fixed 240 px
box: the title is Ysabeau and fits in steps, 96 px on one line or 68 px on two,
so the Continue row does not move
([#722](https://github.com/LimeBeck/arinn/issues/722)). The brief's y 150 would
overlap the row, which is still at y 372; the box starts at y 132 so it ends
on that row. `downloadActive` and
`downloadProgress` drive the status-area download chip. `muted`, `wifi` and
`wifiLevel` (1–4, or 0 when unknown) show the mute and Wi-Fi icons between the
chip and the clock, only while known; `network` (`wired` or `disconnected`)
shows the cable or the crossed-out Wi-Fi icon in the Wi-Fi icon's place
([#408](https://github.com/LimeBeck/arinn/issues/408)); `ambientSuppressed` turns
the light field off while an Activity background is shown. A media entry whose
`serviceId` names an open service is that service's card
([ADR-0096](../adr/0096-browser-media-session-association.md)): it merges into the
current-service card, or becomes a `tab` card that emits `tabRequested`.
Play, Pause and the toggle key on it emit `mediaActionRequested` with the session ID and `play`, `pause` or `playPause`. QML neither aggregates providers nor persists these arrays.
`returnActivityId` identifies the return target independently of focus.
`homeState` is ready/empty/loading/error/unavailable/updated, accompanied by
`statusTitle` and `statusDescription`; `clockText` is display-only.

In production, `Main.qml` passes the browser catalog's tabs as `openServices` and
`DownloadCatalog.downloading`/`downloadProgress` (bytes when every active download
knows its size, else the mean of the known fractions, else unknown). Continue media
without session artwork take the backdrop of the library item the session plays
through `MediaCatalog.libraryContent` and `LibraryCatalog.artworkFor`. Mute,
Wi-Fi and connectivity come from `SystemStatus`, the shell's reader of the daemon's
`SystemStatus1` ([ADR-0100](../adr/0100-system-status.md),
[#387](https://github.com/LimeBeck/arinn/issues/387)).

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
cmake --build build --target arinn-ui-test
python scripts/ui-compare.py
```

The Home states are the `home-*` entries of
[`tests/ui/visual/manifest.json`](../../tests/ui/visual/manifest.json):
populated, navigation, empty, loading, error and unavailable among them. The
1080p and 3840×2160 renders are written to `build/ui-compare/`.
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
wordmark. Home and the navigation panel use this lockup. `BrandButton`, Home's
menu button, shows the bare lockup at rest, with a 36 px mark at x120 as in
handoff 01. Focus adds a translucent plate, its border and the shared focus
ring around it (owner decision on #564, 2026-11-01).
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
Fixtures live only in `tests/ui/visual/capture.qml`. They use fictional
services and titles on `.example` addresses and fix the clock (`clockText`
"12:00", daytime, on Home, player and Open services). `home-evening` is
20:30 and `home-night` is 23:30, the middle of each period. `ui-compare` also sets
`ARINN_CAPTURE_STILL_ANIMATIONS`. The test runner (`tests/ui/main.cpp`) then
takes animation time, which also drives QML `Timer`s, off the wall clock: during
a capture it moves only when `capture.qml` runs it, by exactly a state's
`animationMs`. By default that is 50 400 ms, a whole period of every looping
animation (900, 1400 and 800 ms), so transitions have finished and loops are at
their start. Transition states (`browser-closing`, `launch-opening`, `launch-slow`,
`launch-starting`, `launch-stopping`, `settings-saving`) run a few loop
periods plus the phase their handoff frame shows (or the same mid-bar when
there is no handoff), so they are captured mid-transition. States with a
toast run 2000 ms, before its 3 s timer hides it. Two captures of the same tree
are identical. The product and the ordinary QML tests use real time.
Each entry is one screen state. `reference` names a file in
[`docs/design/handoff/screenshots/`](../design/handoff/screenshots/): the 22 main
`*.jpg` files, the `os-*.png` Open services states, the `in-*.png` address
states, the `cl-*.png` closing-applications states, the `kb-*.png` on-screen
keyboard states and the `se-*.png` Settings editor states. Every handoff file is
in the manifest exactly once. A state with no handoff file has `reference` null;
these capture-only states cover screens with no handoff frame, such as the quick
menu, the volume HUD, Switch to desktop and the application launch states. A
handoff file whose state the shell does not draw has `render` false and a
reason: the address and search states, the running-applications screen and
the folder picker are in that group. A rendered state whose layout differs from
its handoff frame says how in `note`: the Library folders frames, for example,
map to the inline folder list that Settings draws for a multiple path setting. The
comparison does not edit those screens.

```sh
cmake -S . -B build -G Ninja
cmake --build build --target arinn-ui-test
python scripts/ui-compare.py --check
python scripts/ui-compare.py
```

`--check` only confirms that the manifest and the handoff directory name the
same files. [`tests/ci/test_ui_compare.py`](../../tests/ci/test_ui_compare.py)
runs the same validation in the `docs` CI job, together with a check that every
rendered state has a scene in `capture.qml`, so a new handoff file cannot be
added without a manifest entry (#513). The full command renders every `render: true` state with
`arinn-ui-test` and [`tests/ui/visual/capture.qml`](../../tests/ui/visual/capture.qml)
on the offscreen platform and the software scene graph. It writes PNGs and
`report.html` under `build/ui-compare/`. The capture request is generated in
that output directory. The tool does not rewrite a tracked file. Those files
are generated and are not committed. A scene can name the parts it must show
(`expect`); the capture fails when one of them is missing or hidden instead of
saving an empty frame. The rendering itself is not part of CI.

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
The `updated` Home state (the daemon runs another build, ADR-0097) offers
Restart instead of Retry and emits `restartRequested()`; `Main.qml` calls
`Backend.restartArinn()`, which exits with status 75 (ADR-0098).
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
