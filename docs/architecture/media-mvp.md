# Media MVP: folders, library and external mpv

Part of [ADR-0024](../adr/0024-local-library-mpv.md). The scenario was chosen by the project owner (P-02); components and technical defaults are a delegated choice.

## Providers

`arinn-plugin-library` implements LibraryProvider and scans configured roots on the local filesystem, including already mounted network folders. No own SMB/NFS client, mount management or Jellyfin server is needed for the MVP. Scanning is asynchronous: initial scan, explicit rescan, filesystem hints and a periodic check every 10 minutes when autoscan is on. Network I/O is bounded by a worker pool and deadlines and never blocks the core.

The library metadata and resume database is the provider's own state, not the core's. A stable item UUID is stored in its SQLite database; matching inode/size/mtime helps detect renames only inside an available root and is not a global ID. An unavailable mount makes the root unavailable; films are never marked deleted because of it. Symlinks outside configured roots are not followed automatically. "Remove from library" hides the entry and never deletes the file.

MVP metadata: title, year, season and episode from the file name and sidecar NFO; duration and track info from ffprobe; local poster and fanart next to the video. When data is missing: the file name, generic artwork and unknown fields — never an invented plot or year. NFO XML is parsed without external entities or network DTDs. Online enrichment is not required for a usable MVP and may be added later through provider configuration. This fallback works with any folder and needs no API key.

`arinn-plugin-mpv` implements MediaProvider plus additional typed playback interfaces. mpv is a separate player process with a regular video window. Commands and events go through a private Unix JSON IPC socket in `XDG_RUNTIME_DIR` (directory 0700, no public `/tmp` endpoint). The shell never touches the socket and embeds no media engine. mpv offers JSON IPC and a track list, which is why it was chosen ([mpv manual](https://mpv.io/manual/stable/)).

## Shell OSD and platform

mpv draws the video; Arinn draws the OSD and track sheets through the platform overlay. Two modes must be verified: OSD hidden — the video window receives input through an adapter that sends semantic events through the plugin and daemon; OSD visible — the shell has focus. Seek must not be duplicated between mpv bindings and the shell. The shipped mpv configuration disables conflicting default bindings and the OSC and defines Arinn actions. No global plain-letter keys control other applications.

The hidden-OSD input path conflicts with the local-focus principle ([specification §6.5](../specifications/arinn-specification.md)) and is tracked as [OQ-01](../specifications/arinn-specification.md#21-open-questions).

This is an extra integration gate before the media UI: an overlay over video without losing playback, correct input, audio/subtitle sheets and Home/Return. Track selection is confirmed by the mpv API; the QML overlay on the target HTPC is not yet confirmed.

## Contract and resume ownership

The MediaProvider base stays MPRIS-compatible. Rich features are separate typed interfaces with feature flags `MediaTracks1`, `MediaTimeline1` and `MediaOpen1` ([API review](../plugin-api/mvp-extensions.md)). The MPRIS provider does not have to declare them. A missing feature is hidden or disabled in the UI, never a no-op.

Library launch returns a content `EntityRef` and calls the selected `MediaOpen1` provider through daemon routing. The media session reports the same content ref. The library provider receives progress through a daemon subscription and writes resume every 5 s and on pause, seek, end and stop. Only the library is the persistent source of truth; mpv watch-later never becomes a second resume store for Arinn-managed sessions.

Resume > 0 shows as "Continue". The design's 90-second rule applies to regular films and episodes longer than 180 s; for short or unknown durations completion requires a confirmed EOF. A manual seek to the end never marks watched without playback/EOF confirmation. The next episode is set to position 0 only when the season/episode order is unambiguous. Auto-next is a separate setting. A missing file or mount never erases resume.

Chapters and buffered ranges are shown only when data exists. Seek thumbnails come from a separate asynchronous, rate-limited, cancellable extractor and cache in the provider (for example ffmpeg); it never seeks the main mpv to get an image. The timeline and time stay usable until a thumbnail is ready. Skip intro is available only with real chapter or segment metadata; there is no own intro detector.

## Verification

AC-15, AC-16: local file, mounted network file, missing file, mount loss and recovery, two audio tracks, external and embedded subtitles, pause/seek/restart resume, end of film and episode advance. A provider crash does not close the shell; the player runs in its own application scope and reconnecting to an Arinn-owned player never kills other mpv instances.

Codecs, hardware decoding, HDR and passthrough are not guaranteed by documentation; they are checked on the target HTPC. Library/parser and IPC contract tests run on the development machine.
