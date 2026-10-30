# ADR-0120: Recently added library films and series on Home

- **Status:** Proposed
- **Date:** 2026-10-30
- **Deciders:** owner requested recently added library content in chat on 2026-10-29; implementation defaults proposed in task [#587](https://github.com/LimeBeck/arinn/issues/587)
- **Amends / Supersedes:** —
- **Amended by / Superseded by:** —
- **Specification:** §5.1, §7.4, OQ-05, R-01–R-05

## Context

The owner explicitly requested the existing Home New row for recently added
library films and series, superseding the original OQ-05 exclusion. The optional
[ADR-0118](0118-library-additions-contract.md) contract has landed separately.
File modification time, release year, playback and when the shell first runs do
not establish when a provider discovered content.

## Decision

Persist first-discovery Unix microseconds per film/episode UUID in the local
provider's SQLite database in the same transaction as the catalogue update.
Keep dates across rescans, renames, metadata/progress updates, restarts, import
retries and temporary unavailability. Seed pre-existing items with unknown dates
(zero), so an upgrade does not invent a recent library. New discoveries receive
positive dates. Retained date rows do not expose removed or unavailable items.

Project a film's own discovery and a series' newest episode discovery to one
available top-level card. Publish the latest 64 known entries atomically with
the catalogue through LibraryAdditions1. The shell validates exact catalogue
identity, ordering, bounds and references; optional failure cannot break Library.

Home shows the newest 12 across providers, descending by time then ascending
provider/local ID. Unknown dates are omitted, and an empty row is hidden. Reuse
Library's poster, title, year and progress presentation. Selecting a card opens
the existing Library detail; Play remains explicit. Back returns to its Home
opener, using the established fallback when the card disappears. Ordinary
Library and Continue/Favorites navigation remain unchanged.

## Consequences

The local provider owns persistence. The shell neither reads media files nor
infers discovery from file paths or list position. Legacy content remains usable
in Library but cannot appear in New without a known discovery date. Discovery
uses the provider machine's clock; providers remain responsible for those dates.
The projection has no recommendation engine and no calendar-age cutoff.

## Alternatives considered

- Shell first-observation dates depend on shell uptime and duplicate provider state.
- File modification dates change during copies and metadata edits.
- Dating every legacy item on upgrade would misleadingly make the whole library new.

## Verification

Provider tests cover migration, bounded/tie-sorted dates, rescans, renames,
metadata/progress changes, restarts, import idempotency, new episode promotion
and unavailable roots. Private-bus coverage verifies atomic publication and stale
revision rejection. C++ tests cover provider merging, bounds, malformed/stale
metadata, lifecycle and unchanged reloads. Bound Main QML and Home tests verify
the detail intent, explicit playback separation, Back and focus restoration.
Required checks and independent review are recorded on the PR; this ADR does not
claim real-desktop qualification or a phase gate is complete.
