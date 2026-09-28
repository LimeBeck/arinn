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
| `entries` | `{id, tabId, title, subtitle, description, artwork, kind, missing, enabled, busy, progress, episodes}` |
| `episodes` | `{id, title, missing, enabled}` on an entry |
| `columns` | Poster columns; `0` fits the 1920×1080 safe area |

`missing` draws a "No file" badge on the poster and, in detail, explains that
the entry stays in the catalogue. `progress` greater than zero labels the
primary action Continue and shows the progress track. A missing entry's primary
action is Remove from library.

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
that action reaches episodes when the entry has them. Back or Esc closes detail
and restores the opener. A further Back emits `backRequested` and leaves focus
on the poster. Home emits `homeRequested`, closes detail when it is open, and
leaves focus inside the view. Tab does not leave the view.

## Verification

`tests/ui/tst_library.qml` imports this directory from the filesystem. The
existing `ui-components` CTest loads every `tests/ui/tst_*.qml`, so no build
file change is required. The cases cover grid and detail focus, the four
non-ready states plus a ready grid, the missing-file badge, and focus after a
poster disappears.

```sh
cmake -S . -B build -G Ninja
cmake --build build --target arinn-ui-test
QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software \
  ./build/apps/arinn-shell/arinn-ui-test -input tests/ui tst_library
```

Operator tests, television readability and the 22-screen design gate are not
part of this increment. Connecting the view to Home is a follow-up.
