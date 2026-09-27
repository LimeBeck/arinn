# ADR-0025: DownloadProvider and Transmission in the MVP

- **Status:** Accepted
- **Date:** 2026-09-27
- **Deciders:** project owner (Transmission in MVP, P-03); API by delegated choice
- **Amends / Supersedes:** amends [ADR-0001](0001-product-scope.md), [ADR-0004](0004-capability-contracts.md)
- **Amended by / Superseded by:** —
- **Specification:** [§7.5, §9.1](../specifications/arinn-specification.md)

## Context

The developer specification v0.3 deferred Downloads. The project owner moved Transmission into the MVP; R-10 requires a generalized API extension.

## Decision

A generic sixth capability, DownloadProvider, with typed job state and actions; a separate Transmission adapter; and `LibraryImport1` for completed media. The [API review](../plugin-api/mvp-extensions.md) fixes operations, limits and import idempotency. [Transmission MVP](../architecture/transmission-mvp.md) describes RPC, reconnect and the link to the library.

## Consequences

There are six base capabilities. Downloads becomes a working MVP section; "completed" and "imported" are separate states; torrent terminology stays inside the plugin. The core never talks to Transmission directly.

## Alternatives considered

A reserved Downloads route does not satisfy the owner's answer. Raw Transmission RPC in the core and a workaround through arbitrary Actions are rejected.

## Verification

AC-17: queue, actions, error/reconnect, completion and import without duplicates in Library/New.
