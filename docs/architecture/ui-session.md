<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Now-playing session

Presentation for [specification §7.3](../specifications/arinn-specification.md#73-media-sessions-mpris)
and [§5.1](../specifications/arinn-specification.md#51-home) Continue.
`MediaCatalog` in `apps/arinn-shell/media_catalog.cpp` reads
`org.arinn.Media1` on each ready `media` provider path ([ADR-0058](../adr/0058-media-routing.md)).
The screen and Home do not talk to MPRIS, and they do not send a file URI.

This is the shell half of AC-09. Operator playback on a real session is not
part of this change. Asset bytes stay out of the shell until
[#131](https://github.com/LimeBeck/arinn/issues/131).

## Model

Reads are asynchronous. `GetSessions` is paged. A repeated page token, a
page generation other than the provider's, a revision that is not the first
page's, or an invalid session drops that provider. A revision starts at 1. If every media provider fails, the session state is `unavailable`.
No media provider is `empty`. The daemon disappearing is `unavailable`.

Sessions are ordered for Continue: `playing`, then `paused`, then any other
state. Within a state, the newer `anchor_us` comes first. The first session is
selected until the user picks a Continue card. A picked session that leaves
the snapshot is `unavailable`; the model does not switch to another one.

`position_us` is shown as published for every state except `playing`. While
`playing`, the model adds the elapsed `CLOCK_MONOTONIC` time since `anchor_us`
and clamps to a known duration. `paused`, `stopped` and any other token do not
advance. Unknown actions are ignored. `play-pause` is presented as `playPause`.
An action is listed only when the session carries that token.

Commands (`Play`, `Pause`, `PlayPause`, `Seek`, `Next`, `Previous`, `Stop`)
are sent only when the selected session lists them. Otherwise the operation
state is `Unsupported` and nothing is called. A negative seek is
`InvalidArgument`. A method error, including `Unsupported`, is not success.
A returned object path stays `pending` until `Operation1.Get` reports
`succeeded`. Playback on screen changes only when a later snapshot says so.

Artwork ids are discarded. The plate stays the generic theme colour.

## Screen

`SessionView` lives in `apps/arinn-shell/ui/screens/session/`. Production
`Main.qml` binds it to `MediaCatalog` and opens it from a Continue card whose
`kind` is `media`. The view still does not call D-Bus. It reuses `Theme`,
`UiText`, `LogicalCanvas`, `ProgressTrack`, `ActionButton` and `StatusPane`.

`sessionState` `ready` shows the session. `empty` and `loading` show
`StatusPane` with those kinds. Any other value, including `unavailable`, shows
the unavailable pane. Buttons are visible only when `availableActions` lists
them. The progress track stays visible while a session is ready so the
position can be read; without `seek` it is dimmed and does not emit.

`seekRequested` carries an absolute microsecond target. Left or Right on the
focused seek track steps 10 seconds, clamped to `0..duration` when duration is
known, and only when `seek` is listed. Activating the Seek button moves focus
to that track. The view never writes `positionUs` or `playbackState`.

## Focus

The view is its own focus scope. It takes focus only while it is visible and
enabled, so a closed session in `Main.qml` does not consume Home keys.
While a session is ready, Left and Right move among the listed actions and
stop at the ends. Up opens the seek track and Down returns to the action that
opened it. On the seek track, Left and Right are the seek step. Empty, loading
and unavailable focus the screen itself. Escape, Backspace and the remote Back
key each emit `backRequested()` once and do not change playback.

## Verification

C++ model tests are `apps/arinn-shell/media_catalog_test.cpp` (`media-catalog`,
private bus): paging, anchor interpolation, a paused clock, duration clamp,
`Unsupported` without a call, success only after `Operation1.Get`, a repeated
page token, a revision or generation that breaks the snapshot, and the
production Continue and session binding.

Qt Quick tests are `tests/ui/tst_session.qml` (`Session`) and
`tests/ui/tst_media_continue.qml` (`MediaContinue`). Run offscreen:

```sh
QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software \
  dbus-run-session -- build/apps/arinn-shell/arinn-media-catalog-test
QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software \
  build/apps/arinn-shell/arinn-ui-test -input tests/ui/tst_session.qml
QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software \
  build/apps/arinn-shell/arinn-ui-test -input tests/ui/tst_media_continue.qml
```

`arinn-browser-catalog-test` also loads `Main.qml`, so that executable links
`media_catalog.cpp` and registers `MediaCatalog` the same way the shell does.
`productionSaveWiring` presses Backspace while the session screen is closed
and expects Home navigation to open.

Local verification on 2026-09-30, Qt 6.11.2, offscreen platform, software
renderer: `arinn-media-catalog-test` reported 11 passed,
`arinn-browser-catalog-test` reported 8 passed,
`arinn-ui-test -input tests/ui` reported 176 passed, and
`dbus-run-session -- python tests/integration/phase1.py` passed, including
`BackendTest::typedWireAndLocalFocus`. This is not the Arch CI Qt. Operator
tests were not run. AC-09 is not marked passed.
