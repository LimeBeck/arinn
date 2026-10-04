# ADR-0084: Revision-guarded daemon-owned favorites

- **Status:** Accepted
- **Date:** 2026-10-04
- **Deciders:** delegated technical choice, owner-directed implementation of existing pin/unpin requirements
- **Amends / Supersedes:** —
- **Amended by / Superseded by:** —
- **Specification:** [§5.1, §5.5, §9, §11](../specifications/arinn-specification.md)

## Context

[#68](https://github.com/LimeBeck/arinn/issues/68) cannot implement accepted
pin/unpin requirements because no public Favorites mutation interface exists.
The shell currently substitutes every application for saved favorites. Existing
SQLite storage and stable target rules are defined by ADR-0022 and ADR-0043.

## Decision

Add daemon-root `Favorites1` with `List`, revision-guarded complete-set `Set` and
post-commit `Changed`. Reuse the three-string EntityRef for stable applications
and the reserved daemon-owned shortcut namespace. Store only identities and order;
resolve current presentation through existing catalogs. The detailed limits,
admission, transaction and client reconciliation rules are in the
[favorites contract](../architecture/favorites.md).

## Consequences

Empty favorites remain empty, unavailable providers cannot erase saved targets,
and competing shell/CLI writers cannot silently overwrite each other. A separate
implementation task must export the daemon interface and bind the shell after
this contract merges. No new provider API or shell persistence is required.

## Alternatives considered

Shell-local settings duplicate daemon-owned state and fail shared CLI semantics.
Blind pin/unpin writes lose concurrent updates; full replacement with a revision
also provides ordering without adding another method.

## Verification

Protocol wire/validation tests accompany this contract. Daemon and shell tests
specified in the technical contract remain implementation work. No phase or
real-device gate is closed by accepting this ADR.
