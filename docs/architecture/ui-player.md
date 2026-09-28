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
| `title` | Line shown on the OSD |
| `position`, `duration` | Seconds. The screen displays them and does not write them |
| `playing` | Play/pause glyph and whether the OSD may auto-hide |
| `seekSupported` | When false, Left/Right do not emit `seek` |
| `audioTracks`, `subtitleTracks` | Lists of `{id, label}` |
| `audioTrackId`, `subtitleTrackId` | Selected ids supplied by the fixture |
| `playbackState` | `ready`, `loading`, `error` or `unavailable` |
| `osdVisible` | Local chrome. Hidden video is a clear surface, not a stopped session |
| `osdIdleMs` | Defaults to 4000. Local hide delay, not a provider timeout |

Signals, and only these, leave the screen: `play`, `pause`,
`seek(targetSeconds)`, `selectAudio(trackId)`, `selectSubtitle(trackId)`,
`stop`, `leave`. Emitting one does not change the fixture. The caller applies
a provider result by writing the properties.

`ready` shows the transport OSD. `loading`, `error` and `unavailable` show a
status message and ignore transport keys. An empty audio or subtitle list is
labelled unavailable. OK on that row does not emit a select signal.

## Keys

The screen keeps keyboard focus, including while the OSD is hidden. Keys are
handled here. Nothing in this file forwards them to a player.

| OSD | Key | Result |
| --- | --- | --- |
| Hidden | Left/Right | `seek` ±10 s when seek is supported, then show the OSD focused on the timeline. Without seek support, show the OSD and do not emit `seek` |
| Hidden | OK, Up, Down | Show the OSD on the timeline. No seek, play or pause |
| Visible | OK on the timeline | `pause` when `playing` is true, otherwise `play` |
| Visible | OK on a track | `selectAudio` or `selectSubtitle` with that id |
| Visible | Up/Down | Move focus among the timeline, the audio list and the subtitle list |
| Visible | Left/Right on the timeline | Same seek rule. The OSD stays open |
| Visible | Left/Right on a list | Move the highlight. No select signal until OK |
| Visible | Back, Esc, Backspace | Hide the OSD. No `leave` and no `stop` |
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
