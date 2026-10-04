# ADR-0057: mpv IPC, process scope and thumbnails

- **Status:** Accepted
- **Date:** 2026-09-30
- **Deciders:** delegated technical choice ([ADR-0037](0037-agent-autonomy-and-owner-gated-decisions.md)) within [ADR-0024](0024-local-library-mpv.md), [ADR-0046](0046-shell-input-layer-during-playback.md) and [ADR-0054](0054-media-and-library-wire-contract.md)
- **Amends / Supersedes:** amends [ADR-0024](0024-local-library-mpv.md)
- **Amended by / Superseded by:** [ADR-0083](0083-managed-player-stop.md) (proposed)
- **Specification:** [§6.4](../specifications/arinn-specification.md#64-player), [§7.4](../specifications/arinn-specification.md#74-local-library-and-mpv), [§13](../specifications/arinn-specification.md#13-security)

## Context

[ADR-0024](0024-local-library-mpv.md) chooses an external mpv process and a private JSON IPC socket. It does not fix the unit name, the command shapes, or how a thumbnail is produced. The wire contract is already [ADR-0054](0054-media-and-library-wire-contract.md). This ADR records the provider's technical defaults.

## Decision

`arinn-plugin-mpv` exports `MediaProvider1`, `MediaOpen1`, `MediaTracks1` and `MediaTimeline1`.

**Process.** One Arinn-owned mpv runs as the transient user unit `app-arinn-mpv.service` in `app.slice`. `AddRef` is false, so the unit outlives the provider connection and a provider crash does not stop playback. Standard output and error are discarded. The unit is started only when it is not already active. No other mpv process is signalled. When the mpv binary is missing, the backend stays `unavailable` and `Open` returns `Unavailable`.

**Socket.** The IPC socket is `$XDG_RUNTIME_DIR/arinn/mpv/ipc`. That directory is mode `0700`. A runtime directory of `/tmp`, or any path under `/tmp`, is refused.

**IPC.** The provider speaks mpv's JSON IPC. `Open` sends `loadfile` with the mpv 0.38 shape (`replace`, insertion index `-1`, then a string map of `start`, `alang` and `slang`). If mpv answers `invalid parameter`, the same `Open` sends the pre-0.38 shape once. A reconnect observes properties and does not send `loadfile` or repeat a command whose peer already failed. mpv is started with `--no-config`, scripts, auto profiles, default bindings, the OSC, the OSD, the stats overlay, the OSD console, media keys, youtube-dl and watch-later disabled (`--ytdl=no`, `--load-stats-overlay=no`, `--load-osd-console=no`, `--load-auto-profiles=no`), so it never acts on keys and does not fetch remote media.

**Session.** One session. A later `Open` replaces it. The session id and the content `EntityRef` are stored in the runtime directory, mode `0600`. The file path and the title are not stored and are not logged. `Open` waits up to 20 s for `file-loaded` or an error `end-file` after the unit and socket are up. If that wait expires it sends `stop` and returns `Unavailable`, so a late load does not keep playing with no session. Play, pause, play-pause, seek and stop are the actions while `playing` or `paused`. While `ended`, the actions are seek and stop: unpause at end of file succeeds in mpv and changes nothing. Stop leaves the session `stopped` with an empty action list. A confirmed end of file (`end-file` with reason `eof`, or `eof-reached`) reports `ended` instead of `paused`. A seek does not. An `end-file` with reason `error` before the file has loaded fails `Open`: `NotFound` when the player reports that the file is missing, and `Unavailable` otherwise. Position anchors follow the same rule as the MPRIS provider: a new anchor on the first sample, on a state change and on a seek, and at most once per second while `playing`.

**Tracks and timeline.** Audio and subtitle tracks come from `track-list`. Selecting a track, or turning subtitles off, waits until `track-list` shows that choice and fails when it does not. `sid` `no` is subtitles off. Chapters come from `chapter-list`. Buffered ranges come from `demuxer-cache-state`.

**Thumbnails.** A separate ffmpeg process extracts one frame. The input is `file:<path>` with `-protocol_whitelist file`. It never seeks the playback mpv. One extraction runs at a time; a newer request cancels the older one. The cache keeps the newest 8 frames. No video track, no ffmpeg, or a failed extract is `Unsupported`.

## Consequences

- The provider can be tested against a fake JSON peer and, when mpv is installed, against `mpv --vo=null --ao=null`.
- The Rust CI image does not install mpv, so that real-player test skips there.
- The shell still cannot fetch thumbnail bytes. The result is an asset id, as in [ADR-0058](0058-media-routing.md).

## Alternatives considered

- **Spawn mpv as a child of the plugin.** Rejected. A provider crash would kill playback, and the player would share the plugin cgroup.
- **`AddRef` on the transient unit.** Rejected. Dropping the provider's bus connection would stop the unit.
- **Seek the main mpv for a thumbnail.** Rejected. It moves the picture the user is watching.
- **One `loadfile` shape.** Rejected. mpv 0.38 added an insertion index and mpv 0.37, which is what the development machine has, rejects that shape.

## Verification

- Fake IPC peer: open, position anchors, tracks, subtitles off, chapters, buffered ranges, `Unsupported` thumbnails, a peer crash and reconnect without replaying `loadfile` or the failed seek.
- Fake systemd: the unit name, `AddRef` false, input flags, and no second start while the unit is active.
- Real mpv 0.37 with `--vo=null --ao=null` on a generated file: open, pause, seek, tracks, subtitles off, stop. ffmpeg writes a JPEG thumbnail.
- Not run in CI's Rust job: that image installs dbus and Python only, not mpv.
