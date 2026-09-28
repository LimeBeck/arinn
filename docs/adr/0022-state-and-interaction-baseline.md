# ADR-0022: Activity ownership, persistence and interaction state

- **Status:** Accepted
- **Date:** 2026-09-27
- **Deciders:** delegated technical choice
- **Amends / Supersedes:** amends [ADR-0010](0010-activity-home.md), [ADR-0013](0013-mpris-media.md), [ADR-0014](0014-state-persistence.md), [ADR-0017](0017-navigation-design-boundary.md), [ADR-0019](0019-design-handoff-contract.md)
- **Amended by / Superseded by:** [ADR-0034](0034-provider-data-directory-identity.md) — provider data-directory identity
- **Specification:** [§6, §10, §11](../specifications/arinn-specification.md)

## Context

The design prototype keeps all data locally, but the production shell must only display provider state. Most conflicts about timers, focus and ownership can be resolved technically.

## Decision

Explicit `EntityRef` relationships, provider-owned playback and library data, Activity aggregation in the core, and shell-local focus, OSD and preview state with separate operation pending states. Arinn persists a TOML config and its own SQLite state as described in the [implementation baseline](../architecture/implementation-baseline.md). This closes DR-04, DR-07, DR-08 and the technical part of DR-09/DR-10 of the [integration review](../design/integration-review.md).

The detailed contract is [Activity and interaction](../architecture/activity-and-interaction.md), which is part of this decision.

## Consequences

The core never guesses matches from titles or URLs and never copies foreign databases. Browser-first priority stays. A visual prototype timer never becomes the completion of an operation.

## Alternatives considered

Heuristic merging by name, a media database in the shell and fake success after an animation timeout are rejected.

## Verification

Contract and UI tests of Activity aggregation, focus rules and pending states.
