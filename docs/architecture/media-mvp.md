# Media MVP: folders, library and external mpv

Part of [ADR-0024](../adr/0024-local-library-mpv.md). The scenario was chosen by the project owner (P-02); components and technical defaults are a delegated choice.

## Providers

`arinn-plugin-library` implements LibraryProvider and scans configured roots on the local filesystem, including already mounted network folders. No own SMB/NFS client, mount management or Jellyfin server is needed for the MVP. Scanning is asynchronous: initial scan, explicit rescan, filesystem hints and a periodic check every 10 minutes when autoscan is on. Network I/O is bounded by a worker pool and deadlines and never blocks the core.

The library metadata and resume database is the provider's own state, not the core's. A stable item UUID is stored in its SQLite database; matching inode/size/mtime helps detect renames only inside an available root and is not a global ID. An unavailable mount makes the root unavailable; films are never marked deleted because of it. Symlinks outside configured roots are not followed automatically. "Remove from library" hides the entry and never deletes the file.

MVP metadata: title, year, season and episode from the file name and sidecar NFO; duration and track info from ffprobe; local poster and fanart next to the video. When data is missing: the file name, generic artwork and unknown fields — never an invented plot or year. NFO XML is parsed without external entities or network DTDs. Online enrichment is not required for a usable MVP and may be added later through provider configuration. This fallback works with any folder and needs no API key.

