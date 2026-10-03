<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Player screen

Fixture presentation of the playback OSD. `PlayerView` draws a logical
1920×1080 surface and emits intents. A media provider still owns the picture,
the transport and the track change. This screen does not start a player
process, open a socket or call the daemon.

Behaviour follows specification
[§6.4](../specifications/arinn-specification.md#64-player) and
[§7.4](../specifications/arinn-specification.md#74-local-library-and-mpv),
and the input path in
[ADR-0046](../adr/0046-shell-input-layer-during-playback.md).
[§7.4](../specifications/arinn-specification.md#74-local-library-and-mpv)
says the shell draws the OSD and track lists while the external player draws
video. Resume, watched state and the real track list stay with the providers.

## Fixture contract

| Property | Meaning |
| --- | --- |
| `title`, `episodeLine` | Title at the top left, and an optional line under it |
| `clockText` | Top-right clock. Empty uses the local time as display only |
| `position`, `duration` | Seconds. The screen displays them and does not write them |
| `buffered`, `chapters` | Optional buffer end, in seconds, and chapter positions. Absent data draws nothing extra |
| `playing` | Centered pause badge when false, and whether the OSD may auto-hide |
| `seekSupported` | When false, Left/Right on the timeline do not emit `seek` |
| `audioTracks`, `subtitleTracks` | Lists of `{id, label}`. An optional `detail` is the sheet subline |
| `audioTrackId`, `subtitleTrackId` | Selected ids supplied by the fixture. The pills show those names |
| `playbackState` | `ready`, `loading`, `error` or `unavailable` |
| `pictureStandIn` | Defaults to true. The full-bleed stand-in. False leaves that layer transparent so a later binding can sit over the video window ([ADR-0046](../adr/0046-shell-input-layer-during-playback.md)) |
| `thumbnail` | Optional `image://arinn-assets/` URL. The timeline plate and its label stay until that image is ready, and stay when the URL is empty or the fetch fails |
| `osdVisible` | Local chrome. Hiding the OSD does not change the picture stand-in |
| `osdIdleMs` | Defaults to 4000. Local hide delay, not a provider timeout |

Signals, and only these, leave the screen: `play`, `pause`,
`seek(targetSeconds)`, `selectAudio(trackId)`, `selectSubtitle(trackId)`,
`stop`, `leave`, `homeRequested`. Emitting one does not change the fixture.
The caller applies a provider result by writing the properties. Home closes
local chrome and emits `homeRequested`; it does not pause or stop playback.

The ready surface is the handoff OSD: a full-bleed picture stand-in when
`pictureStandIn` is true, top and bottom scrims, the timeline along the bottom,
and a row of Subtitles, Audio and More pills. Pill and sheet fills use
`Theme.plate` and `Theme.plateFocus`. A focused timeline shows a seek plate.
`thumbnail` is an Assets1 image URL from the shell image cache. The plate,
labelled thumbnail, stays until that image is ready. An empty URL, `NotFound`,
`Unsupported` or any other error leaves the plate. Requesting the thumbnail
id is `MediaCatalog`, not this screen.

`tracksSupported` and `timelineSupported` default to true so this fixture
still draws the pills, chapters, buffer and thumbnail plate. A production
binding sets them from the provider flags. When `tracksSupported` is false
the audio and subtitle pills are hidden, focus skips them, and OK does not
open a sheet. When `timelineSupported` is false the chapter marks, buffer
and thumbnail plate are hidden. The position timeline and seek stay; base
seek does not depend on `MediaTimeline1`.
`loading`, `error` and `unavailable` show a status message and ignore transport
keys. An empty audio or subtitle list labels that pill
unavailable. OK on it does not open a sheet and does not emit a select signal.
OK on a pill that has tracks opens a sheet; OK on a row emits the select
signal and closes the sheet, returning focus to that pill. More does not emit
a transport signal.

## Keys

The screen keeps keyboard focus, including while the OSD is hidden. Keys are
handled here. Nothing in this file forwards them to a player. During managed
playback the shell handles media keys
([ADR-0046](../adr/0046-shell-input-layer-during-playback.md)); outside the
shell, Plasma does (OQ-04).

| OSD | Key | Result |
| --- | --- | --- |
| Hidden | Left/Right | `seek` ±10 s when seek is supported, then show the OSD focused on the timeline. Without seek support, show the OSD and do not emit `seek` |
| Hidden | OK, Up, Down | Show the OSD on the timeline. No seek, play or pause |
| Visible | OK on the timeline | `pause` when `playing` is true, otherwise `play` |
| Visible | Up/Down | Move between the timeline and the pill row. No seek and no toggle |
| Visible | Left/Right on the timeline | Same seek rule. The OSD stays open |
| Visible | Left/Right on the pills | Move among the visible pills. With tracks supported, those are Subtitles, Audio and More. No seek |
| Visible | OK on Subtitles or Audio | Open that sheet when the list is non-empty. No select signal yet |
| Sheet | Up/Down or Left/Right | Move the highlight. No select signal |
| Sheet | OK | `selectAudio` or `selectSubtitle`, then close the sheet onto its pill |
| Sheet | Back, Esc, Backspace | Close the sheet onto its pill. The OSD stays. No `leave` and no `stop` |
| Sheet or OSD | Home | Close an open sheet without selecting a track, then `homeRequested`. No `leave`, `stop`, `play` or `pause`. Playback stays as it is |
| Visible, no sheet | Back, Esc, Backspace | Hide the OSD. No `leave` and no `stop` |
| Hidden | Back, Esc, Backspace | `leave`. Playback properties stay as they are. No `stop` |
| Either, `ready` | Media play, pause, play/pause, stop | The matching signal, then show the OSD if it was hidden |

The OSD hides `osdIdleMs` after the last input while `playing` is true, and
never while paused. The timer only clears `osdVisible`. It does not emit
`play`, `pause`, `seek`, `stop` or `leave`, and it is not the first command.

## Verification

[`tests/ui/tst_player.qml`](../../tests/ui/tst_player.qml) imports
[`PlayerView.qml`](../../apps/arinn-shell/ui/screens/player/PlayerView.qml)
by filesystem path. The existing `ui-components` CTest passes `-input tests/ui`
to `arinn-ui-test`, which loads every `tst_*.qml` on the offscreen platform.
Back, Esc, arrows and OK go through `keyClick`. Qt Test aborts on media-key
codes, so those codes call the same key handler directly. No operator test
is involved.

This does not pass AC-16 or the media integration gate. There is no video
window in these tests. The plate that stays until the image is ready is
checked by `asset-images`.

## Production binding

`MediaCatalog` reads `org.arinn.Media1` for the selected session
([ADR-0058](../adr/0058-media-routing.md)). `Features` is read with the
provider list. Unknown flags are dropped. A read error is an empty flag
list, so the session itself still loads.

| Flag | Binding |
| --- | --- |
| none of `MediaOpen1`, `MediaTracks1`, `MediaTimeline1`, `MediaWindow1` | Continue opens `SessionView`. Track and timeline reads are not called |
| `MediaOpen1`, `MediaTracks1`, `MediaTimeline1` or `MediaWindow1` | Continue opens this screen. `pictureStandIn` is false |
| `MediaTracks1` | `ListTracks`, `SelectTrack` and `DisableSubtitles`. The subtitle sheet starts with Off, a shell-only id. Choosing it calls `DisableSubtitles` and never `SelectTrack` |
| `MediaTimeline1` | `GetTimeline` supplies chapter positions and the buffered range that contains the playhead. `RequestThumbnail` runs when the timeline is focused and again for a seek target. The operation result's `local_id` becomes the `thumbnail` URL. A failure leaves the previous URL |

A Continue card for a **stopped** session whose content reference has the
`library` capability opens neither screen. A stopped session has nothing to
present (its launch id is empty, see [managed playback](managed-playback.md)),
so `Main.qml` calls `LibraryCatalog.launchContent(provider, local)`, which is
`Library1.Launch` for that item, and the library applies the stored resume
position. The player then opens from `sessionLaunched`, as for a library
Play. When the library catalog is still being read, the launch waits for the
read to finish. A failure (`NotFound`, `Unavailable`, `Timeout` and so on)
shows the library's operation message on Home.

A library launch opens this screen when the provider lists one of those
flags, and also while the new session is not in the snapshot yet. When that
row arrives without the flags, production uses `SessionView` instead. The
shell never sends a URI.

Transport keys stay in this screen ([ADR-0046](../adr/0046-shell-input-layer-during-playback.md)).
Each seek emits one `seek` and `MediaCatalog` sends one `Seek`. The
thumbnail request is a different method. Back hides the OSD, and the next
Back emits `leave`; production then sends one `Stop` and returns to Home or
the library detail. Home emits `homeRequested` and does not stop playback.

`apps/arinn-shell/media_catalog_test.cpp` drives a fake `Media1` with and
without the flags, including one `Seek` beside `RequestThumbnail`. The
focus and sheet cases above cover a missing flag. Neither is the media
integration gate.

## Managed input layer

`MediaWindow1` adds the native video presentation binding from
[ADR-0074](../adr/0074-managed-playback-window.md).
`MediaCatalog` waits up to five seconds for `GetWindow`, then submits one
`Present` and observes its Operation1 acknowledgement. Presentation has its own
request ticket, separate from transport and thumbnail requests. Calls use the
current unique daemon owner and the original provider registration/session.
Cancelled reads do not submit a late Present. Presentation intent is consumed
once per explicit open; ordinary snapshots cannot retry a failed or abandoned
presentation. Operation observation failure/timeout tears down the remembered
scope, even when compositor dispatch may have already occurred. Back sends one Stop and a separate
Release, retaining the session reference even if Stop removes the session.
Release cleanup has separate remembered scope until acknowledgement succeeds.
Admission errors, failed/unobservable operations and timeouts retain that scope
for an explicit retry with a fresh request ID and surface cleanup failure. There
is no automatic retry loop; new Present intent or daemon/provider replacement
invalidates stale retry state.

The shell remains fullscreen and transparent with PlayerView focused while
playing, including when the OSD hides. Home calls the daemon Home intent and
shows an opaque Home over the same player without stopping it. The global
HomeRequested signal also closes playback chrome, scoped to the current daemon
epoch. Return reopens the same PlayerView. The library origin survives Home and
Return so Back can restore its detail. Focus outside managed playback, window
loss, provider replacement, lock or unknown lock state relinquishes presentation;
SessionLifecycle remains an independent visibility guard. MediaWindow1.Changed
revalidates the original WindowRef without submitting another Present.

`media-catalog` exercises Present acknowledgement, cancellation before window
mapping, epoch-scoped Home, Home/Return without Stop, and Back's independent
Stop/Release. The adapter suite checks exact native identity, temporary flags,
output placement, focus loss and lock/late-callback fencing. These are synthetic
checks; G-4.1 still needs owner-run KDE Wayland/TV qualification.

## Owner TV trial

Use a build/package containing the managed playback producer and #141 adapter
and shell changes, on KDE Plasma Wayland with mpv installed. Restart the Arinn
session after updating so daemon, shell and adapter use the same contract. An
older running `app-arinn-mpv.service` has no launch app ID; stop that old Arinn
player before launching the test film. Provider restart alone preserves its
process and cannot retrofit an app ID. These steps change the real session and
are for the owner, not an agent-run test.

1. Open a local library film on the TV output. Check visible picture and OSD,
   then pause, seek and choose audio/subtitles with arrows and OK.
2. Let the OSD hide. Check arrows/OK still reach Arinn and reveal controls.
3. Open global Home during playback. Check opaque Home covers video without
   pause or restart, then Return restores controls over the same film/position.
4. Back hides the OSD; another Back stops playback and returns to its library
   detail. Check another application's ordinary window flags/focus remain usable.
5. Record KDE/Qt versions, output layout and results for #141/G-4.1. Qualification
   is pending until this owner-run evidence exists.

Late Present admission cleanup is detached from the current session's Release acknowledgement and retry state; its outcome cannot erase another session's remembered cleanup.

A failed Release exposes an explicit Back retry on Home even without a Return target. The retry uses a fresh request ID and the remembered cleanup scope; it sends no Stop. Home navigation and the shortcut editor consume their normal Back action before cleanup retry becomes available. Successful cleanup restores ordinary Home Back behavior.
