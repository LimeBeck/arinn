# ADR-0124: Offline folder and season navigation

- **Status:** Proposed
- **Date:** 2026-11-01
- **Deciders:** owner approved the mockup and clarified Catalogue/Folders modes in chat; implementation review/merge pending, task #629 / parent #242
- **Amends / Supersedes:** extends ADR-0056 and ADR-0123
- **Amended by / Superseded by:** —
- **Specification:** §5.3, §7.4, §11, §13; AC-02, AC-12, AC-15

## Context

The owner selected offline folder/season navigation first, with existing local
metadata and artwork. A physical folder named Series can contain the same media
as the logical Series category, so a third category tab is confusing. The owner
agreed to continue after the Catalogue/Folders mode distinction was explained.
The optional hierarchy prerequisite is merged in #627.

On 2026-11-01 the owner requested that visual changes be developed first in the
original HTML/JS design prototype. The editable
[working derivative](../design/prototypes/library-navigation/README.md) keeps
the historical handoff unchanged. Further shell presentation changes follow
review of that prototype. After viewing it, the owner requested continuation;
the shell presentation follows the derivative in the same pull request.

## Decision proposed for owner merge

Use a Catalogue/Folders view switch. Catalogue keeps the existing collection
categories (Films, Series and Other); Folders shows physical configured roots,
subdirectories and existing file items. The shell never infers filesystem
structure from a title or sees a raw file URI. A file opens its existing detail
and launch action; logical series/season navigation remains in Catalogue.

Provider folder UUIDs persist by (root identity, relative directory path) in
SQLite. Equal folder names in different roots remain distinct. Directory renames
create a new folder identity; existing content identity/resume still follow the
provider's existing rename rules. Scan records readable directories, including
empty ones, within the existing depth/file safeguards and a directory bound.
Unavailable directories/roots and their known content remain in the tree. On a
legacy database first reopened offline, stored item paths reconstruct ancestry
without consulting the unavailable filesystem. Import adds required ancestry.
Folder topology and catalogue publish atomically using ADR-0123.

The shell reads optional hierarchy under its own bounded deadline and validates
paging, generation/revision, parents and memberships. Unsupported/invalid data
keeps the catalogue usable. Folder presentation IDs use a namespace disjoint
from content UUIDs. The view switch is absent when no provider supplies valid
hierarchy. Changing modes remembers the folder location and the last still-valid
catalogue category. It does not duplicate content/progress records.

Opening a folder remembers the opener and scroll position. From a focused
season/episode rail, Back first returns to the detail's primary action, matching
the HTML prototype. Another Back leaves detail, then one folder level per press,
restoring that level's opener; at the
folder root it follows the existing Library Back behavior. A visible Back button
supports pointer input. Category and mode controls remain reachable with the
D-pad, and data refresh under another overlay never takes focus.

Focusing the season/episode rail raises it to keep its captions on-screen.
Opening a physical episode selects that episode in its existing series when
the series is present; closing detail restores the physical file opener.

Series episodes sort numerically by known season and episode, then by opaque ID;
unknown values sort last. If known seasons coexist with unknown ones, use a
separate Without season group, never repeat unknown episodes in every season.
When no season is known, keep the direct episode list. Existing IDs, watched
state, resume and actions are retained. Do not invent season numbers or remote
metadata. Existing local sidecar artwork and missing-artwork plates remain.

## Consequences

Both views reach the same underlying content. Folder navigation needs no network
metadata service or API key. Folders follow physical storage, while Series is a
logical catalogue across roots; their contents can overlap by design.

## Alternatives considered

- A third Folders category next to Films/Series was clarified with the owner;
  an explicit mode switch makes their different meanings clearer.
- Online enrichment is deferred, not selected by this increment.
- Reparenting base items to folders would break series membership and is avoided.

## Verification

Synthetic provider tests cover nested/empty/equal-name folders, repeated scan,
restart, unavailable roots, legacy offline migration and retained progress.
C++ tests cover optional paged/stale/malformed hierarchy, identity separation and
numeric/unknown seasons. QML tests cover modes, traversal, empty/unavailable
folders, Back/focus and season filtering. The PR records check results and
independent review; real HTPC tests are not part of this change.
Browser smoke checks exercise the working HTML prototype's view modes, nested
directories, episode/detail return focus, empty/unavailable folders and season
groups in Russian and English.
