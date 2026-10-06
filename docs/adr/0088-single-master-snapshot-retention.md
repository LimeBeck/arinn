# ADR-0088: Retain one verified master snapshot release

- **Status:** Proposed
- **Date:** 2026-10-06
- **Deciders:** owner requested one master pre-release; process acceptance and merge remain owner-gated
- **Amends / Supersedes:** ADR-0086 (snapshot publication and retention)
- **Amended by / Superseded by:** —
- **Specification:** §17.1, §18

## Context

The owner requested retaining only one master pre-release and permits deleting
older ones. Independent master builds can finish out of order; choosing by upload
time can replace a newer tested commit with an older one. Interrupted cleanup
must leave a verified release available, and legacy publishers may still run.

## Decision

Keep one published automatic `snapshot-<full SHA>` pre-release: the newest
successfully verified commit on master's ancestry. Preserve ordinary version
releases, drafts belonging to other runs, stable releases and off-master tags.
Delete obsolete GitHub release objects/assets only; never move or delete Git tags.
Published retained snapshots remain immutable and never become the latest stable
release. Full CI/package gates and per-master-commit build groups remain unchanged.
A superseded publication/rerun may skip upload rather than recreate a retired
snapshot while a newer verified snapshot exists.

Serialize snapshot publication and retention under one non-cancelling job lock
with `queue: max`, so housekeeping cannot replace a pending newer publication.
The platform queue holds up to 100 waiting jobs; excess jobs may be cancelled.
Selection still uses ancestry rather than trusting queue arrival order.
Recheck ancestry before un-drafting so a newer release appearing after prepare
wins. Verify the keeper's tag, master lineage, asset upload state, size/digest
metadata and downloaded SHA256SUMS content before deleting older fallbacks.
Enumerate all pages before deletion; idempotent cleanup tolerates already-missing
objects and retries after partial failure. Ambiguous history or inconsistent
metadata fails closed without selecting a release by upload time.

A release-published cleanup workflow executes trusted master code, shares the
same lock, and covers older/in-flight publishers which lack inline retention.
It is also manually dispatchable only on master. It reads no PR/release artifacts
as code and uses the existing SNAPSHOT_RELEASE_TOKEN; no new secret/settings are
introduced. Checksum downloads strip authorization on redirects and require HTTPS.
Agents prepare this change; only the owner merges the release workflow/policy.

## Consequences

The master channel exposes one usable snapshot after cleanup, reducing asset
storage. A newly verified release is published before the old one is deleted,
so there may be a short overlap rather than downtime. Version-tag pre-releases
are a separate channel. SHA source tags remain available after old binary assets
are retired. Cleanup failure keeps the newest verified snapshot and reports an
error; publication must not delete the last good fallback before verification.

## Alternatives considered

Deleting the old release before validating the new one risks an empty channel.
Choosing by publication date allows out-of-order CI rollback. Reusing or moving a
single mutable snapshot tag would discard the established exact-commit identity.
Cancelling full master CI was not selected; only obsolete publication is skipped.

## Verification

Unit tests cover ancestry ordering, immutable reruns, stale prepare/publish races,
protected release classes, pagination, bad keeper/digests/checksum content, API204,
redirect token stripping and event/branch fences. Current old snapshot objects
may be deleted under the explicit request after a verified preview. Future
workflow enforcement requires owner merge and a successful real publish/cleanup
cycle; local tests do not claim that qualification.

Sources: [GitHub concurrency and queuing](https://docs.github.com/en/actions/how-tos/write-workflows/choose-when-workflows-run/control-workflow-concurrency),
[release API](https://docs.github.com/en/rest/releases/releases).
