<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Library fixture screen

Presentation-only Library view for [specification §5.3](../specifications/arinn-specification.md#53-library)
and [§7.4](../specifications/arinn-specification.md#74-local-library-and-mpv).
It is not wired into Home, does not talk to a daemon or provider, and does not
close AC-15 or any Phase 5 gate.

`LibraryView` lives in `apps/arinn-shell/ui/screens/library/` and imports the
existing presentation components. It does not modify them. Posters are
`Surface` controls because `ContentCard` is the landscape card, not a poster.
Loading uses `Skeleton`. Empty, error and unavailable states use `StatusPane`.
Resume uses `ProgressTrack`. Detail actions use `ActionButton`.

## Fixture contract

Callers pass immutable presentation data. Identifiers are opaque. The view
filters `entries` by `tabId` against `currentTabId` and does not infer a group
from titles. `kind` may be `film` or `series` for the detail eyebrow only.

| Property | Role |
| --- | --- |
| `libraryState` | `ready`, `empty`, `loading`, `error` or `unavailable` |
| `tabs` | `{id, title}` catalogue tabs |
| `entries` | `{id, tabId, title, subtitle, year, description, artwork, kind, noArt, missing, enabled, busy, progress, seasons, episodes}` |
| `seasons` | `{id, title}` on an entry. Season pills are shown only when this list is non-empty |
| `episodes` | `{id, seasonId, number, title, duration, missing, enabled, progress}` on an entry. `seasonId` filters the stills; an episode without one stays visible for every season |
| `columns` | Poster columns. `0` uses the seven-column frame; a positive value overrides it |

`missing` draws a "No file" pill on the poster's bottom-left, not a second
title, and in detail explains that the entry stays in the catalogue. `progress`
greater than zero labels the primary action Continue and shows the progress
track. A missing entry's primary action is Remove from library. `noArt` draws a
plate with a film glyph and the title. Any other poster is a colored stand-in
labeled `poster`. `year`, or `subtitle` when `year` is absent, sits under the
title.

## Frame

The view is a 1920×1080 canvas. The heading is at left 120, top 96, 56px.
Catalogue tabs sit at top 190: 64px pills with a 32px radius. The selected tab
has an ember underline. The poster grid starts at top 282, inset 28px, so the
first poster is at left 120, top 310. Posters are 210×315 with a 20px radius,
in seven columns unless `columns` is set, with a 40px column gap and a 48px
row gap. Focus scales the poster and lifts its label. Detail replaces the grid:
a full-bleed stand-in backdrop, a left scrim, an uppercase eyebrow, a 112px
title, meta, description, and pill actions. A series with `seasons` adds season
pills and episode stills at 420×236, labeled `still`.

Signals are intents only: `tabRequested`, `entryRequested`, `playRequested`,
`episodeRequested`, `removeRequested`, `retryRequested`, `backRequested` and
`homeRequested`. Nothing is launched, hidden or deleted inside the view.
`retryRequested` is the status action for empty, error and unavailable. Loading
shows skeletons and keeps the tabs focusable; the status action stays hidden.

## Focus

D-pad focus stays inside this screen. Tabs move horizontally and do not wrap
past either end. Down enters the first poster, or the status action when the
grid is absent. Posters move by column and skip disabled entries; the edges do
not wrap to another screen. Up from the first row returns to the current tab.
Opening a poster pushes detail and focuses Play, Continue or Remove. Down from
that action reaches season pills when the entry has seasons, otherwise episodes
when the entry has them. Down from a season pill reaches that season's stills.
Up from a still returns to the season pills when they exist, otherwise to the
action. Escape, Backspace and Back close detail and restore the opener. A
further one of those keys emits `backRequested` and leaves focus on the poster.
Home emits `homeRequested`, closes detail when it is open, and
leaves focus inside the view. Tab does not leave the view.

## Verification

`tests/ui/tst_library.qml` imports this directory from the filesystem. The
existing `ui-components` CTest loads every `tests/ui/tst_*.qml`, so no build
file change is required. The cases cover grid and detail focus, the four
non-ready states plus a ready grid, the missing-file badge, focus after a
poster disappears, and the handoff frame: column count, poster geometry, the
ember tab underline, stand-in and no-art plates, and series season stills.

```sh
cmake -S . -B build -G Ninja
cmake --build build --target arinn-ui-test
QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software \
  ./build/apps/arinn-shell/arinn-ui-test -input tests/ui tst_library
```

Operator tests, television readability and the 22-screen design gate are not
part of this increment. Connecting the view to Home is a follow-up.
