# ADR-0054: Media and library wire contract

- **Status:** Accepted
- **Date:** 2026-09-29
- **Deciders:** delegated technical choice ([ADR-0037](0037-agent-autonomy-and-owner-gated-decisions.md)) within [ADR-0013](0013-mpris-media.md), [ADR-0024](0024-local-library-mpv.md) and specification §7.3, §7.4 and §9.1
- **Amends / Supersedes:** amends [ADR-0013](0013-mpris-media.md), [ADR-0024](0024-local-library-mpv.md)
- **Amended by / Superseded by:** [ADR-0062](0062-asset-contract.md) — fetching assets by id; [ADR-0063](0063-ended-session-state.md) — the `ended` session state; [ADR-0071](0071-settings-editor-contract.md) — the optional `LibraryRoots1` extension
- **Specification:** [§7.3](../specifications/arinn-specification.md#73-media-sessions-mpris), [§7.4](../specifications/arinn-specification.md#74-local-library-and-mpv), [§9.1](../specifications/arinn-specification.md#91-capabilities), [§9.7](../specifications/arinn-specification.md#97-limits-and-validation)

## Context

Phase 4 needs a wire contract before an MPRIS provider, a library scanner or mpv. The application and browser contracts already fix the patterns: paged snapshots, daemon-owned operations, and one daemon endpoint per provider. Rich playback is specified as optional feature flags so MPRIS does not have to implement tracks or thumbnails ([API review](../plugin-api/mvp-extensions.md), [media MVP](../architecture/media-mvp.md)).

## Decision

**Media base** (`arinn-protocol::media`):

- `Session = (id, title, subtitle, artwork, state, duration_us, position_us, anchor_us, actions, has_content, content)`, signature `(sssssxxtasb(sss))`.
- `artwork` is an opaque asset id or empty, never inline bytes. `actions` is a set of short tokens. Known tokens are `play`, `pause`, `play-pause`, `seek`, `next`, `previous` and `stop`. An unknown token is ignored by consumers.
- `state` is a short token. Only `playing` advances a position on the shell clock. `paused`, `stopped` and any other token do not.
- `duration_us` is −1 when unknown. `position_us` is non-negative and not past a known duration. `anchor_us` is `CLOCK_MONOTONIC` in microseconds when `position_us` was sampled. The provider publishes a new anchor on state change or seek and at most once per second while playing.
- `content` is an `EntityRef` when `has_content` is true (a library item for Arinn-managed playback) and empty otherwise.
- `SessionPage = (generation, revision, sessions, next_token)` uses the same paging limits as application pages.

**Provider** `org.arinn.MediaProvider1` at `/org/arinn/Provider1`, daemon-only:

- `GetSessions`, `GetSession`, `BackendState`, `Features`, `Play`, `Pause`, `PlayPause`, `Seek`, `Next`, `Previous`, `Stop`, and `StateChanged(generation, revision)`.
- A command whose token is absent from `actions` returns `Unsupported`. It is never a success with no effect.
- `Features` returns the rich flags below, in that order, or an empty list.

**Daemon** `org.arinn.Media1` on the provider path, same reads, and commands that take `request_id` and return `Operation1`. `ListTracks`, `SelectTrack`, `DisableSubtitles`, `GetTimeline` and `RequestThumbnail` are on this interface too. The daemon returns `Unsupported` unless `Features` lists the matching flag. The shell does not send a URI.

**Optional media interfaces**, exported only by providers that implement them:

| Flag | Interface | Behaviour |
| --- | --- | --- |
| `MediaOpen1` | `Open(generation, source) → session id` | `source` is `(content EntityRef, file URI, start_position_us, audio language, subtitle language)`. The URI is `file:///` only, visible to the daemon and the provider, and never logged. |
| `MediaTracks1` | `ListTracks`, `SelectTrack`, `DisableSubtitles` | Track kind is `audio` or `subtitle`. At most one selected track of each kind. Selecting a track needs a real id. Subtitles off is `DisableSubtitles`. |
| `MediaTimeline1` | `GetTimeline`, `RequestThumbnail` | Chapters and buffered ranges. A thumbnail the provider cannot make returns `Unsupported`. Base seek does not depend on this interface. |

Track, timeline and thumbnail changes that are not in the session struct call the SDK `touch`, which publishes a new session revision. There is no separate tracks signal.

**Library base** (`arinn-protocol::library`):

- `Collection = (id, title, kind)` and `Item = (id, collection_id, parent_id, title, kind, has_year, year, has_season, season, has_episode, episode, duration_us, poster, fanart, available)`.
- Known collection kinds are `film`, `series` and `other`. Known item kinds add `episode`. Unknown tokens display as `other`.
- Year, season and episode use a has-value flag. The number is 0 when the flag is false.
- `parent_id` is empty or another item in the same snapshot. Parents are acyclic. An episode points at its series.
- `poster` and `fanart` are asset ids or empty. `available` false is a missing file or an unavailable mount. The entry stays in the catalogue.
- `LibraryPage` pages collections and items together, with the application page limits.

**Provider** `org.arinn.LibraryProvider1`: `GetLibrary`, `GetItem`, `BackendState`, `Features`, `Launch`, `StateChanged`. `Launch` returns `(local_id, file URI, start_position_us, audio language, subtitle language)` for an available item. A missing file returns `Unavailable`. The daemon turns that source into `MediaOpen1.Open` on the selected media provider and completes the shell's `Library1.Launch` operation with the session `EntityRef`. The shell never receives the URI.

**Daemon** `org.arinn.Library1`: `GetLibrary`, `GetItem`, `Features`, `Launch(generation, id, request_id) → Operation1`, plus `GetResumes` and `RestartFromBeginning`, which return `Unsupported` without `LibraryResume1`.

**Optional library interfaces:**

| Flag | Interface | Behaviour |
| --- | --- | --- |
| `LibraryResume1` | `GetResumes`, `SetProgress`, `RestartFromBeginning`, `StateChanged` | Resume is `(content_id, position_us, duration_us, completed, observed_at_us)`. `observed_at_us` is Unix time in microseconds. `SetProgress` is daemon-only: the daemon forwards the linked session's position, duration and a confirmed-EOF flag. The provider applies the 90-second / EOF rule and is the only persistent store. `RestartFromBeginning` is the explicit user action. Resume revisions do not bump the catalogue revision. |
| `LibraryImport1` | `ImportCompleted(generation, candidates) → local ids` | A candidate is `(source EntityRef, file URI, completion id)`, at most 32. Repeating a completion id must not create a second item. The shell has no import method. |

An MPRIS provider exports only `MediaProvider1` and returns no feature flags. A missing flag hides or disables the control. It does not make the base capability incompatible.

The SDK serves the provider side (`MediaService`, `LibraryService`, `run_media`, `run_library`). The production daemon does not export `Media1` or `Library1` yet and still treats `media` and `library` as unrouted capabilities.

## Consequences

- MPRIS, mpv and the folder scanner can be separate tasks on this contract. They do not change the shell or the core to add a product name.
- File URIs cross the daemon boundary twice (library → daemon → media) and are never logged.
- "Remove from library" is not a new method. A hidden entry disappears from the snapshot the provider publishes. A missing file stays, with `available` false.

## Alternatives considered

- **Rich fields on the base session:** rejected. MPRIS would have to invent tracks and thumbnails or return empty success, which the UI must not treat as a no-op.
- **A shell `Open` that takes a URI:** rejected. The UI never passes a raw path. Library launch is the only shell entry.
- **One revision for catalogue and resume:** rejected. Resume is written every few seconds and must not force a full catalogue read.

## Verification

- `arinn-protocol` unit tests: signatures, round trip, file-URI limits, session and library consistency.
- `arinn-plugin-sdk` store tests: session and library paging.
- Private-bus tests: registration as `media` or `library`; command checks (`Unsupported`, `NotFound`, `Conflict`, `InvalidArgument`, `Unavailable`, `PermissionDenied`); a base media export has no rich interface; enabling the flags exports `MediaOpen1`, `MediaTracks1` and `MediaTimeline1`, and the library flags export `LibraryResume1` and `LibraryImport1`; live introspection matches the provider XML.
- Daemon `Media1` / `Library1` exports are not implemented in this change, so they are not compared to a live daemon.
