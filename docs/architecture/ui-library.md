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

## Home New

`LibraryCatalog.newEntries` merges optional `LibraryAdditions1` metadata across
ready library providers. `GetAdditions` is guarded by the exact catalogue
generation/revision (not the resume revision) and a 500 ms deadline. Invalid,
unsupported or missing metadata leaves the base library usable without dates.
At most 64 canonical, unique, available top-level film/series references per
provider are accepted. Home receives the newest 12 across providers, descending
by discovery time with provider/local id ties, using existing poster/title/year
and progress presentation. Undated legacy items are omitted. Owner loss clears
the projection; an unchanged reload emits no presentation change.

Selecting New emits a library-detail intent. Main checks the current projection,
opens the existing detail and records Home as its opener. Back closes detail and
restores the selected Home card or Home's normal fallback. Ordinary Library
navigation keeps its existing Back behaviour. Detail Play and episode selection
use the existing library launch route. See
[ADR-0120](../adr/0120-home-recent-library-additions.md).

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

Collections are the tabs. The shell names the `film`, `series` and `other` kinds itself, in the user's language; a provider's collection title is shown only for any other kind ([#431](https://github.com/LimeBeck/arinn/issues/431)). Items with an empty `parent_id` are the posters.
Children are that parent's episodes, sorted numerically by known season and episode. Season pills
include a separate Without season group when known and unknown seasons coexist.
A series with no known season keeps the direct episode list. The shell does not invent a plot.

The catalog subscribes to `Changed`, `ProvidersChanged` and `StateChanged`
before the first read, and those signals reload the snapshot. A 30 s timer
is only the fallback for a missed signal. `catalogChanged` is emitted only
when the published catalog or its state changed, so a reload that finds the
same snapshot leaves bound views alone.

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
| `episodes` | `{id, seasonId, number, title, duration, missing, enabled, progress, artwork}` on an entry. `seasonId` filters the stills exactly; missing values belong only to the `unknown` group. `artwork` is optional |
| `operationState` | Empty, `pending`, `succeeded`, or an error code |
| `launchingId` | Presentation id of the item whose launch is in progress |
| `columns` | Poster columns. `0` uses the seven-column frame; a positive value overrides it |

`noArt` draws a plate with a film glyph and the title (handoff §05, *Missing
artwork*). The bound catalogue sets it when the poster id is empty. An entry
without an `artwork` URL shows the same plate, and a poster URL shows it
until the image is ready, and leaves it there when the image does not arrive.
An episode still without a ready image is the same neutral plate with the
glyph centred; its number, title and duration stay under it. `fanart` is the
detail backdrop URL. Without one, the coloured stand-in stays. The view never
paints the prototype's `poster`, `backdrop` or `still` labels (#610).

## Frame

The view is a 1920×1080 canvas. The heading is at left 120, top 96, 56px.
Catalogue tabs sit at top 190: 64px pills with a 32px radius. The selected tab
has an ember underline. The poster grid starts at top 282, inset 28px, so the
first poster is at left 120, top 310. Posters are 210×315 with a 20px radius,
in seven columns unless `columns` is set, with a 40px column gap and a 48px
row gap. Focus scales the poster and lifts its label. Detail replaces the grid:
a full-bleed stand-in backdrop, a left scrim, an uppercase eyebrow, a 112px
title, meta, description, and pill actions. A series with `seasons` adds season
pills and episode stills at 420×236.
Loaded posters and stills use the same rounded shape as their plates (20px
and 22px respectively). A local Canvas slot reuses `artwork.js` for clipping
and centre cropping on software and accelerated renderers; the fallback stays
until that canvas has painted the current source, and source changes release
the previous image. Focus rings, progress and missing-file labels remain above
the image. The episode viewport includes 16px of focus room for the scale/lift;
offsetting its row preserves the normal first-still position while keeping the
first and last focused stills inside the clip. Pixel tests cover all four
corners, focused edges, aspect crop and source/error recovery.

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

The view does not handle Menu. The key bubbles to `Main.qml`, which opens
Home's navigation menu above the library (#329). The menu then holds focus,
and closing it restores the poster or tab that opened it. Settings and
Downloads work the same way, and so do Open services, Browse and a media
session. While the menu is open it is drawn above them and owns focus; only
the player stays above it. A section picked in the menu, or Home pressed in
it, closes an open overlay; Escape and Menu close only the menu and return to
the screen that opened it.

From Settings, Downloads, Open services, Browse and a media session, Home
shows Home. Editors take keys before they bubble, so two screens send Home
themselves with `homeRequested()`:
- Settings: an open editor or option list is closed first, without
  committing its draft.
- Browse: the address field gives up Home as cursor movement. In a text field
  only Backspace and Esc keep a text meaning (§6.2).

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

## Persisted Home Continue (#643)

LibraryCatalog projects up to four available, unfinished film/series cards from
LibraryResume1, ordered by the saved observation time. One series card selects
the same most recent unfinished episode as Library launch. Unknown duration
does not discard a positive saved position. A fresh shell reads these cards
without requiring a retained Media1 session; no second history database is made.

Home merges this projection with current media sessions. Active sessions
take precedence for their library title; an old stopped session does not hide the
provider's newer saved episode. Prefixed presentation IDs keep Continue separate
from New. OK resumes the exact saved content through LibraryCatalog.launch,
while the same title in New continues to open detail without playback.

External browser/app sessions remain live-provider data. Restarting the shell
rediscovers a surviving player; retaining watched-video history after that player
exits needs a durable provider-owned reopen target and is not supplied by this
local-library correction.

## Offline folder mode (#629)

The Catalogue/Folders switch separates collection categories from physical
storage. The model exposes `folderEntries`, flattened presentation cards with a
`folderId` parent. Folder cards use `kind: folder` and a disjoint `folder:UUID`
identity namespace; media cards keep their existing content UUID. No filesystem
path reaches the view. All folder metadata comes through Library1.GetHierarchy
at the current catalogue generation/revision; the five-second assembled read is
optional and rejects stale pages, repeated tokens, invalid trees and unknown
memberships. Unsupported or invalid metadata leaves Catalogue available.

Only current-level folder members are visible. `folderTrail` retains openers and
scroll positions; Back from detail preserves the current folder, and another
Back restores its parent opener. Empty and unavailable folders stay navigable.
The mode controls are reachable from categories with Up; Down returns to each
mode's remembered card and scroll position. The current directory survives mode
changes. A pointer Back button provides the same level
transition. Refreshes respect `ownsFocus`, including while the global menu is up.

Presentation follows the [editable HTML prototype](../design/prototypes/library-navigation/README.md):
transparent view/category/season pills, a white fill for focus and an ember
underline for an unfocused selection; folder names below their icon plates; a
compact Up control; plain empty-folder text and an unavailable-root notice above
retained entries. These controls are local to LibraryView; shared controls and
historical handoff materials are unchanged.
Season/episode focus raises the detail rail, keeping its captions in the viewport.
The long-series regression covers wrapped headings, descriptions and resume progress,
with and without season pills. It traverses twelve episodes in both directions,
checks the focused still and caption against viewport coordinates, launches the
last episode, and returns through the primary action to the opener.
Horizontal episode focus reveals animate over 450 ms with the shared ease-out
curve. A new key retargets from the current visual position; native pointer
dragging or flicking cancels the focus animation. Season and detail changes
cancel motion and reset the row, so an old target cannot move a new list.
Offset changes suppress hover focus until 500 ms after the final movement.
Tests observe intermediate positions, settling, rapid reversal, pointer flick
interruption and season/detail resets as well as final focus and launch.
Back from that rail restores the primary action before leaving detail. Physical
episodes resolve their already-present parent series from the presentation model;
`detailOpenerId` retains the original file card independently of the series detail
ID, so Back and Home return to the physical location.
An external detail route discards an opener that belongs to a different title;
opening a new Home card cannot reuse the previous detail's return target.

Validation fixtures are in `library_catalog_test.cpp` and `tst_library.qml`;
provider persistence is described in [media-mvp.md](media-mvp.md). This change
adds no online metadata or artwork fetches and closes no operator gate.

## Local NFO description binding

`LibraryDescriptions1` ([ADR-0125](../adr/0125-library-descriptions.md)) is optional.
Opening a detail emits the existing `entryRequested` intent; Main binds it to
`LibraryCatalog.requestDescription`. The model requests one item asynchronously
and exposes `descriptionEntryId`/`descriptionText`. The screen uses it only for
the same open entry and renders `Text.PlainText` within the existing three-line
elision. Fixtures may still supply an entry description. The selected result is
cached at the item generation/revision; navigation, provider loss or changed
catalogue metadata clears/reloads it. Tickets and daemon epochs fence late
replies. Missing support and lookup failures leave descriptions empty and never
change Library readiness or playback actions. No full-catalogue plot reads occur.
