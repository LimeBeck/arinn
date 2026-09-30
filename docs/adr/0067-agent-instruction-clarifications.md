# ADR-0067: Agent task scope, verification and coordination

- **Status:** Proposed
- **Date:** 2026-09-30
- **Deciders:** project owner; instruction corrections requested on 2026-09-30, process details pending review of [#187](https://github.com/LimeBeck/arinn/issues/187)
- **Amends / Supersedes:** amends [ADR-0037](0037-agent-autonomy-and-owner-gated-decisions.md) (scope of autonomous task selection and handling dependencies); clarifies [ADR-0028](0028-documentation-governance.md) and [ADR-0036](0036-parallel-development.md)
- **Amended by / Superseded by:** —
- **Specification:** [§1.2](../specifications/arinn-specification.md#12-changing-this-document)

## Context

The repository review found that agent instructions required roadmap task selection
even for directed requests, required inherited CI fixes without reconciling task
zones, and duplicated a check list that had drifted from CONTRIBUTING.md and CI.
The documentation rule also failed to distinguish fixing implementation from
changing a product requirement. The owner requested instruction corrections.

## Decision

- A review produces findings; a directed change completes its requested task.
  Agents select unrelated roadmap work only when the user authorizes that mode.
  Explicit local-only requests do not publish issues or pull requests. If GitHub
  is unavailable, agents may prepare independent local changes but report missing
  coordination before publishing or merging.
- Necessary changes outside the issue zone, including inherited CI fixes, require
  an overlap check and a recorded zone expansion before editing. Active claims,
  contract boundaries and owner-gated decisions require separate coordination.
  Unresolved required CI remains a merge blocker.
- Fixes restoring an existing requirement need a regression test, a changelog
  fragment and relevant technical documentation or evidence. An accurate
  specification is referenced rather than rewritten. Intended product changes
  and corrections in specification product-behaviour sections receive owner review.
- CONTRIBUTING.md owns the canonical check commands and diff-based scope.
  Drafts may record outstanding checks; acceptance and merging require the
  applicable checks to pass. Failed full runs and isolated reruns are recorded
  separately. Operator permission and owner-only merging remain required.
- SPDX instructions describe the existing license policy and REUSE coverage;
  this decision changes no license. Synthetic fixtures and public documentation
  links are distinct from real user data; runtime logging keeps specification §13.
- ADR numbers are recorded in issues after checking existing ADRs and open
  reservations, then rechecked before writing and publishing. Registered ADRs
  keep their numbers. Competing unpublished reservations use issue-number order:
  the lower issue number keeps the reservation, the other task chooses a new
  number and updates draft links. This resolves collisions without claiming an
  atomic locking service exists.

## Consequences

Directed requests have a clear stopping point. CI failures do not justify editing
another task's zone. Documentation-only work has an explicit check scope, and
license instructions match the existing annotations. Owner-gated changes remain
prepared for the owner's review and merge; this ADR stays Proposed until accepted.

## Alternatives considered

- Keep duplicate command lists: rejected because they already diverged.
- Treat a successful rerun as a clean full run: rejected because it hides failures.
- Build an atomic ADR reservation service in this change: deferred; this task
  corrects instructions and introduces no tooling or repository-setting changes.

## Verification

Review AGENTS.md, CONTRIBUTING.md and the issue/PR templates together against
this decision. Run documentation, ADR, architecture, version and licensing checks
and Python CI tests. Because the issue template is YAML, follow the full check
scope and report any inherited failures without changing unrelated code. No
operator tests are authorized by this decision. No phase gate is closed.
