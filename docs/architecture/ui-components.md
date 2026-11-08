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
| `Theme`, `UiText` | Shared colours (the Hearth light palette: Charcoal `#0F1115`, warm day text, `chip`, `mat`, `rim`, `hearth` and the pool centre, plus `nightText1..3` and `eveningText3`), radii, timing, fonts and named typography roles. Onest for system text and Playfair Display for `hero` (96/600), `detailTitle` (104/600) and `display` (220/600, the Ambient clock face), with tabular numerals and, on content roles, lining figures. Each weight is applied to the font's `wght` axis (see *Font weights*). Eyebrows are sentence case (26/500). Page titles are Onest 56/600 with no negative tracking. `fitSteps` (`[[size, lines], …]`) fits a content title: the first step whose rendered text is not elided wins, and `fitStep` reports it. `fit()` writes `fittedPixelSize` and `fittedLineCount`; `font.pixelSize` and `maximumLineCount` stay bound to those properties, so a later text or font change is measured again and clearing the steps returns the role size ([ADR-0136](../adr/0136-hearth-light-visual-direction.md), [ADR-0137](../adr/0137-playfair-display-for-content-titles.md), [#722](https://github.com/LimeBeck/arinn/issues/722)) |
| `FontWeightAxis` | Put inside a plain `Text` or `TextInput` in Onest that sets `font.weight`; applies the weight to the `wght` axis as `UiText` does. Draws nothing |
| `LogicalCanvas` | Centred 1920×1080 stage, uniform `min(width/1920, height/1080)` scale |
| `Surface`, `FocusRing` | Focus, press, disabled and busy treatment. `shapeRole` selects the plate: content and containers have none at rest, actions use `plateAction` (primary `platePrimary`), objects use `plateObject`. Content and objects focus at scale 1.04 from the bottom edge, lifted 8 px. `FocusRing` is the 2 px rim (`rimBottom` to `rimTop`), not a white ring; Settings category, row, path, remove and add use it with an 8% plate. `activated()` intent |
| `ActionButton` | Content-sized pill, `text`, optional `iconName` rendered by LineIcon, `destructive` tint and `primary`/`selected` (14% plate). `labels` reserves the widest alternate label, including late font loading and language changes; `busyText` defaults to “Loading…” but a feature may keep its own pending/failure label. `maximumWidth` bounds long labels with elision; the accessible name retains the full text. Focus fills the pill with text-1 and draws its contents in ink. `highlighted` lets a local virtual-focus owner request the same treatment without moving Qt focus |
| `IconButton` | The same Surface/ActionButton activation, disabled, pressed, selected and primary/destructive contract with a round plate and LineIcon, using `text` as the accessible name. Busy blocks activation and shows a rotating refresh mark. Music's round buttons, QuickMenu's media-card controls and Player transport use it; Music keeps its focus caption as feature composition. Player retains its local keyboard/OSD focus model and routes clicks through its existing transport intents |
| `ConfirmationDialog` | Shared full scrim and centred 900 px plate, Theme dialog surface/radius/line, wrapped `title` and optional `body`, `leading`/`content` slots and a wrapping action flow. Music confirmations, CloseFlow and LibraryFolders use the same composition. The owning screen opens it, chooses initial Cancel focus (`focusInitial()` can choose `initialFocusItem` or the first enabled action), owns operations and restores the opener or its fallback. The local focus scope traps arrows and cycles Tab/Backtab through visible enabled actions; Return/Enter and busy/disabled activation stay with Surface. `dismissible` gates Back and outside-click `dismissed()` intents, so the feature retains its in-flight cancellation policy. Home bubbles to the screen. Hidden dialogs leave pending screen focus restoration alone, and deferred navigation never reclaims another scope's focus |
| `SideSheet` | Matching 720 px right-hand plate with Theme dialog surface/line and a full scrim; `panelWidth`, a content slot and an optional outside-click dismissal intent. Music Sources, More and Queue share it. Header/content geometry, scrolling, initial focus, local navigation, hover-motion suppression and operation policy remain in each feature scope |
| `ArtworkFrame` | Bounded asynchronous Image/Canvas loading, centre crop, rounded clipping and release of the previous Canvas copy on URL or decoded-size changes. `source`, `radius`, `cache`, `placeholderColor` and placeholder/content/status slots are presentation inputs. `imageReady` reports Image readiness; `loaded`/`ready` require the current Canvas paint. `imageDisplay` retains the Image-like `source`/`status`/`ready` adapter used by Library and native fixtures. Only the image slot transforms: `focused`, `pointerMode`, `alignStart`, `focusScale`/`focusLift`, mat/rim/bounce and row brightness follow the shared Hearth contract. Feature content is shaded; status content stays readable. `image`/`canvas` expose the existing bounded rendering objects for feature effects and verification. It owns no retries, entry mapping or operations. |
| `CaptionedCard` | A Surface with an ArtworkFrame and separate stationary caption slot. `artworkHeight` (or `defaultArtworkHeight`), `captionHeight` and `captionGap` set geometry; `artworkContent`, `artworkStatus` and `caption` are composition slots. The hit area and caption keep their focus geometry; `imageScale`/`imageLift` report the frame transform. ContentCard, MusicCard, Home's title poster and Library's poster delegate are production consumers. Feature wrappers retain progress, identities, source marks, playing/missing markers, folder roles and activation routing. |
| `ContentCard` | `text`, `subtitle`, `artwork` URL supplied by trusted shell models (a file or `image://arinn-assets/`), `running`, optional `showProgress` and normalized `progress`, optional `stack` of up to three `{letter, icon}` chips drawn overlapping instead of the monogram. The plate stays until the image is ready; the artwork is centre-cropped to the card like `Image.PreserveAspectCrop` (the canvas helper `ui/artwork.js`, shared with the Downloads poster), the previous image is released when the URL changes, and the title and subtitle sit below the image, each on one line and elided, so a word wider than the column does not run past the card. A 4 px Ember line is flush with the bottom edge of the image, with a 2 px Charcoal band above it and no scrim. Focus puts a 5 px Charcoal mat around the image and a 2 px rim outside that mat ([ADR-0136](../adr/0136-hearth-light-visual-direction.md), [#724](https://github.com/LimeBeck/arinn/issues/724)). Only the image lifts (`imageScale` 1.04 from its bottom edge, `imageLift` −8 px); the card and its caption stay, so caption text never moves on focus. A card at its row's start (`alignStart`) grows from its left edge, shifted by the mat and rim, so the rim stays inside the safe area. The warm bounce shows only on artwork, and a captioned card has no cast light: both read as bars. A card without artwork is an identity-light tile as in Applications, with its chip, or chip stack, top-left and a running entry's Ember dot before the caption ([#837](https://github.com/LimeBeck/arinn/issues/837)). A narrow card (under 300 px, the New row's posters) sets the title at 24 px ([#655](https://github.com/LimeBeck/arinn/issues/655)). Bytes from Assets1 are decoded in C++ at the card's size: the Image `sourceSize` and the Canvas copy both request it, the decoder covers it without upscaling, and no image exceeds 4096 pixels a side. A request cancelled before a pool thread takes it, such as a poster scrolled away, finishes without a daemon call ([#611](https://github.com/LimeBeck/arinn/issues/611)) |
| `AppTile` | `text`, neutral `monogram`, `running`. The default object tile names the application at 26 px on up to two lines. `shelf` is the Home favourites row: height 88, Onest 600 at 32 px, one line elided at 360 px, a provider icon in a neutral chip and no letter, an Ember dot after the name, and a 4 px lift in focus. `identityLight` is an Applications tile, 300×168, chip 64, name 30 px, with a radial identity glow at half strength at rest and full in focus ([#725](https://github.com/LimeBeck/arinn/issues/725)) |
| `ProgressTrack` | Clamped finite `value` from 0 to 1; Ember represents progress only. `line: true` is the artwork line: 4 px Ember flush with the bottom and a 2 px Charcoal band, with no grey track |
| `Skeleton` | Fixed-size pulsing placeholder; same default geometry as a content card |
| `AmbientGlow` | Hearth light (ADR-0136): a black ceiling over the top 44% (50% at the top edge), a warm floor ellipse and a pool under the focused item. `source` is artwork, else `tint` is the identity colour; `lit` and `strength` (1 Home, 0.6 Applications, 0.6 Library, 0.5 detail) fade the field. The pool's hue is the dominant hue of a 24×24 artwork decode, chroma 0.10 reduced to 0.065 at hue 240°. Floor and pool are painted once into a one-fifth-resolution canvas and scaled up; a 900 ms cross-fade follows focus after a 200 ms settle. No full-resolution art is blurred. `followsTime` with `clockText` or `hour` selects day, evening (19:00, light 85%, veil 6%) and night (23:00, light 60%, veil 22%). Day text is `oklch(0.95/0.80/0.66 0.008 75)` on Charcoal. At night, Theme text becomes text-1 `oklch(0.97 0.006 75)`, text-2 `0.84` and text-3 `0.761`. `textContrastMinimum` is 4.8, the brief's bar; WCAG AA is 4.5 and the owner has not chosen. The check covers the veiled background, a resting plate, the night focused plate, Home and Applications at their brightest light, and Settings and Downloads under the 22% veil. The brief's night text-3 `0.71` is about 4.07:1 on the brightest Home light, 4.38:1 on a resting plate and 4.36:1 in Applications, so it misses both bars. `0.761` is the smallest thousandth of OKLCH L that holds 4.8:1 on those surfaces. A focused plate at the day fill (white at 20%) would need text-3 `0.845` to reach 4.8:1; at night the fill is white at 9.5% (`plateFocusNightAlpha`), the lightest fill that still holds 4.8:1 for text-3 `0.761`. Settings rows use `Theme.plateContainer` (white at 8%) and the rim. The player OSD still uses `Theme.plateFocus`. A focused locked Settings row uses `Theme.plateDisabled` (white at 12%) and draws the same rim; its text stays the hint colour. An unfocused container row, including a locked Settings row, has no plate. Night text-3 on the locked fill is about 4.54:1, under the bar, which is accepted for an inactive row. A 4.5 bar would allow text-3 `0.741` with that same night fill. Evening keeps the day titles and the day focus fill, and raises hints to `oklch(0.778 0.008 75)`, the smallest thousandth that holds `eveningContrastMinimum` (4.5) on the evening light. Evening Home is about 4.50:1 and a focused plate about 4.78:1, both below `textContrastMinimum`. The evening checks use `eveningContrastMinimum`, defined next to the night bar; whether evening should meet 4.8 is the owner's decision. The step above the brief, the night fill and the 4.8 bar are not confirmed by the owner. Floor and pool centres are the `Theme` tokens `hearthLightness` 0.44, `hearthChroma` 0.07, `hearthHue` 55, `poolLightness` 0.50, `poolChromaWarm` 0.10 and `poolChromaCool` 0.065. This light does not choose text colours; Theme does, from the period the light publishes. Settings and Downloads host the warm veil only: the floor and the pool stay off, and the veil is not removed when those screens open. The player does not host it, so video stays uncovered. Applications use strength 0.6, with the Library; the prototype's `updateLight()` value 0.8 is not used. The cases live in `tests/ui/hearth` (`ctest -R '^ui-hearth-tst_hearth_light$'`), a separate process from every other QML test file. A QtTest abort in Home ([#733](https://github.com/LimeBeck/arinn/issues/733)) therefore does not prevent the light cases from running. Light under text stays at or below L 0.48, measured by compositing those same stops |
| `LineIcon` | The single shared line-icon component: `name` (a Material Symbols name from the handoff), `color`, size from `width`/`height` on a 24-unit grid. Icons are drawn in code on a `Canvas`; the shell ships no icon font, so rendering does not depend on installed fonts or the Qt version. Today: the on-screen keyboard icons, Open services `bookmark_add`/`bookmark_added`, the navigation icons `home`, `library`, `download`, `apps`, `settings`, `screen` and `power` (`public` and `close` are reused), and the music controls `stop`, `queue_music`, `thumb_up`, `thumb_down`, `radio`, `push_pin`, `refresh` and `music_note` with `play`, `pause`, `skip_previous`, `skip_next` and `schedule`, and Now playing's `favorite`, `favorite_fill` and `more_horiz` ([#802](https://github.com/LimeBeck/arinn/issues/802)); the music screens draw no icon set of their own ([#756](https://github.com/LimeBeck/arinn/issues/756) M23). A new handoff icon (such as the Close icon of [#565](https://github.com/LimeBeck/arinn/issues/565)) is added here, not as a glyph or a second component |
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
test rejects any other `forceActiveFocus()` call. The `focus-fill` test scans every `ui/**/*.qml` file except `Theme.qml` and
rejects a focus plate written as white at alpha 0.2 or 0.20, as an eight-digit
hex colour with an alpha channel, or as opacity 0.2, including a plate selected
by `isCurrentItem`, so those controls use `Theme.plateFocus`.

Screens own focus scopes, explicit neighbours and restoration. The input mode is
shell-wide ([ADR-0095](../adr/0095-shell-input-mode-ownership.md)): the
`InputMode` singleton holds pointer/D-pad mode and the 2 s key cooldown, screens
bind `pointerMode` to it and report keys and hover positions, and
`InputModeSource` in `Main.qml` reports every key press and pointer movement in
the shell window and hides the cursor in D-pad mode. Screens pass `pointerMode`
and `hoverFocusEnabled` to controls; the latter must be false during the 500 ms
hover suppression after a scroll. Controls may also receive
`hoverMotionRevision` from `InputMode.pointerRevision`: each genuine movement is
consumed once, including while hover is suppressed. Newly created controls
start at the current revision, so stale deferred hover and model replacement
cannot reuse old movement to take focus. The default `-1` retains the existing
contract for consumers that have not opted in. Components never decide
global navigation or call providers for a focus change. The gallery demonstrates
only a small local focus graph, not the complete screen/modal stack.
`dpadRequested()` reports the key to `InputMode` even when `KeyNavigation`
consumes the arrow event locally; in the shell `InputModeSource` sees it first.

Music forwards this same contract through its room, shelves, All grid, search,
detail, transport controls and sheets ([#878](https://github.com/LimeBeck/arinn/issues/878)).
`Main.qml` binds `MusicView.ownsFocus`; controls behind a local modal or a different
screen cannot take hover focus. A scroll, a room shelf refresh or an overlay
transition suppresses hover for 500 ms. Expiry alone never requests focus.
Hover updates a list/grid's current index, so the next D-pad move starts at the
hovered entry. The existing 2 s key cooldown still belongs to `InputMode`.

`MusicCard` shades its artwork and placeholder, keeping captions and playing
markers at full brightness. The focused card is at 1, its row's neighbors at
0.82 and other rows at 0.62, including the All grid. `tst_music.qml` covers
rendered artwork/caption brightness, pointer/D-pad transitions, cooldown,
stationary-pointer scroll/refresh protection and modal/screen focus ownership.
The shared ArtworkFrame/CaptionedCard composition now keeps Music and Library
poster captions stationary while their image alone grows and lifts
([#881](https://github.com/LimeBeck/arinn/issues/881)). MusicArtwork retains its
six-attempt exponential retry policy around the same frame, including hidden
state suppression; it supplies an existing card frame instead of creating a
second loader. Unavailable Music cards reserve the internally transformed rim
in their compositing layer. Home title posters keep their flat, unlit fallback;
ContentCard retains its feature-specific identity, progress and music face.
Library episode stills reuse ArtworkFrame through a thin Image-like adapter
without changing episode navigation. This does not qualify a physical TV.

## Development and verification

```sh
cmake -S . -B build -G Ninja
cmake --build build
ctest --test-dir build --parallel 2 --output-on-failure
build/apps/arinn-shell/arinn-ui-gallery
build/apps/arinn-shell/arinn-ui-gallery --4k
QT_QPA_PLATFORM=offscreen build/apps/arinn-shell/arinn-ui-gallery --capture /tmp/arinn-ui.png
```

The gallery is a component catalogue: `Gallery.qml` and, with `--keyboard`, the
on-screen keyboard over Site or search. It does not render screens or their
states; those come only from the `tests/ui/visual` fixtures (see
[Handoff visual comparison](#handoff-visual-comparison), #567), so there is one
set of screen fixtures.
Its confirmation-dialog and side-sheet launchers exercise the shared slots,
local focus and return to the opener with synthetic actions. `tst_modals.qml`
covers cancellation policy, modal Tab/Backtab and arrow navigation, hidden or
missing openers, busy/disabled activation, pointer transitions and live updates
of long English/Russian labels; screen suites retain their domain cases.
If a focused action is removed, the dialog scope keeps modal key ownership;
the next arrow chooses an available action. A feature that wants a fresh default
after changing its actions calls `focusInitial()` explicitly.
The gallery opens a regular window and requires no daemon or session bus. It uses
original geometric artwork and neutral monograms. Its actions update explanatory
fixture text only. `--capture` saves a PNG and exits; `--4k` uses 3840×2160.
Any other option, including the removed screen flags, prints an error and
exits with status 2.
The test runner uses the offscreen platform and software rendering in CTest/CI;
it does not touch KWin, real applications, screen lock or session services.

CTest registers each `tests/ui/**/tst_*.qml` file once as a separate
`ui-<relative-path-without-extension>` test, replacing `/` with `-` in the
name. For example, `ctest --test-dir build -R '^ui-tst_music$'
--output-on-failure` runs the music file. Recursive discovery includes Hearth
light as `ui-hearth-tst_hearth_light`; it is not also run by a directory-wide
suite. CI uses two CTest workers, so independent private-bus and offscreen
processes can overlap. An abort in one QML file still fails the run while the
other files execute.

The source-stop deadline regression in `tst_music.qml` observes one real tick
of `musicSourceStopWatch`, then emits its timer callbacks synchronously to
advance the remaining wait and Stop phases. It checks that the timer stays
running and that sending Stop resets the phase's elapsed time. This avoids
17 seconds of wall-clock sleeps without changing production intervals or
deadlines. Other source-stop cases retain their real timer-driven paths.

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

Playfair Display, the content-title face
([ADR-0137](../adr/0137-playfair-display-for-content-titles.md), revising
[ADR-0136](../adr/0136-hearth-light-visual-direction.md) D-2), is bundled
unchanged as `ui/fonts/PlayfairDisplay.ttf` from the Google Fonts
`ofl/playfairdisplay` directory (`PlayfairDisplay[wght].ttf`, SHA-256
`c40f2293766a503bc70cce9e512ef844a4ccb7cbcde792fe2ea31d191917d8d6`):
[font source](https://github.com/google/fonts/tree/main/ofl/playfairdisplay),
[upstream project](https://github.com/clauseggers/Playfair-Display).
Copyright 2017 The Playfair Display Project Authors. License: SIL Open Font
License 1.1, in `LICENSES/OFL-1.1.txt`, with the binary annotated in
`REUSE.toml` the same way as Onest. The owner approved this face; the pull
request that adds the file still needs the owner's review. It is a variable
font with a `wght` axis from 400 to 900 (default 400; SemiBold is 600) and is
weighted through the same `Theme.applyWeight` path as Onest. Content roles
request `lnum` and `tnum`. The face has lining figures and no tabular-figures
feature, so `tnum` does not change it. Ysabeau is no longer bundled. The
`display` role is this face at 220/600 for the Ambient clock. The Ambient
screen still draws its handoff clock at 160/500 until that screen's own task;
the settings integer readout keeps Onest 112/700 and does not use `display`.

## Font weights

Onest is a variable font: one face with a `wght` axis from 100 to 900 and
named instances. Playfair Display, used by the content roles, is weighted the same way. Qt does not match those instances by `font.weight` for an
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
A row keeps Home's safe right edge (x1824, handoff 01). The viewport keeps a 48 px margin on the right for the
focus lift and scale, and each card is clipped at that edge. Only
the focused card, while its body is inside the safe area, paints into that
margin. A neighbour, or a focused card flicked
across the right edge, is cut at the edge. On the left, cards stay visible as they cross the screen and fade out from x 24 to x 112 (ADR-0136); the fade is a Charcoal ramp because the software renderer has no row mask. A card wholly outside the safe right area is not
painted and takes no pointer focus or clicks, though D-pad focus still reaches
it. Scrolling to a focused card keeps its body inside the safe right edge. So
no card peeks past x1824 (#564). The rest of a focused row is drawn at brightness 0.82 and other rows at 0.62; the focused image stays at 1.
A playing or paused music session leads the further Continue cards as the
music card's variant 1 (#803, ADR-0142 MH-2): `ContentCard.musicState` draws
the cover blurred by 28 px under a 30–55 % Charcoal shade, the cover centred
at the frame height minus 44 with a hairline and a cast shadow, the playing
chip (56 × 42 at 16 px, three Ember bars that move while playing and stand
still in `text-3` on pause) and the Ember progress line. The backdrop is masked
to the card's rounded outline without padding (`autoPaddingEnabled: false`),
so it never spills past the card, and it replaces the sharp cover wherever
shader effects draw; the software renderer keeps the sharp cover. The card has
no transport button of its own. Without a cover,
the station's is used, then the source's mark (`screen` or `public`) in a
neutral chip. The caption is
the track over "artist · station or album"; the hero reads "station ·
playing" or "Paused" and names the keys, the Play/Pause key only while the
session offers the action it needs (`homeEntries.playPause`), else just "OK —
open player". Play/Pause, Play and Pause act only when the session can do what
each asks; otherwise the key passes on. OK opens *Now playing*, Back
returns to the card, and Play/Pause on Home acts on the music session
without moving focus unless the focused card is linked to a media session
(`HomeView.mediaKey`). Next and Previous go to the music session too, and
the daemon sends them only where it offers them; `Key_Play` is a toggle, as
in Ambient. A held media key acts once: its auto-repeats are taken without
another request ([#831](https://github.com/LimeBeck/arinn/issues/831)). The shell has no reduced-motion setting yet; the
bars stop with `motionEnabled`.
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
([#364](https://github.com/LimeBeck/arinn/issues/364)). The title is Playfair Display 600 and
fits in steps, 96 px on one line or 68 px on two
([#722](https://github.com/LimeBeck/arinn/issues/722)). The block starts at y 150
and the title slot is two lines at 68 px, bottom-aligned, so a one-line title
sits lower and a two-line title grows upward. The rows follow the brief's positions ([#837](https://github.com/LimeBeck/arinn/issues/837)): the Continue label at y 404 (`firstRowY`, fixed, so nothing under the hero moves when it changes; an empty eyebrow keeps its line) and the Favourites label at y 836 (`homeRowStride`, at least 432 px). Cards sit 58 px under their row label and shelf names 20 px, so a focused item, lifted and ringed, stays below its label. Favourites are a shelf of names (`variant: "shelf"`, 137 px). New follows that shelf by `shelfStep` (149 px), so its label peeks at the bottom edge while Continue is focused. Focusing New moves the rows up until Favourites starts at the first fully lit line under the header (y 180), so no empty band stays where the hero was. The hero eyebrow names the kind of the focused item and never repeats the row label: "Application · Online", "Web shortcut", "Station", "Service", "Film · 2024". A New poster with no artwork is a title card in the content face. Their y does not change when the title wraps. The hero block stays at y 150, below the menu, which ends at y 108. Global navigation is a full-height edge panel, 660 px wide, with line icons and no plate at rest. When the optional music section is present, section rows use a 70 px step instead of 84 px so service and system actions remain separate; icons, download status and action indices follow the section routes. Applications tiles carry identity light; "Add application" is an action pill. `downloadActive` and
`downloadProgress` drive the status-area download chip. `muted`, `wifi` and
`wifiLevel` (1–4, or 0 when unknown) show the mute and Wi-Fi icons between the
chip and the clock, only while known; `network` (`wired` or `disconnected`)
shows the cable or the crossed-out Wi-Fi icon in the Wi-Fi icon's place
([#408](https://github.com/LimeBeck/arinn/issues/408)); `ambientSuppressed` marks
an Activity background: the light field stays on and the floor takes the
Activity's identity hue (`AmbientGlow.floorTint`; `returnActivityIdentity` is the
identity of the running window whose platform token is the Return target, and
without one the warm hearth stays), and the near state draws dark
bands over the top 180 px and the bottom 140 px of the Activity
([ADR-0140](../adr/0140-tv-review-visual-decisions.md)). A media entry whose
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

## Navigation, Applications and screen chrome (#762)

Fixes from the TV design review [#756](https://github.com/LimeBeck/arinn/issues/756)
(C17, A9, C25, C26, C27, C30) and [#740](https://github.com/LimeBeck/arinn/issues/740):

- **Under the navigation** (C17). Home's stage (`homeStage`) is no longer drawn
  at opacity 0.45, which let the page read through the panel. It keeps full
  opacity; `navigationDim`, a black veil at 0.55, sets everything under the
  panel to brightness 0.45, and a `MultiEffect` layer blurs the stage by about
  6 px (`blurMax` 12, `blur` 0.5; Hearth light brief, *Global navigation*).
  A covering section screen gets the same veil from the navigation, and
  `Main.qml` blurs it with the same effect (`navBlurEffect`). The software
  renderer has no shader effects: `navBlurAvailable` is false there and the
  veil alone remains.
- **Applications column** (A9, C25). The title, the description, the group
  rows and the pills fade out as their on-screen top nears the header band,
  with the same `headerClearance` (120) and `headerFade` (60) as Home's rows;
  a faded row takes no pointer input. Rows are revealed as before; the edit,
  *Add application* and *Back to Home* pills call `revealItem()`, which scrolls
  only as far as needed to show the pill whole between y 180 and y 880. The
  *Add application* pill's `contentItem` is an Item that centres its icon and
  label; the Row was itself the content item before and drifted down and left.
- **Right edge** (C27). `HomeRow.fadeRight` lets an Applications row paint to
  x 1896 and fade out from x 1808 to x 1896 (`rowRightFade`), the mirror of the
  left fade. Focus, `reveal()` and the focused card's glow still use the safe
  right edge at x 1824. Home rows keep the handoff's hard safe edge.
- **Status area** (C26). `StatusArea.qml` is the top bar's right side: the
  browser-unavailable chip, the download chip, the music line, mute, Wi-Fi or
  cable or no network, and the clock. While music plays, `musicLine` shows an
  Ember dot and "track · artist" (24/500, `text-2`, at most 420 px, elided);
  on pause it goes. `Main.qml` binds it from `MusicCatalog.statusLine` to
  Home's and the screens' areas; the video player and Ambient show no status
  area (#803, ADR-0142 MH-4). Home uses it next to its lockup; `Main.qml` draws one
  more (`screenStatusArea`) at the same place over Library, Settings,
  Downloads and Open services, bound to Home's state. Open services drops its
  own clock there (`BrowserView.ownClock: false`); a view shown alone keeps
  it. Those screens' page titles hold the lockup's place at y 96, as in the
  Hearth light prototype (`#topbar .lockup.gone` off Home), so the lockup is
  drawn on Home and Applications only.
- **Power and Quick menu** (C30). See [ui-power.md](ui-power.md) and
  [ui-quick-menu.md](ui-quick-menu.md): no opaque layer under the Power scrim,
  and container rows in both.

Verification: `tst_home.qml` (`test_navigation_dims_the_page_instead_of_fading_it`,
`test_applications_reveal_every_focusable_below_the_header`,
`test_add_application_pill_content_is_centred`,
`test_application_rows_fade_at_the_screen_edge`), `tst_power.qml`
(`test_scrim_lets_the_screen_through`, container rows) and
`library_catalog_test.cpp` (`sectionScreensShareTheStatusArea`). The blur
itself is not covered: the tests run on the software renderer. The TV re-check
was not done.

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

## Interface sounds (#727)

`InterfaceSounds` (`apps/arinn-shell/interface_sounds.cpp`) plays nine pre-rendered
48 kHz WAV files from `apps/arinn-shell/sounds/` through `QSoundEffect`. The files
are generated by `scripts/render-ui-sounds.py` from the same synthesis as the
Hearth light prototype. Level follows Settings › Sound › Interface sounds (Off,
Quiet, Normal; default Quiet) and the system mute snapshot from `SystemStatus`.
While video or music plays (including OSD navigation over a playing session),
every cue except `error` is suppressed. Held-key focus moves throttle `tap` to
40 ms apart at half gain; playback never blocks the action.

The daemon stores the setting as `interface_sounds` on the `arinn` source
(ADR-0112, ADR-0136). `SettingsCatalog` exposes `interfaceSoundsLevel` to
`Main.qml`, which binds media activity and wires Home navigation, the quick menu,
startup, ambient wake and errors. Qt Multimedia is a new runtime dependency
(`qt6-multimedia` in packaging); reserve a technical ADR number before accepting
that dependency if required by project policy.

`QSoundEffect` objects are created only after `QMediaDevices::audioOutputs()`
is non-empty, and only when playback is still available. `available` is
latched for the process: an empty output list at start, or before the first
Ready decode, keeps it false even if Pulse starts later. That is not
in-process recovery, and there is no `paplay` or `libpulse` path. Settings ›
Sound shows a read-only row under Interface sounds (title and `text-2`
detail) while `available` is false. The row stays up when an output appears
after that latch. It is hidden when an output was already present.
`Backend::restartArinn()` remains the update exit (ADR-0098), so the row has
no Restart action.

Verification: `interface-sounds` and `ui-sounds-render` CTest targets;
`settings-catalog` and daemon settings tests. Listening check on the owner's
setup: not run.

### Home handoff increment evidence (#358, #364–#368)

[`evidence/home-handoff/`](evidence/home-handoff/) holds 1920×1080 offscreen
captures (software renderer, Qt 6.8.2, synthetic fixtures and the gallery's
fixture artwork) in Russian (`ru-*`) and English (`en-*`): a focused Continue
media card with the hero time line, progress, backdrop scrim, download chip and
light field; the focused activity; a focused favorite; and the open-services card
with stacked chips. They were captured from `HomeView` alone, not the production
window. No KDE session, TV or physical readability check was performed.
