# Media MVP: folders, library and external mpv

Part of [ADR-0024](../adr/0024-local-library-mpv.md). The scenario was chosen by the project owner (P-02); components and technical defaults are a delegated choice.

Discovery dates belong to the same provider SQLite database. `item_additions`
stores each film/episode UUID's first successful discovery in Unix microseconds,
inside the catalogue update transaction. Scan/import producers finish their disk work before acquiring a shared publication permit; under that permit they read the latest database snapshot and publish it, so a delayed producer cannot overwrite a newer publication. Rescans, renames, metadata/progress edits,
imports retried with the same completion id and restarts retain it. Existing
pre-additions rows migrate to zero (unknown); opening an old database does not
make its entire library new. Unavailable content retains its date but is omitted
while its top-level film/series is unavailable. A series uses the latest episode
discovery. The provider publishes its latest 64 top-level dates together with
the catalogue through the SDK's atomic additions replacement (ADR-0118,
[ADR-0120](../adr/0120-home-recent-library-additions.md)).

## Providers

`arinn-plugin-library` implements LibraryProvider and scans configured roots on the local filesystem, including already mounted network folders. No own SMB/NFS client, mount management or Jellyfin server is needed for the MVP. Roots come from `ARINN_PROVIDER_SETTINGS` at provider start ([ADR-0056](../adr/0056-local-library-scan-metadata-and-resume.md), [ADR-0064](../adr/0064-provider-settings-in-config.md)). Scanning is asynchronous: initial scan, explicit rescan (`SIGHUP`), filesystem hints and a periodic check every 10 minutes when autoscan is on. The snapshot already published keeps being served while a scan runs. ffprobe runs two at a time with a 5 second deadline. Parsing and artwork ids are ADR-0056. Anime `TVn` season markers are [ADR-0085](../adr/0085-anime-tv-season-markers.md); a standalone `Snn` season marker (`Show Name [WEB-DL 1080p] S01/`, `S02/`, `Show S01 - 05.mkv`) follows the same rules ([ADR-0109](../adr/0109-season-only-snn-markers.md)).

The library metadata and resume database is the provider's own state, not the core's. A stable item UUID is stored in its SQLite database; matching inode/size/mtime helps detect renames only inside an available root and is not a global ID. An unavailable mount makes the root unavailable; films are never marked deleted because of it. That covers a root that cannot be read. Since [ADR-0115](../adr/0115-unmounted-share-root-is-unavailable.md) it also covers an empty root whose nearest `/etc/fstab` mount point has nothing mounted on it, or between it and the root, by `/proc/self/mountinfo`, compared in canonical form. That is the empty mount point a `nofail` NAS entry leaves behind. A folder with files, or a folder no fstab entry covers, keeps the readable test, so a folder the user empties stays available. An import under an unavailable root is refused rather than filed under an outer root. The provider never mounts anything. Symlinks outside configured roots are not followed automatically. "Remove from library" hides the entry and never deletes the file.

