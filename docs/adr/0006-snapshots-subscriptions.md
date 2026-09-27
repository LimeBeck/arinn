# ADR-0006: State through snapshots, revisions and subscriptions

- **Status:** Accepted
- **Date:** 2026-09-27 (proposed); accepted 2026-09-27 by [ADR-0028](0028-documentation-governance.md)
- **Deciders:** project owner (Developer Specification v0.3)
- **Amends / Supersedes:** —
- **Amended by / Superseded by:** [ADR-0020](0020-dbus-contract-baseline.md) (generation/revision scope, subscribe/read ordering)
- **Specification:** [§9.3, §9.4](../specifications/arinn-specification.md)

## Context

Reconnects and restarts of the shell or plugins must not require replaying a long stream of imperative events. The provider remains the source of integration state.

## Decision

State is transferred as snapshot + revision. When state changes, the provider publishes a new revision.

The API supports subscriptions: `StateChanged(providerId, revision)`, `ProviderAvailable(providerId)`, `ProviderUnavailable(providerId)`, `ActivityChanged(...)`. After `StateChanged` a consumer may fetch the snapshot; events do not carry the full state.

The daemon aggregates provider state. When the daemon is temporarily unavailable, the shell shows a degraded state and reconnects.

## Consequences

Full snapshots make resynchronization simple. Subscription/read ordering, snapshot size and stale replies need precise rules (defined in ADR-0020).

## Alternatives considered

Many mutable imperative events as the synchronization basis are rejected. A delta protocol is not part of this decision.

## Verification

Contract tests check revision growth and reconnect after restart; integration tests check disappear/reappear and shell recovery.
