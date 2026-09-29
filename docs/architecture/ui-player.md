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
| `osdVisible` | Local chrome. Hiding the OSD does not change the picture stand-in |
| `osdIdleMs` | Defaults to 4000. Local hide delay, not a provider timeout |

Signals, and only these, leave the screen: `play`, `pause`,
`seek(targetSeconds)`, `selectAudio(trackId)`, `selectSubtitle(trackId)`,
`stop`, `leave`. Emitting one does not change the fixture. The caller applies
a provider result by writing the properties.

The ready surface is the handoff OSD: a full-bleed picture stand-in when
`pictureStandIn` is true, top and bottom scrims, the timeline along the bottom,
and a row of Subtitles, Audio and More pills. Pill and sheet fills use
`Theme.plate` and `Theme.plateFocus`. A focused timeline shows a seek stand-in
labelled thumbnail.
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
| Visible | Left/Right on the pills | Move among Subtitles, Audio and More. No seek |
| Visible | OK on Subtitles or Audio | Open that sheet when the list is non-empty. No select signal yet |
| Sheet | Up/Down or Left/Right | Move the highlight. No select signal |
| Sheet | OK | `selectAudio` or `selectSubtitle`, then close the sheet onto its pill |
| Sheet | Back, Esc, Backspace | Close the sheet onto its pill. The OSD stays. No `leave` and no `stop` |
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

This does not pass AC-16 or the media integration gate. There is no provider,
resume store or video window.
