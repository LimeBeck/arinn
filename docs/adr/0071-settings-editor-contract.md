# ADR-0071: Settings editor contract (integer unit, folder listing, library root availability)

<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

- **Status:** Accepted
- **Date:** 2026-10-03
- **Deciders:** project owner for the behaviour (spec §5.6, owner decisions of 2026-09-30, [#172](https://github.com/LimeBeck/arinn/issues/172)); the wire design is a delegated technical choice ([ADR-0037](0037-agent-autonomy-and-owner-gated-decisions.md)), task [#192](https://github.com/LimeBeck/arinn/issues/192)
- **Amends / Supersedes:** amends [ADR-0065](0065-provider-settings-contract.md) (integer constraints)
- **Amended by / Superseded by:** —
- **Specification:** [§5.6](../specifications/arinn-specification.md#56-settings), R-04, R-05, [§13](../specifications/arinn-specification.md#13-security)

## Context

The settings editors handoff (#172, DR-25 to DR-27) and the owner decisions of 2026-09-30 in spec §5.6 need three things that the contract of [ADR-0065](0065-provider-settings-contract.md) does not carry:

1. An integer may show a short unit next to its value.
2. A `path` setting with `directory` is chosen in a folder picker that shows folders only. Mounted folders are start points, and network shares are never browsed. The shell must not read the file system itself (R-05).
3. A library folder is shown as unavailable only when the library provider reports it.

## Decision

**Integer unit.** Integer `Constraints` gain an optional `unit`: display text of 1 to 16 bytes, with no control characters and no surrounding spaces (`settings::unit`, `MAX_UNIT_BYTES`). It is rejected on every other kind. A unit never changes a value, which stays a plain JSON integer.

**Folder listing: `org.arinn.Folders1`** on `/org/arinn/Daemon1`, a new daemon interface ([folders1.xml](../plugin-api/folders1.xml), `arinn_protocol::folders`). `Settings1` is unchanged.
- `Places() → a(ss)` returns `(kind, path)` start points:
  - `home`, the user's home folder;
  - each mounted folder under `/media`, `/run/media` and `/mnt` as `mount`, in picker order;
  - `root`, which is `/`.
- `List(path, after, limit) → (as, b)` returns the names of the subfolders of an absolute normal path, in picker order, after the cursor `after` (empty for the first page), at most `limit` (1–256), and whether more follow.
  - An entry is listed when it resolves to a directory, so symbolic links to folders are included.
  - Hidden names (leading `.`) and names that cannot be shown or passed back (control characters, `/`, more than 255 bytes) are skipped.
  - Picker order is case-insensitive, then exact text. That makes the order total, so a cursor that no longer exists still continues correctly.
  - No file names, sizes or contents are returned.
- Errors:
  - `InvalidArgument` for a bad path, cursor or limit;
  - `NotFound` when the path is missing, is not a folder or its mount has gone;
  - `PermissionDenied` when it cannot be read.
- The daemon never logs a path (spec §13).

**Library root availability: `org.arinn.LibraryRoots1`**, an optional provider interface listed as `LibraryRoots1` in `Features` ([library-roots1.xml](../plugin-api/library-roots1.xml)).
- `GetRoots(generation) → (generation, revision, a(path, available))` returns every configured root exactly as written in the provider's `path` setting, so the shell can match it to the value it shows. There are at most 64 roots, unique and absolute.
- The method answers while the provider's backend is `unavailable`, because a missing drive is exactly what it reports.
- The provider emits `StateChanged(generation, revision)` when any root changes. Publishing the same list is not a change.
- The SDK adds `LibraryExtensions::roots()` and `LibraryService::replace_roots`.

**Daemon follow-ups.** The daemon parts are not in this change, because the private-bus suite requires an export and its XML to land together:
- the `Folders1` implementation;
- the shell-facing roots method, `Library1.GetRoots() → (generation, revision, a(sb))`, which returns `Unsupported` unless `Features` lists `LibraryRoots1`, with the daemon emitting its existing `StateChanged` for the `library` capability when a provider's roots change.

That follow-up adds `GetRoots` to [library1.xml](../plugin-api/library1.xml) together with its implementation, so it is a `contract-change` task too.

## Consequences

- The shell can render the integer unit as soon as a provider declares it. Old providers are unaffected, because the field is optional.
- The folder picker needs only daemon calls. File system policy (hidden entries, mounts, errors, privacy) lives in one place.
- Library root availability stays provider-owned, as the owner decided. A provider without `LibraryRoots1` never shows a folder as unavailable.
- Until the daemon follow-up lands, the shell cannot call `Folders1` or `Library1.GetRoots`. The settings screen keeps its current path editing and no unavailable state.

## Alternatives considered

- **Folder listing as a `Settings1` method.** Rejected: it is not about settings values, and other pickers (a download location) can reuse a separate interface.
- **Offset paging.** Rejected: offsets skip or repeat names when a folder changes between pages. A name cursor in a total order does not.
- **Availability inside the library page (`Collection`/`Item`).** Rejected: roots are configuration, not catalogue entries, and a root can be unavailable while it has no items.
- **The daemon checking root availability itself.** Rejected by the owner decision of 2026-09-30: the library provider reports it.

## Verification

- `arinn-protocol`:
  - unit tests for `unit` on every kind;
  - `folders` places, names, order and paging;
  - roots validation and wire signatures;
  - `tests/folders.rs` compares `folders1.xml` with a live export and round-trips the typed proxy and its errors on a private bus.
- `arinn-plugin-sdk` `tests/library_service.rs`:
  - the `LibraryRoots1` feature, `GetRoots`, a stale generation, `StateChanged` with a new revision, no change for the same list and invalid roots;
  - comparison of `library-roots1.xml` with live introspection;
  - `Unsupported` without the extension.
- The daemon follow-up adds its own private-bus export comparison.