**Settings and root availability** ([#551](https://github.com/LimeBeck/arinn/issues/551), [ADR-0065](../adr/0065-provider-settings-contract.md), [ADR-0071](../adr/0071-settings-editor-contract.md)). The provider exports `ProviderSettings1`, so Settings › Media edits its values. The schema has three settings:

- `roots`, *Library folders*: a `path` with `directory` and `multiple`, at most 64, default empty;
- `autoscan`, *Rescan regularly*: a boolean, default true;
- `probe`, *Read video details*: a boolean, default true.

Titles and descriptions are provider text in English, shown as sent, like the other providers' settings. `ffprobe` stays a hand-edited key in `config.toml` and is not in the schema.

Settings are still read once at start (ADR-0056), so `Apply` answers `Unsupported` for valid values: the daemon keeps them and the shell shows that they apply at the next start. `Apply` answers `InvalidArgument`, and the daemon restores the previous file, for a root the provider would refuse at start, such as `/`.

The provider also lists `LibraryRoots1` and publishes every configured root exactly as written, with whether the last scan could read it. The same unreadable-root check marks the root's items unavailable. A text that is not a valid settings path, or that repeats one already listed, is not reported. The SDK announces a changed list only, so a rescan that finds the same state emits nothing. Settings that do not load report no roots. Paths are not logged.

MVP metadata: title, year, season and episode from the file name and sidecar NFO; duration and languages from ffprobe when it is installed; local poster and fanart next to the video become asset ids. When data is missing: the file name, an empty artwork id and unknown fields — never an invented plot or year. NFO XML is parsed without external entities or network DTDs. A long item is completed by a short playback step into the final 90 seconds, or by confirmed EOF; a seek does not complete it. Online enrichment is not required for a usable MVP and may be added later through provider configuration. This fallback works with any folder and needs no API key.

`arinn-plugin-mpv` implements MediaProvider plus additional typed playback interfaces. mpv is a separate player process with a regular video window. Commands and events go through a private Unix JSON IPC socket in `XDG_RUNTIME_DIR` (directory 0700, no public `/tmp` endpoint). The shell never touches the socket and embeds no media engine. mpv offers JSON IPC and a track list, which is why it was chosen ([mpv manual](https://mpv.io/manual/stable/)). The process, the command shapes and the thumbnail extractor are [ADR-0057](../adr/0057-mpv-ipc-and-process-scope.md) and [below](#mpv).

## Shell OSD and platform

mpv draws the video; Arinn draws the OSD and track sheets through the platform overlay. Two modes must be verified: OSD hidden — the shell's transparent input layer has focus and the video stays visible; OSD visible — the shell has focus and draws the OSD. Seek must not be duplicated between mpv bindings and the shell. The shipped mpv configuration disables the default key bindings and the OSC, so mpv never acts on keys itself. No global plain-letter keys control other applications.

Input does not travel through mpv or the provider: the shell stays focused as a transparent fullscreen input layer over the mpv window, handles every key locally and sends only commands to the provider ([ADR-0046](../adr/0046-shell-input-layer-during-playback.md), specification §6.4–§6.5).

This is an extra integration gate before the media UI: an overlay over video without losing playback, correct input, audio/subtitle sheets and Home/Return. Track selection is confirmed by the mpv API; the QML overlay on the target HTPC is not yet confirmed.

## Contract and resume ownership

The MediaProvider base stays MPRIS-compatible. Rich features are separate typed interfaces with feature flags `MediaTracks1`, `MediaTimeline1` and `MediaOpen1` ([API review](../plugin-api/mvp-extensions.md), [wire contract](../adr/0054-media-and-library-wire-contract.md)). The MPRIS provider does not have to declare them. A missing feature is hidden or disabled in the UI, never a no-op.

Library launch returns a content `EntityRef` and calls the selected `MediaOpen1` provider through daemon routing. The daemon forwards progress of library-linked sessions to `SetProgress` on first sight, on play/pause changes, on seeks over 2 s, every 5 s while playing and on stop; `ended` is sent only for the `ended` session state, a player-confirmed end of file ([ADR-0061](../adr/0061-playback-progress-forwarding.md), [ADR-0063](../adr/0063-ended-session-state.md)). The media session reports the same content ref. The library provider receives progress through a daemon subscription and writes resume every 5 s and on pause, seek, end and stop. Only the library is the persistent source of truth; mpv watch-later never becomes a second resume store for Arinn-managed sessions.

Resume > 0 shows as "Continue". The design's 90-second rule applies to regular films and episodes longer than 180 s; for short or unknown durations completion requires a confirmed EOF. A manual seek to the end never marks watched without playback/EOF confirmation. The next episode is set to position 0 only when the season/episode order is unambiguous. Auto-next is a separate setting. A missing file or mount never erases resume.

Chapters and buffered ranges are shown only when data exists. Seek thumbnails come from a separate asynchronous, rate-limited, cancellable extractor and cache in the provider (for example ffmpeg); it never seeks the main mpv to get an image. The timeline and time stay usable until a thumbnail is ready. Skip intro is available only with real chapter or segment metadata; there is no own intro detector.

## Verification

AC-15, AC-16: local file, mounted network file, missing file, mount loss and recovery, two audio tracks, external and embedded subtitles, pause/seek/restart resume, end of film and episode advance. A provider crash does not close the shell; the player runs in its own application scope and reconnecting to an Arinn-owned player never kills other mpv instances.

Codecs, hardware decoding, HDR and passthrough are not guaranteed by documentation; they are checked on the target HTPC. Library/parser and IPC contract tests run on the development machine.

## mpv

`arinn-plugin-mpv` is the `media` provider for one Arinn-owned player (AC-16, [ADR-0057](../adr/0057-mpv-ipc-and-process-scope.md)). `Features` lists `MediaOpen1`, `MediaTracks1`, `MediaTimeline1`, `AssetProvider1` and `MediaWindow1`. mpv is an optional install. When `/usr/bin/mpv` is missing and no live IPC player is reachable, the backend is `unavailable` and `Open` returns `Unavailable`. Successfully observing an existing player restores availability even if the launcher executable is now absent; reconnect does not launch or reload it.

The player is the transient user unit `app-arinn-mpv.service` in `app.slice`, without `AddRef`, so it is not in the plugin cgroup and a provider crash does not stop it. An active unit is left running. The socket is `$XDG_RUNTIME_DIR/arinn/mpv/ipc` in a `0700` directory. `/tmp` is refused. Reconnecting observes mpv and does not send `loadfile` again, and it does not signal any other mpv.

`Open` accepts a `file:///` URI, a start position and preferred audio and subtitle languages. It returns one session id. A later open replaces that session. The session carries the content `EntityRef`. A missing or unreadable file does not open successfully: `end-file` with reason `error` returns `NotFound` when the player says the file is missing, and `Unavailable` otherwise. If the file has not loaded within 20 s, `Open` sends `stop` and returns `Unavailable`. Play, pause, play-pause, seek and stop are the actions while `playing` or `paused`. While `ended`, the actions are seek and stop. After explicit Stop (including Back that leaves playback), the provider freezes and samples the final position, retains the last video frame, sends `quit`, and waits for the owned process/unit to exit. The session stays with state `stopped`, content, position and artwork, and no actions; live window identity is cleared. Continue reopens through the library into a fresh process. Pause and global Home preserve the live process. Preparation, quit and exit confirmation fit the five-second control budget ([ADR-0083](../adr/0083-managed-player-stop.md)); errors remain visible and retryable in the shell. A confirmed end of file (`end-file` with reason `eof`, or `eof-reached`) reports `ended`. A seek does not. `anchor_us` is `CLOCK_MONOTONIC` in microseconds, published on the first sample, on a state change, on a seek, and at most once per second while `playing`.

The shipped mpv arguments disable configuration, scripts, auto profiles, default bindings, the on-screen controller, the OSD, the stats overlay, the OSD console, media keys, youtube-dl and watch-later. The owned player requests fullscreen at process startup, before its first video window maps; it does not wait for the later exact-window Present handoff to resize from a normal window. Present still owns shell focus and stacking, and an already-active owned unit is reused. `loadfile` uses the mpv 0.38 argument order, and if that command is rejected, the pre-0.38 order once.

Audio and subtitle tracks come from mpv's `track-list`. Selecting a track, or turning subtitles off, is confirmed from that list and fails when mpv does not apply it. Subtitles off sets `sid` to `no`. Chapters come from `chapter-list` and buffered ranges from `demuxer-cache-state`. A seek thumbnail is one frame from a separate ffmpeg process, read as `file:<path>` with `-protocol_whitelist file`. The cache keeps the newest 8 frames. It never seeks the playback mpv. A newer request cancels the one in flight. No video track, no ffmpeg, or a failed extract is `Unsupported`. The frame is fetched with `AssetProvider1.GetAsset` ([ADR-0062](../adr/0062-asset-contract.md)): the asset id is the canonical UUID that names the cached JPEG, and any other id is `NotFound`. The session card on Home shows a frame of the current position: a second extractor, so seek thumbnails are never cancelled, captures one about every 30 s of playback and once when the session is paused or stopped at a new position, stores it under `posters/`, and publishes its id as the session `artwork`.

On Stop, the provider captures the displayed frame with private IPC `screenshot-to-file` (`video`, without subtitles or OSD), validates it against the asset limit, and uses the separate extractor to resize it to 320 pixels wide. Capture has a one-second combined deadline and cannot prevent player shutdown. A failure retains the previous valid asset. Temporary captures are retained through confirmed player exit so a late IPC screenshot cannot recreate a deleted file; a failed Stop retains one guard until exit retry. IPC EOF after quit does not turn a failed unit-exit check into a stopped session: only Stop remains advertised so its shutdown check can be retried without sending commands to a closed peer. Interrupted extracts are deleted even when their future is dropped, stopped sessions do not restart periodic extraction, and late poster replies cannot replace final artwork. Frames remain local and never enter diagnostic telemetry. The shell waits for Stop operation success before releasing presentation and exposing Home; a failure keeps the player route with retry feedback. The reported Home/OSD overlap is covered at the shell routing boundary; real compositor qualification is still pending.

The runtime file that remembers the session id and the content reference does not contain the file path or the title. Neither is logged.

### MPRIS export

While its session is playing or paused, the mpv provider is an MPRIS player itself: `org.mpris.MediaPlayer2.arinn_mpv.instance<pid>` at `/org/mpris/MediaPlayer2` ([ADR-0081](../adr/0081-mpris-export-of-managed-playback.md)). Plasma sends the media keys to the current MPRIS player, so the remote's play/pause reaches Arinn playback this way; the shell never sees that key. The calls map onto the session's media actions:

- `PlayPause`, `Play`, `Pause`, `Stop`, `Seek` and `SetPosition`;
- `Next` and `Previous` when the session allows them;
- `OpenUri` is not supported.

A seek emits `Seeked`. The name is released when the session is stopped or ended or none is left.

## MPRIS

`arinn-plugin-mpris` is the `media` provider for players already on the session bus (AC-09, [ADR-0013](../adr/0013-mpris-media.md), [ADR-0054](../adr/0054-media-and-library-wire-contract.md)). It watches well-known names under `org.mpris.MediaPlayer2.` and publishes one session per name through `org.arinn.MediaProvider1`. The session id is the instance suffix of that name. There is no per-player special case. Names under the reserved prefix `org.mpris.MediaPlayer2.arinn_` are Arinn's own players and are skipped ([ADR-0081](../adr/0081-mpris-export-of-managed-playback.md)). `Features` is empty: the plugin does not export `MediaOpen1`, `MediaTracks1` or `MediaTimeline1`. An empty player list leaves the backend `available` and the snapshot empty.

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

Managed window identity and acknowledged presentation routing follow [ADR-0074](../adr/0074-managed-playback-window.md) and the [managed playback contract](managed-playback.md). The producer recovers the exact Wayland application ID through its private player IPC on reconnection. Unmanaged players retain ordinary media controls but expose no managed window.

During owned-player shutdown recovery, a retained Stop-only session does not
keep the MPRIS bus name. Media-key routing requires a playing or paused session
with Play, Pause or PlayPause available, so an IPC-disconnected retry cannot
intercept keys intended for another player.
