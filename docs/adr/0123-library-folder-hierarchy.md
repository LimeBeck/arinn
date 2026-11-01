# ADR-0123: Optional paged library folder hierarchy

- **Status:** Accepted
- **Date:** 2026-11-01
- **Deciders:** delegated technical choice under ADR-0037; owner requested offline folders and seasons on 2026-11-01, task #626 / parent #242
- **Amends / Supersedes:** extends ADR-0054
- **Amended by / Superseded by:** —
- **Specification:** §5.3, §7.4, §11, §13; AC-02, AC-12, AC-15

## Context

Library items describe content identity and series/episode parents, not filesystem
folders. Replacing that parent relation would break series navigation and resume.
The owner selected offline folders and seasons before online enrichment. This
prerequisite adds no provider scan or UI implementation.

## Decision

Add optional `LibraryHierarchy1` in provider Features, served by
`org.arinn.LibraryHierarchy1.GetHierarchy(generation, revision, token)` and the
same method on the daemon's `Library1`. The response is
`(generation, revision, folders[], members[], next_token)`, signature
`(sta(sssb)a(ss)s)`. A folder is `(id, parent_id, title, available)`, a membership
is `(folder_id, item_id)`. IDs are opaque and scoped to the provider. Display
titles are labels, never identity or filesystem paths. Roots have empty parents;
all other folders reference existing folders. A maximum of 64 ancestor edges
bounds validation. Cycles, duplicate IDs, orphan folders, duplicate membership
pairs and references to unknown items are invalid. Items may appear in multiple
folders; they keep their original catalogue, progress and launch identity.
Unavailable folders/items remain representable. Empty folders are permitted.

The hierarchy belongs to the base catalogue generation/revision. SDK
`LibraryExtensions::hierarchy()` and `replace_with_hierarchy` publish the
catalogue, additions and hierarchy atomically. A hierarchy-only change advances
the catalogue revision. Legacy replacement clears hierarchy. Registration resets
paging state. Only the registered daemon owner reads the provider interface.

Use the existing SDK snapshot store: at most 200 entries and 256 KiB per page,
16 MiB and 100000 entries per assembled snapshot, 30-second caller-bound tokens,
at most 32 paging leases. Every request specifies generation/revision; a stale
request or a token for a different revision returns Conflict. No new path,
file URI, executable or remote endpoint is exposed.

The daemon imports under a separate 500 ms deadline. It validates page bounds,
consistent generation/revision, token progress, the complete tree and item
references. Invalid, missing, unsupported or timed-out hierarchy leaves the
base catalogue usable. GetHierarchy returns Unsupported without the feature,
Conflict for a stale catalogue pair, Unavailable without validated hierarchy.
Full reimports cannot change accepted snapshots at the same revision. When a
validated hierarchy remains cached at that pair, polling reads only page 1;
unchanged revisions keep the immutable cached content. Providers must advance
revision for any metadata change. Provider removal
clears cached hierarchy and leases. Catalogue StateChanged covers hierarchy
availability changes too.

## Consequences

Existing Item, LibraryPage and provider signatures remain compatible. The
implementation task can build folder navigation without reading local files in
the shell or changing series parents. Optional hierarchy failure must preserve
the flat catalogue and content actions. Seasons continue using existing item
metadata; online sources are outside this increment.

## Alternatives considered

- Reparenting items to folders would overwrite series membership.
- Passing raw paths to QML would move provider responsibilities into the shell.
- A single unpaged response would impose an artificial small-library limit.

## Verification

Protocol validation/round trips; SDK pagination, caller isolation, atomic
publication and revision fencing; daemon optional/invalid/stale metadata
recovery and live introspection. Check results and independent review belong
on the PR; this ADR does not claim tests have passed.