`arinn-plugin-mpv` implements MediaProvider plus additional typed playback interfaces. mpv is a separate player process with a regular video window. Commands and events go through a private Unix JSON IPC socket in `XDG_RUNTIME_DIR` (directory 0700, no public `/tmp` endpoint). The shell never touches the socket and embeds no media engine. mpv offers JSON IPC and a track list, which is why it was chosen ([mpv manual](https://mpv.io/manual/stable/)). The process, the command shapes and the thumbnail extractor are [ADR-0057](../adr/0057-mpv-ipc-and-process-scope.md) and [below](#mpv).

## Shell OSD and platform

mpv draws the video; Arinn draws the OSD and track sheets through the platform overlay. Two modes must be verified: OSD hidden — the shell's transparent input layer has focus and the video stays visible; OSD visible — the shell has focus and draws the OSD. Seek must not be duplicated between mpv bindings and the shell. The shipped mpv configuration disables the default key bindings and the OSC, so mpv never acts on keys itself. No global plain-letter keys control other applications.

Input does not travel through mpv or the provider: the shell stays focused as a transparent fullscreen input layer over the mpv window, handles every key locally and sends only commands to the provider ([ADR-0046](../adr/0046-shell-input-layer-during-playback.md), specification §6.4–§6.5).

This is an extra integration gate before the media UI: an overlay over video without losing playback, correct input, audio/subtitle sheets and Home/Return. Track selection is confirmed by the mpv API; the QML overlay on the target HTPC is not yet confirmed.

## Contract and resume ownership

The MediaProvider base stays MPRIS-compatible. Rich features are separate typed interfaces with feature flags `MediaTracks1`, `MediaTimeline1` and `MediaOpen1` ([API review](../plugin-api/mvp-extensions.md), [wire contract](../adr/0054-media-and-library-wire-contract.md)). The MPRIS provider does not have to declare them. A missing feature is hidden or disabled in the UI, never a no-op.

Library launch returns a content `EntityRef` and calls the selected `MediaOpen1` provider through daemon routing. The media session reports the same content ref. The library provider receives progress through a daemon subscription and writes resume every 5 s and on pause, seek, end and stop. Only the library is the persistent source of truth; mpv watch-later never becomes a second resume store for Arinn-managed sessions.

Resume > 0 shows as "Continue". The design's 90-second rule applies to regular films and episodes longer than 180 s; for short or unknown durations completion requires a confirmed EOF. A manual seek to the end never marks watched without playback/EOF confirmation. The next episode is set to position 0 only when the season/episode order is unambiguous. Auto-next is a separate setting. A missing file or mount never erases resume.

Chapters and buffered ranges are shown only when data exists. Seek thumbnails come from a separate asynchronous, rate-limited, cancellable extractor and cache in the provider (for example ffmpeg); it never seeks the main mpv to get an image. The timeline and time stay usable until a thumbnail is ready. Skip intro is available only with real chapter or segment metadata; there is no own intro detector.

## Verification

AC-15, AC-16: local file, mounted network file, missing file, mount loss and recovery, two audio tracks, external and embedded subtitles, pause/seek/restart resume, end of film and episode advance. A provider crash does not close the shell; the player runs in its own application scope and reconnecting to an Arinn-owned player never kills other mpv instances.

Codecs, hardware decoding, HDR and passthrough are not guaranteed by documentation; they are checked on the target HTPC. Library/parser and IPC contract tests run on the development machine.

## mpv

`arinn-plugin-mpv` is the `media` provider for one Arinn-owned player (AC-16, [ADR-0057](../adr/0057-mpv-ipc-and-process-scope.md)). `Features` lists `MediaOpen1`, `MediaTracks1` and `MediaTimeline1`. mpv is an optional install. When `/usr/bin/mpv` is missing, the backend is `unavailable` and `Open` returns `Unavailable`.

The player is the transient user unit `app-arinn-mpv.service` in `app.slice`, without `AddRef`, so it is not in the plugin cgroup and a provider crash does not stop it. An active unit is left running. The socket is `$XDG_RUNTIME_DIR/arinn/mpv/ipc` in a `0700` directory. `/tmp` is refused. Reconnecting observes mpv and does not send `loadfile` again, and it does not signal any other mpv.

`Open` accepts a `file:///` URI, a start position and preferred audio and subtitle languages. It returns one session id. A later open replaces that session. The session carries the content `EntityRef`. Play, pause, play-pause, seek and stop are the actions while a file is loaded. After stop the session stays, with state `stopped` and no actions. `anchor_us` is `CLOCK_MONOTONIC` in microseconds, published on the first sample, on a state change, on a seek, and at most once per second while `playing`.

The shipped mpv arguments disable configuration, scripts, default bindings, the on-screen controller, the OSD, media keys and watch-later. `loadfile` uses the mpv 0.38 argument order, and if that command is rejected, the pre-0.38 order once.

Audio and subtitle tracks come from mpv's `track-list`. Subtitles off sets `sid` to `no`. Chapters come from `chapter-list` and buffered ranges from `demuxer-cache-state`. A seek thumbnail is one frame from a separate ffmpeg process. It never seeks the playback mpv. A newer request cancels the one in flight. No video track, no ffmpeg, or a failed extract is `Unsupported`.

The runtime file that remembers the session id and the content reference does not contain the file path or the title. Neither is logged.

## MPRIS

`arinn-plugin-mpris` is the `media` provider for players already on the session bus (AC-09, [ADR-0013](../adr/0013-mpris-media.md), [ADR-0054](../adr/0054-media-and-library-wire-contract.md)). It watches well-known names under `org.mpris.MediaPlayer2.` and publishes one session per name through `org.arinn.MediaProvider1`. The session id is the instance suffix of that name. There is no per-player special case. `Features` is empty: the plugin does not export `MediaOpen1`, `MediaTracks1` or `MediaTimeline1`. An empty player list leaves the backend `available` and the snapshot empty.

A name stays tracked while it has an owner. Each player is read on its own task, apart from the snapshot loop and from `GetSessions`, so one player that never answers does not stop the others. A read that fails, times out, or does not validate is omitted from that snapshot when the player has never been read, or keeps the last good session when it has, and is tried again. The player is removed only when `NameOwnerChanged` reports no new owner or a fresh `ListNames` omits the name. Outgoing player calls are bounded so a stalled method does not stick.

Session fields come from `org.mpris.MediaPlayer2.Player`:

| MPRIS | Session |
| --- | --- |
| `xesam:title` | `title` |
| `xesam:artist`, or `xesam:album` when the artist list is empty | `subtitle` |
| `PlaybackStatus` `Playing` / `Paused` / `Stopped` | `playing` / `paused` / `stopped`; any other status is `unknown` and does not advance |
| `mpris:length` | `duration_us`, or −1 when missing |
| `Position` | `position_us`, clamped to a known duration |
| `mpris:trackid` | used only as the `SetPosition` argument; not a session field. `/org/mpris/MediaPlayer2/TrackList/NoTrack` is treated as missing |

`mpris:artUrl` is not read into the session and is not fetched. `artwork` stays empty. `has_content` stays false. Titles and URLs are not logged.

`CanControl` false is treated as every `Can*` flag false, so the session offers no commands. Otherwise the tokens follow the flags. `play-pause` requires `CanPause`, because `PlayPause` is specified to fail when the player cannot pause.

| Flag | Action token | Method |
| --- | --- | --- |
| `CanPlay` | `play` | `Play` |
| `CanPause` | `pause`, `play-pause` | `Pause`, `PlayPause` |
| `CanControl` | `stop` | `Stop` |
| `CanSeek` | `seek` | `SetPosition` when `mpris:trackid` is a real track path. A missing id, including the no-track path, uses `Seek`. The offset is the target minus the live `Position`, or minus the position extrapolated from the anchor while playing when that read fails |
| `CanGoNext` | `next` | `Next` |
| `CanGoPrevious` | `previous` | `Previous` |

A command whose token is absent from `actions` returns `Unsupported` and does not call the player. A failed player call returns `Unavailable`, not success.

`anchor_us` is `CLOCK_MONOTONIC` in microseconds. A new anchor is published on the first sample, on a playback-state change, on a seek (including a position jump while playing), and at most once per second while the state is `playing`. While that interval is open, the published position stays at the last anchor so the shell can interpolate.
