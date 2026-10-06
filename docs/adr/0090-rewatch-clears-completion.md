# ADR-0090: Launching a completed item is a rewatch

- **Status:** Accepted
- **Date:** 2026-10-06
- **Deciders:** delegated technical choice for a bug fix ([#373](https://github.com/LimeBeck/arinn/issues/373)), within [ADR-0024](0024-local-library-mpv.md)
- **Amends / Supersedes:** amends [ADR-0056](0056-local-library-scan-metadata-and-resume.md) (*Resume*)
- **Amended by / Superseded by:** —
- **Specification:** [§7.4](../specifications/arinn-specification.md#74-local-library-and-mpv), AC-16

## Context

ADR-0056 starts a completed item at 0 on Launch but keeps it completed until
`RestartFromBeginning`. `SetProgress` never clears completion, and the shell
has no control that calls `RestartFromBeginning`. On the owner's HTPC an
episode once seeked to its end stayed completed: every later Continue started
it from 0, wherever the viewer had stopped.

## Decision

`Launch` of a completed item is a rewatch. The library provider clears
completion and resets the position to 0, which is the effect of
`RestartFromBeginning`, as part of answering `Launch`. It then publishes the
changed resume. Later samples resume normally, and stepping into the final
window or a confirmed end completes the item again. Launch of an item that is
not completed is unchanged.

## Consequences

- A rewatch can be resumed from where the viewer stopped.
- While an item is being rewatched it is not marked completed. A separate
  "watched at least once" marker would be a product decision and is not part
  of this change.
- `RestartFromBeginning` keeps its meaning for an explicit restart of an
  unfinished item.

## Alternatives considered

- **Clear completion on the first forward progress sample.** Rejected: it
  depends on sample timing and would leave a window where Continue still
  starts at 0.
- **A shell "Watch again" control.** Rejected for this fix: it needs new UI,
  and Continue would still restart from 0 without it.

## Verification

- `plugins/library/tests/catalogue.rs`
  `rewatching_a_completed_item_resumes_where_it_stopped`: launch of a
  completed item, a mid-episode stop resumes, and the end completes again.
- `plugins/library/tests/bus.rs`: a Launch over the bus clears completion in
  the published resumes; this fails without the change.
