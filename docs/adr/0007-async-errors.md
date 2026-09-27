# ADR-0007: Asynchronous commands and normalized errors

- **Status:** Accepted
- **Date:** 2026-09-27 (proposed); accepted 2026-09-27 by [ADR-0028](0028-documentation-governance.md)
- **Deciders:** project owner (Developer Specification v0.3)
- **Amends / Supersedes:** —
- **Amended by / Superseded by:** [ADR-0020](0020-dbus-contract-baseline.md) (timeouts, request IDs, cancellation, deduplication, late replies, `Cancelled` error)
- **Specification:** [§6.5, §9.5, §9.6](../specifications/arinn-specification.md)

## Context

External applications may respond slowly or be unavailable. Their latency must not block the UI, and provider stack traces must not reach the QML API.

## Decision

Operations that may touch an external application — including launch, open, activate and play — are asynchronous. Each has a timeout, a typed error and a request ID; cancellation is supported where it makes sense.

The core normalizes errors to NotFound, Unavailable, Unsupported, PermissionDenied, Timeout, InvalidArgument, Conflict and Internal.

A slow provider ends in a timeout/error state without blocking the UI thread. Data loads asynchronously; focus and navigation are local to the shell and never wait for IPC.

## Consequences

The UI stays responsive when integrations misbehave. Pending and error states appear, and the moment an external operation completes must be defined explicitly.

## Alternatives considered

The synchronous chain keypress → D-Bus → plugin → response → focus change is forbidden. Immediate success of external commands is never assumed.

## Verification

Simulate a hung provider and check the timeout while D-pad navigation keeps working. An unknown ID returns NotFound; no stack trace reaches the QML API.
