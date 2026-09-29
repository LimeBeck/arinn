<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Now-playing session fixture

Fixture presentation for [specification §7.3](../specifications/arinn-specification.md#73-media-sessions-mpris).
The screen draws one media session supplied by the caller. It does not talk to
MPRIS, D-Bus or `arinnd`, and it is not mounted from Home. Wiring a provider
snapshot into these properties is a separate change.

This is not AC-09. That criterion needs a discovered session and control through
the daemon. Here the caller supplies the snapshot and receives intents.
Session selection, microsecond units and position anchors stay with the
provider ([plugin runtime](plugin-runtime.md#desktop-and-media-edge-cases)).
The player OSD for a managed mpv session is a different screen
([specification §6.4](../specifications/arinn-specification.md#64-player)).

## Boundary

`SessionView` lives in `apps/arinn-shell/ui/screens/session/` and is loaded by a
filesystem import (`tests/ui/tst_session.qml`). It reuses `Theme`, `UiText`,
`LogicalCanvas`, `ProgressTrack`, `ActionButton` and `StatusPane` from
`apps/arinn-shell/ui`. Those types are not modified.

QML knows the action names `play`, `pause`, `playPause`, `seek`, `next`,
`previous` and `stop`, and the playback words `playing`, `paused` and `stopped`.
Anything else is shown as unsupported. It does not know product names. Title and
subtitle are opaque strings. The screen never writes them to a log. Artwork is a
flat fixture hue (`artworkHue`) or the shared `Theme.plate` colour. There is no image
element and no download.

## Properties

| Property | Role |
| --- | --- |
| `sessionState` | `ready` shows the session. `empty` and `unavailable`, and any other value, show no session |
| `title`, `subtitle` | Session title and artist line. Shown only while `ready` |
| `playbackState` | `playing`, `paused`, `stopped`, or unsupported |
| `durationUs`, `positionUs` | Signed microseconds. Unknown duration is −1. A negative or non-finite position displays as `0:00` |
| `artworkHue` | Plate colour. Defaults to `Theme.plate`; a caller may set a fixture hue |
| `availableActions` | Names of actions that may emit. Missing names stay visible and disabled |
| `pointerMode` | Set when the pointer moves; cleared on an arrow key |

The progress fraction is `position / duration` when duration is greater than
zero, clamped to 0..1. Otherwise the track is empty. The view never writes
`positionUs` or `durationUs`.

## Signals

`playRequested()`, `pauseRequested()`, `playPauseRequested()`,
`seekRequested(targetUs)`, `nextRequested()`, `previousRequested()`,
`stopRequested()` and `backRequested()`. Nothing in the view applies those
intents to its properties.

`seekRequested` carries an absolute microsecond target. Left or Right on the
focused seek track steps 10 seconds, clamped to `0..duration` when duration is
known, and only when `seek` is listed. At the ends the step is not emitted.
Activating the Seek button moves focus to that track; it does not emit.
Disabled actions use the shared unavailable opacity and do not emit, including
after a click or Return.

## Focus

The view is its own focus scope. While a session is ready, Left and Right move
among enabled actions and stop at the ends. Up opens the seek track and Down
returns to the action that opened it. On the seek track, Left and Right are the
seek step and do not leave the track. Empty and unavailable focus the screen
itself and show `StatusPane` with no action. Arrow keys, Tab and Return stay
inside the view and do not emit a transport intent. Escape, Backspace and the
remote Back key each emit `backRequested()` once and do not change playback.
The check lives in this screen; it does not add a shared key helper.

## Known presentation gaps

There is no light field, no scrub thumbnail and no material icon. The seven
actions share `ActionButton`. Full handoff fidelity, 3-metre TV review and
AC-09 are not claimed.

## Verification

The Qt Quick case is `tests/ui/tst_session.qml` (`Session`). The existing
`ui-components` CTest loads every `tests/ui/tst_*.qml`; this screen does not add
a CMake target. Run offscreen:

```sh
QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software \
  build/apps/arinn-shell/arinn-ui-test -input tests/ui
```

Local verification on 2026-09-29 (Qt 6.8.3, offscreen platform, software
renderer): `arinn-ui-test -input tests/ui/tst_session.qml` reported 10 passed
and 0 failed. That includes play/pause, disabled next, seek, D-pad focus, empty,
unavailable, the `Theme.plate` artwork default, and Back matching Escape.
This is not the Arch CI Qt. Operator tests were not run. CI has not run for
this change.
