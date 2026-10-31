# ADR-0122: Revision-aware provider polling

- **Status:** Accepted
- **Date:** 2026-10-31
- **Deciders:** Codex, delegated technical decision under ADR-0037; owner-directed [#605](https://github.com/LimeBeck/arinn/issues/605)
- **Amends / Supersedes:** amends [ADR-0058](0058-media-routing.md) (snapshot refresh)
- **Amended by / Superseded by:** —
- **Specification:** §6.5, §9.7, §11; G-5.4

## Context

Provider revisions identify immutable snapshots within one generation. Reading
all pages on every one-second fallback tick repeats IPC, validation, cloning and
cache construction even when nothing changed. Restart maintenance also reopens
SQLite and updates an unchanged counter on every tick.

## Decision

The daemon reads and validates the first page before deciding whether to fetch
the remaining pages. Only a revision already imported successfully by that
watcher, for its fixed owner and generation, permits an unchanged result. A new
watcher always reads a complete snapshot. Changed revisions retain the existing
paging, generation, size, duplicate and cache-import checks; failed reads do not
advance the remembered revision.

An unchanged result carries no replacement entry lists. Backend state and
optional feature flags are still read, independently of catalogue revisions.
Library resume revisions are tracked separately; roots and additions retain
their existing refresh and validation rules. Download import retries and media
progress forwarding continue when catalogue revisions are unchanged.

Restart maintenance reuses a lazily opened SQLite connection on blocking workers.
An unchanged supervisor counter performs no UPDATE. A counter decrease is still
persisted, and later increases retain the existing cumulative-count semantics.
The SQLite journal mode is unchanged; removing redundant transactions does not
require WAL or a storage-policy change.

## Consequences

An unchanged paged catalogue needs only its first page, rather than all pages.
The daemon trusts the existing immutable-revision contract for omitted pages.
First-page paging leases still expire under the existing SDK limits; signal
coalescing must prevent redundant same-revision signals from exhausting them.
No wire interface, product behaviour or target-device qualification changes.

## Alternatives considered

- Read every page to compare identical snapshots: retains the avoidable work.
- Drop the fallback timer and rely only on signals: changes recovery behaviour.
- Enable WAL as the first fix: does not remove unnecessary writes or snapshot IPC.

## Verification

Private-bus regressions cover one-page unchanged probes for applications,
browser, media, library, resumes and downloads, alongside changed/stale snapshot
coverage. Storage regressions count SQLite changes for repeated observations and
check counter reset semantics. Full required checks are recorded in the task PR.
G-5.4 target-device measurement remains pending.
