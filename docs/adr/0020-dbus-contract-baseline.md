# ADR-0020: D-Bus transport, identifiers and Plugin API reliability

- **Status:** Accepted
- **Date:** 2026-09-27
- **Deciders:** delegated technical choice (the project owner asked the implementer to close technical decisions)
- **Amends / Supersedes:** amends [ADR-0002](0002-component-boundaries.md), [ADR-0004](0004-capability-contracts.md), [ADR-0005](0005-ipc-versioning.md), [ADR-0006](0006-snapshots-subscriptions.md), [ADR-0007](0007-async-errors.md), [ADR-0015](0015-security-boundary.md), [ADR-0016](0016-cli-diagnostics.md), [ADR-0018](0018-demo-testing-gates.md)
- **Amended by / Superseded by:** —
- **Specification:** [§8.6, §9, §11, §14, §17](../specifications/arinn-specification.md)

## Context

The skeleton needs a concrete transport, routing, revision and operation semantics.

## Decision

Arinn uses the session D-Bus on both IPC boundaries (shell/CLI ↔ daemon and daemon ↔ plugins), QtDBus in the shell and zbus in Rust. Core fields are typed. Entities are addressed by `EntityRef` with generation/revision. Commands are asynchronous operation objects. Payloads are bounded and timeouts and errors follow an explicit model (adding `Cancelled` to the error set).

The detailed rules and defaults in the [implementation baseline](../architecture/implementation-baseline.md) are part of this decision and are mandatory for the skeleton.

## Consequences

One transport means one IPC stack; versioned typed interfaces keep plugins language-independent. Full wire schemas for browser, media, library and settings are added before each provider as an implementation deliverable.

## Alternatives considered

JSON-RPC over a Unix socket would need its own framing, subscription and lifetime layer; not chosen for the general API. JSON remains required by Chrome Native Messaging.

## Verification

Contract tests check reconnect, races and timeouts. Accepting this choice does not mean the tests pass or that every capability's wire API is ready.
