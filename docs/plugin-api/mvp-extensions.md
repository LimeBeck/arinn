# API review: media and downloads for the extended MVP

Date: 2026-09-27. Accepted at the level of model and semantics by [ADR-0024](../adr/0024-local-library-mpv.md) and [ADR-0025](../adr/0025-downloads-mvp.md). This extends specification v0.3; DownloadProvider was not part of it. The API is not yet published as stable v1; before a provider is implemented its schema is fixed in versioned D-Bus XML and contract fixtures.

## General rules

Transport and operation model: [implementation baseline](../architecture/implementation-baseline.md). The notation below is specification language, not ready Rust or QML signatures. All mutating methods take a `request_id` and the expected generation and return an Operation; IDs are opaque. Optional values in the wire schema have an explicit has-value flag, not magic empty dictionaries. Snapshot lists are paginated and bound to generation/revision.

Extension interfaces are discoverable through introspection and registered feature flags; core routing never depends on the product. An unknown feature never makes the base capability incompatible. An unknown top-level capability in an older daemon causes `Unsupported` at registration, so DownloadProvider ships with a daemon that supports it.

## Rich media without obligations for MPRIS

| Interface | Typed data | Methods / semantics |
| --- | --- | --- |
| `MediaOpen1` | MediaSource: content EntityRef, URI, start_position_us, preferred audio/subtitle language; the URI is visible only to the provider and never logged | `Open(source)`: creates a managed session, result is a session EntityRef. The source must be resolved by a library provider; the UI never passes a raw arbitrary command or URL. |
| `MediaTracks1` | Track: id, kind audio/subtitle, language, label, codec, channels, selected, external | `ListTracks(session)`; `SelectTrack(session, kind, track_id)`; `DisableSubtitles(session)`. Selecting audio needs a real ID; subtitles off is a separate action. |
| `MediaTimeline1` | Chapter: id, start_us, title; BufferedRange: start_us/end_us; Thumbnail: asset_ref, timestamp_us | `GetTimeline(session)`; `RequestThumbnail(session, position_us)`. An unsupported thumbnail returns `Unsupported`; base seek works independently. |
| `LibraryResume1` | Resume: content_id, position_us, duration_us, completed, observed_at | Resumable entries; `SetProgress` from the linked media session through the daemon; `RestartFromBeginning` on explicit action. Watched state is never a shell-local boolean. |
| `LibraryImport1` | ImportCandidate: source EntityRef, mapped local file reference, completion identity | `ImportCompleted`: asynchronous reconciliation of completed files in configured roots; result is Library EntityRefs. Repeating is safe and never duplicates entries. |

These interfaces apply to mpv, other players and other library backends; they contain no mpv property names. Raw player IPC stays in the plugin.

## DownloadProvider v1

A new sixth base capability, `download`. It describes a download job, not a torrent protocol. The interface `org.arinn.DownloadProvider1` offers `GetSnapshot`, `GetJob`, `Pause`, `Resume`, `StartNow` and `Retry`; there are no methods for deleting files or for arbitrary Transmission RPC.

| DownloadJob field | Type and semantics |
| --- | --- |
| id | Opaque string, provider namespace |
| title | UTF-8 display title |
| state | queued, downloading, paused, verifying, completed, error |
| completed_bytes / total_bytes | uint64; total has a has-value flag while unknown |
| rate_bytes_per_second | uint64 |
| eta_seconds | int64; −1 unknown |
| progress | double 0..1 with has-value; the UI never shows a fake percentage for an unknown size |
| available_actions | enum set: pause, resume, start_now, retry |
| error_code / error_message | normalized code and safe text |
| outputs | list of typed completed output refs, never raw arbitrary paths in QML |
| library_refs / import_state | linked Library EntityRefs; pending, imported, unavailable or unsupported, independent of the download state |

Pause stops the transfer; Resume continues and keeps the queue policy; StartNow explicitly bypasses the queue for the selected job. Retry never creates a new job: it repeats an allowed recovery of the existing one; an unsupported operation is `Unsupported`. Filesystem recheck/verify is a provider-specific part of Retry, not a guaranteed full re-download.

Completed means the data is downloaded and verified by the backend; ongoing seeding does not make the file incomplete. "Added to library" is shown only after `LibraryImport` succeeds. A non-media download shows "Done" without a "Watch" button. For media the button becomes active after a Library EntityRef exists and opens Detail.

The UI follows the design handoff: pause/resume/start now/retry, progress and status, service unavailable, completion toast. The extra backend state verifying is shown as "Checking…" without an invented ETA. Peers, seeds, ratio and trackers are not part of the general API or the default UI.

The MVP manages jobs already added to Transmission. A form for adding magnets or .torrent files is not in the handoff and is not required for a usable integration: jobs can be added with the regular Transmission UI. A future `AddSource` needs a typed source, validation and its own UI, never arbitrary RPC through ActionProvider.

## Generic completion → library

A provider publishes the transition to completed with a stable completion identity. The daemon routes the event to the selected `LibraryImport1` provider. The download provider knows backend paths; the library knows configured roots; the mapping of a remote path to a local mount is explicit configuration. Without a mapping, import is unavailable; arbitrary paths are never scanned.

The library provider reconciles files, creates or updates its entries and returns refs. The completion ledger is stored in the library provider, keyed by `(download provider, stable job ID, output identity)`; reconnect or replay never creates duplicate New entries or toasts. Browser downloads or other download managers can use the same flow.

## Rationale and scope of the review

A generic ActionProvider cannot express a structured list of jobs with revisions, states and available actions; a product-specific Transmission API in the core would violate R-01. Hence a separate DownloadProvider. `LibraryImport1` separates download completion from content indexing and works with any download backend.

The chosen types, state machine and ownership close the architectural question of the extension. Media and library D-Bus XML, protocol types, the SDK and the provider contract tests are [ADR-0052](../adr/0052-media-and-library-wire-contract.md). DownloadProvider XML, serializers and its contract suite are still implementation deliverables. Transport limits and errors come from ADR-0020.
