<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Now-playing session

Presentation for [specification §7.3](../specifications/arinn-specification.md#73-media-sessions-mpris)
and [§5.1](../specifications/arinn-specification.md#51-home) Continue.
`MediaCatalog` in `apps/arinn-shell/media_catalog.cpp` reads
`org.arinn.Media1` on each ready `media` provider path ([ADR-0058](../adr/0058-media-routing.md)).
The screen and Home do not talk to MPRIS, and they do not send a file URI.

This is the shell half of AC-09. Operator playback on a real session is not
part of this change. Session artwork is an asset id. The shell loads those
bytes through `Assets1` ([#162](https://github.com/LimeBeck/arinn/issues/162)).

## Model

Reads are asynchronous. `GetSessions` is paged. A repeated page token, a
page generation other than the provider's, a revision that is not the first
page's, or an invalid session drops that provider. A revision starts at 1. If every media provider fails, the session state is `unavailable`.
No media provider is `empty`. The daemon disappearing is `unavailable`.

Sessions are ordered for Continue: `playing`, then `paused`, then any other
state. Within a state, the newer `anchor_us` comes first. The first session is
selected until the user picks a Continue card. A picked session that leaves
the snapshot is `unavailable`; the model does not switch to another one.

The catalog subscribes to `Changed`, `ProvidersChanged` and `StateChanged`
before the first read, and those signals reload the snapshot. A 30 s timer
is only the fallback for a missed signal. One pass reads `Status`, `Providers`
and every `GetSessions` page, so it is not repeated every second. Position
between snapshots is the local clock.

`position_us` is shown as published for every state except `playing`. While
`playing`, the model adds the elapsed `CLOCK_MONOTONIC` time since `anchor_us`
and clamps to a known duration. An anchor of 0 is not a sample: using it would
add the whole uptime and jump the position to the duration or to the int64
limit. An anchor ahead of the clock is not a sample either, so the published
position stays. `paused`, `stopped` and any other token do not advance.
Unknown actions are ignored. `play-pause` is presented as `playPause`.
An action is listed only when the session carries that token.

Commands (`Play`, `Pause`, `PlayPause`, `Seek`, `Next`, `Previous`, `Stop`)
are sent only when the selected session lists them. Otherwise the operation
state is `Unsupported` and nothing is called. A negative seek is
`InvalidArgument`. A method error, including `Unsupported`, is not success.
A returned object path stays `pending` until `Operation1.Get` reports
`succeeded`. The shell stops polling that object after 6 s, one second past
the 5 s control timeout, and the operation state is then `Timeout`. Playback
on screen changes only when a later snapshot says so.

A non-empty artwork id becomes an `image://arinn-assets/` URL for the
selected session and for each Continue card. `AssetImages` calls
`Assets1.GetAsset` off the UI thread and caches the compressed bytes by
(provider, generation, id), up to 64 MiB. An empty id, `NotFound`,
`Unsupported`, a bad image or any other error leaves the generic plate.
The picture is not decoded in QML.

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

`Main.qml` binds `operationState`. While it is `pending`, `queued` or
`running`, the transport is busy and a "Waiting…" line is shown. `succeeded`
and an empty state are quiet. Any other value shows an error banner:
`Unsupported`, `Timeout`, `InvalidArgument` and `Unavailable` use a short
sentence, and any other code is shown as itself.

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
an anchor of 0 and an anchor in the future (the position stays), `Unsupported`
without a call, success only after `Operation1.Get`, a poll that becomes
`Timeout` and then stops, a repeated page token, a revision or generation
that breaks the snapshot, a `StateChanged` refresh with no one-second re-read,
and the production Continue and session binding, including the pending line
and the error banner.

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

Artwork bytes are covered by `apps/arinn-shell/asset_images_test.cpp`
(`asset-images`, private bus): URL round trip, a cache hit, a budget
eviction, separate keys for provider and generation, and the empty,
`NotFound`, `Unsupported` and bad-image fallbacks. That test loads
`tests/ui/asset_placeholder.qml`. A held reply keeps the generic plate
until the image is ready, and the Continue card's pixels are then the
loaded artwork. A missing asset ends as `Image.Error` with a fixed
message, and the plates and labels stay. The file is not named
`tst_*.qml`, so `arinn-ui-test` does not load it on its own.

```sh
QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software \
  dbus-run-session -- build/apps/arinn-shell/arinn-asset-images-test
```

Local verification on 2026-09-30, Qt 6.11.2, offscreen platform, software
renderer, on this change: `arinn-asset-images-test` reported 7 passed
(including `placeholderStaysUntilTheImageArrives` and
`missingAssetKeepsThePlate`),
`arinn-media-catalog-test` reported 14 passed,
`arinn-library-catalog-test` reported 14 passed (including
`emptyPosterAndFanartStayGeneric`),
`arinn-browser-catalog-test` reported 10 passed,
`arinn-session-lifecycle-test` reported 9 passed, and
`arinn-ui-test -input tests/ui` reported 187 passed.
`python scripts/check-docs.py` and `python scripts/adr.py check` passed.
`arinn-catalog-test` and `tests/integration/phase1.py` were not re-run. This
is not the Arch CI Qt. Operator tests were not run. AC-09, AC-12 and G-5.1
are not marked passed.
