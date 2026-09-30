# ADR-0056: Local library scan, metadata and resume

- **Status:** Accepted
- **Date:** 2026-09-30
- **Deciders:** delegated technical choice ([ADR-0037](0037-agent-autonomy-and-owner-gated-decisions.md)) within [ADR-0024](0024-local-library-mpv.md) and [ADR-0054](0054-media-and-library-wire-contract.md)
- **Amends / Supersedes:** —
- **Amended by / Superseded by:** —
- **Specification:** [§7.4](../specifications/arinn-specification.md#74-local-library-and-mpv), [§11](../specifications/arinn-specification.md#11-state-ownership-and-persistence), [§13](../specifications/arinn-specification.md#13-security)

## Context

`arinn-plugin-library` can be implemented on the library wire contract without a new D-Bus method. The contract does not say how file names are parsed, where roots are configured, how a scan stays off the snapshot path, or how `SetProgress` tells a seek from playback. Those choices are local to the provider ([#124](https://github.com/LimeBeck/arinn/issues/124)).

## Decision

Arinn uses the following rules in `arinn-plugin-library`.

**Configuration.** Roots are absolute directories in the provider settings, read once at start from `ARINN_PROVIDER_SETTINGS` (compact JSON of `[providers.<id>.settings]` in the daemon's `config.toml`, or `{}` when that table is unset; [ADR-0064](0064-provider-settings-in-config.md)). Example: `{"roots":["/srv/films"]}`. There is no separate `library.toml`. Optional `autoscan` and `probe` default to true. `ffprobe` defaults to `ffprobe` on `PATH`, or an absolute path. `/` is rejected. An unset variable is the same as `{}` (no roots). A change applies at the next provider start. `SIGHUP` rescans the roots from that start and does not reread settings.

**Database.** Catalogue, resume and import completion ids live in `$XDG_STATE_HOME/arinn/providers/<digest>/library.sqlite3` (mode 0600). Three collections, Films, Series and Other, are created once and keep their ids.

**Scan.** A background task publishes the last snapshot immediately and replaces it when a scan finishes. Triggers are startup, `SIGHUP`, an inotify hint debounced by 2 seconds, and every 10 minutes while `autoscan` is on. The catalogue lock is not held across the walk or ffprobe, and `Launch`, `SetProgress`, `RestartFromBeginning` and `Import` do not take that lock on the async runtime. Hints are best-effort: at most 4096 directory watches, directory symlinks are not watched or walked, and a directory created after the watch is added on the next hint. One unreadable subdirectory, or a file that disappears before it is stat'd, is skipped; the rest of the scan is published and the provider keeps running. A file symlink is catalogued only when its target is a regular file whose canonical path stays inside the root. Dot names are skipped. The walk stops at 64 levels or 50 000 videos; a truncated walk does not mark unseen rows missing. ffprobe runs at most two at a time, 5 seconds each, and only when the file is new or its inode, size and mtime changed.

**Identity.** The item id is a UUID. A path match wins. Otherwise the same inode, size and mtime inside that available root is a rename and keeps the UUID. One inode is one item. A missing file or an unreadable root sets `available` false and does not delete the row or its resume. Removing a root from the configuration does the same.

**Names.** Video extensions are mkv, mp4, m4v, avi, mov, wmv, ts, m2ts, webm, mpg and mpeg. `.` and `_` become spaces, and `[...]` groups are dropped. A year is `(YYYY)` or a trailing `YYYY` in 1888..=2099. `SxxEyy` and `NxNN` mark an episode; a parent directory `Season N` supplies the season only when the name has none. The series title comes from the directory above `Season N`, otherwise from the text before the marker. The episode title is the text after the marker, otherwise the cleaned file name. Anything else is a film. The Other collection stays empty. Nothing is invented when a field is absent.

**NFO.** `<stem>.nfo` beside the video, and `tvshow.nfo` in the series directory, may set title, year, season and episode. A document that contains `<!DOCTYPE` or `<!ENTITY`, is not UTF-8, or is over 1 MiB is ignored. Only `&amp; &lt; &gt; &quot; &apos;` are decoded. No network and no external entity. A sidecar symlink is read only when its canonical path stays inside the root.

**Duration and languages.** Duration is ffprobe's `format.duration` in microseconds, or −1. It is not taken from the NFO. The first audio and subtitle stream languages are kept only when they are language tags. Launch returns them, or empty strings. ffprobe is optional: a missing binary, a non-zero exit or a timeout leaves duration −1 and empty languages, and the scan still publishes the item. The Arch package lists `ffmpeg` as an optional dependency.

**Artwork.** Poster candidates, in order, are `{stem}-poster.jpg/png`, `poster.jpg/png`, `folder.jpg` and `cover.jpg`. Fanart candidates are `{stem}-fanart.jpg/png`, `fanart.jpg/png`, `backdrop.jpg/png`. The asset id is a UUID stored in the database while a regular file is present inside the root, and empty when it is not. A symlink whose target leaves the root is not artwork. Bytes are not on the wire.

**Resume.** `SetProgress` stores the sample. Completion is `ended`, or, for a duration over 180 seconds, a forward step of at most 15 seconds that lands inside the final 90 seconds from a position already within one such step of that window. A larger jump is a seek and does not complete. A repeated position does not complete. Shorter and unknown durations complete only on `ended`. `RestartFromBeginning` sets position 0 and clears completion. Launch of a completed item starts at 0; otherwise it starts at the stored position when that position is greater than 0. The next episode is not written.

**Import.** The file URI must fall inside a configured root that can be read. The same completion id returns the same item id and does not insert another row. A different completion id for a path that is already catalogued returns that item. A batch that would return the same item id twice is rejected and writes nothing.

**Privacy.** Logs are a JSON error line with the component, operation and error code. Paths, titles and URIs are not formatted into it.

## Consequences

The scanner, mpv and the daemon's library routing stay separate tasks. A seek into the credits is not treated as watched until playback moves again inside the window or EOF is confirmed. Empty artwork ids are the fallback the shell already treats as generic art.

## Alternatives considered

A separate `providers/<digest>/library.toml` was the first local choice and is not used. The owner decided on 2026-09-30 that roots live in `config.toml` and reach the provider as `ARINN_PROVIDER_SETTINGS` ([ADR-0064](0064-provider-settings-in-config.md)). Treating every position inside the final 90 seconds as watched was rejected because `SetProgress` is also called on seek. Resolving NFO external entities was rejected because a sidecar must not read or fetch arbitrary targets.

## Verification

Provider tests on temporary folders cover film, series and episode names, a hostile NFO, a missing file, an unavailable root and its recovery, a rename that keeps the UUID, an in-root symlink that stays one item, the resume rules, import idempotency, rejection of an import after its root leaves the configuration, and a scan when ffprobe is missing. A private-bus test drives `LibraryProvider1`, `LibraryResume1` and `LibraryImport1` through the SDK and checks that the title and root path are absent from stderr. No network share and no operator test.
