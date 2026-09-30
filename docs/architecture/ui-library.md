<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Library screen

Presentation for [specification §5.3](../specifications/arinn-specification.md#53-library)
and [§7.4](../specifications/arinn-specification.md#74-local-library-and-mpv).
`LibraryCatalog` in `apps/arinn-shell/library_catalog.cpp` reads `org.arinn.Library1`
on each ready `library` provider path ([ADR-0060](../adr/0060-library-routing.md)).
The screen does not talk to a plugin, and it never sees a file URI.

Poster and fanart ids are loaded through `Assets1`
([#162](https://github.com/LimeBeck/arinn/issues/162)). This binding does not
close AC-15: scanning a folder and playing a file on a real session are not
part of it.

## Model

Reads are asynchronous. `GetLibrary` is paged. A repeated page token, a page
generation other than the provider's, a revision that is not the first page's,
an invalid item, or a parent that is missing or cyclic drops that provider. A
revision starts at 1. If every ready library provider fails, the state is
`error`. A library provider that is not `ready` makes the state `unavailable`.
No library provider is `empty`. The daemon disappearing is `unavailable`.

An unavailable backend is still read. Entries with `available` false stay in
the catalogue as missing; they are not deleted. Unknown kinds display as
`other`. Year, season and episode are shown only when their flag is set.
A non-empty poster or fanart id becomes an `image://arinn-assets/` URL.
The same cache as session artwork applies: `Assets1.GetAsset` off the UI
thread, keyed by (provider, generation, id), within 64 MiB of compressed
bytes. An empty poster id sets `noArt`. A fetch that is still running, or
that returns `NotFound`, `Unsupported` or any other error, keeps the
generic plate. Episode stills use the episode poster id the same way.
Fanart is the detail backdrop; the stand-in stays until that image is ready.

`Features` decides whether `GetResumes` is called. Without `LibraryResume1`,
or when that call fails, the catalogue stays and progress is zero. Resume
revisions are independent of the catalogue revision. A position above zero
that is not `completed` is Continue: the fraction is position over the resume
duration, or over the item duration when the resume duration is unknown. A
position with no duration is a small fraction so Continue still appears.
`completed` stays Play. A series uses the resume of the episode Play would
open, so Continue appears on the series when that episode has progress.

Collections are the tabs. Items with an empty `parent_id` are the posters.
Children are that parent's episodes, grouped into season pills when they
carry a season. The shell does not invent a plot.

The catalog subscribes to `Changed`, `ProvidersChanged` and `StateChanged`
before the first read, and those signals reload the snapshot. A 30 s timer
is only the fallback for a missed signal.

`Launch` for a film is that film. `Launch` for a series is the episode with
an unfinished resume, or, when none is in progress, the first available
episode in season and episode order. The series id is not sent. A missing
target, a series with no available episode, or a catalogue that is not
`ready` sets the operation to `Unavailable` and does not call. A second
launch while one is `pending`, `queued` or `running` is ignored. The call
returns an operation. The shell polls `Operation1.Get` until it leaves
`queued` or `running`, and stops after 31 s (the 30 s launch timeout plus one
second). `succeeded` with a `media` result emits that session's presentation
id. The URI is never a result. `RestartFromBeginning` is not on this screen.

## Screen

`LibraryView` lives in `apps/arinn-shell/ui/screens/library/`. Production
`Main.qml` binds it to `LibraryCatalog` and shows it when Home is on the
Library section. The view still does not call D-Bus. It reuses `Theme`,
`UiText`, `Surface`, `Skeleton`, `StatusPane`, `ActionButton` and
`ProgressTrack`.

`libraryState` `ready` shows the grid. `empty`, `error` and `unavailable`
show `StatusPane` with those kinds. `loading` shows skeletons and keeps the
tabs focusable. `missing` draws the "No file" pill and, in detail, explains
that the entry stays. `progress` greater than zero labels the primary action
Continue. A missing entry's primary action is Remove from library; there is
no hide method on `Library1`, so that signal is not sent to the daemon.

While a launch is `pending`, `queued` or `running`, the action is busy and a
"Waiting…" line is shown. The episode still whose id is `launchingId` is busy
too, and the series action stays busy while that id is one of its episodes.
`succeeded` and an empty state are quiet. Any other value shows an error
banner. A successful launch opens the now-playing screen on the returned media
session. Back from that screen returns focus to the library detail action.
Back from Continue still returns Home.

## Fixture contract

Callers can still pass presentation data. Identifiers are opaque. The view
filters `entries` by `tabId` against `currentTabId` and does not infer a group
from titles. `kind` may be `film` or `series` for the detail eyebrow only.

| Property | Role |
| --- | --- |
| `libraryState` | `ready`, `empty`, `loading`, `error` or `unavailable` |
| `tabs` | `{id, title}` catalogue tabs |
| `entries` | `{id, tabId, title, subtitle, year, description, artwork, fanart, kind, noArt, missing, enabled, busy, progress, seasons, episodes}` |
| `seasons` | `{id, title}` on an entry. Season pills are shown only when this list is non-empty |
| `episodes` | `{id, seasonId, number, title, duration, missing, enabled, progress, artwork}` on an entry. `seasonId` filters the stills; an episode without one stays visible for every season. `artwork` is optional |
| `operationState` | Empty, `pending`, `succeeded`, or an error code |
| `launchingId` | Presentation id of the item whose launch is in progress |
| `columns` | Poster columns. `0` uses the seven-column frame; a positive value overrides it |

`noArt` draws a plate with a film glyph and the title. The bound catalogue
sets it when the poster id is empty. A poster URL still shows that plate
until the image is ready, and leaves it there when the image does not arrive.
`fanart` is the detail backdrop URL. Without one, the coloured stand-in and
the backdrop label stay.

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
`homeRequested`. Nothing is launched inside the view. `retryRequested` is the
status action for empty, error and unavailable. Loading shows skeletons and
keeps the tabs focusable; the status action stays hidden.

## Focus

D-pad focus stays inside this screen while it is visible. The view takes
focus only when it is visible and enabled, so a hidden library does not take
Home keys. Tabs move horizontally and do not wrap past either end. Down enters
the first poster, or the status action when the grid is absent. Posters move
by column and skip disabled entries; the edges do not wrap to another screen.
Up from the first row returns to the current tab. Opening a poster pushes
detail and focuses Play, Continue or Remove. Down from that action reaches
season pills when the entry has seasons, otherwise episodes when the entry
has them. Down from a season pill reaches that season's stills. Up from a
still returns to the season pills when they exist, otherwise to the action.
Escape, Backspace and Back close detail and restore the opener. A further one
of those keys emits `backRequested`. Home emits `homeRequested`, closes detail
when it is open, and leaves focus inside the view. Tab does not leave the
view.

## Verification

`tests/ui/tst_library.qml` imports this directory from the filesystem and
covers grid and detail focus, empty, error and unavailable, resume as
Continue, the missing-file badge, and the operation line.
`apps/arinn-shell/library_catalog_test.cpp` drives a fake `Library1`: paging,
a dropped provider, resume, a missing file on an unavailable backend, Launch
as an operation, and the bound screen's empty, unavailable and resume states.
An empty poster or fanart id leaves `noArt` and omits the image URL.
Image bytes and the plate that stays until a poster is ready are the
`asset-images` test described in the session screen.

```sh
cmake -S . -B build -G Ninja
cmake --build build --target arinn-ui-test arinn-library-catalog-test
QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software \
  ./build/apps/arinn-shell/arinn-ui-test -input tests/ui tst_library
dbus-run-session -- ./build/apps/arinn-shell/arinn-library-catalog-test
```

Operator tests, television readability and the 22-screen design gate are not
part of this increment.
