# ADR-0035: Phase 1 application wire contract

- **Status:** Accepted
- **Date:** 2026-09-28
- **Deciders:** Delegated technical choice within the owner-authorized Phase 1 implementation
- **Amends / Supersedes:** ADR-0020 (concrete application wire bindings)
- **Amended by / Superseded by:** —
- **Specification:** §9, §17

## Context

[ADR-0020](0020-dbus-contract-baseline.md) selects typed, versioned D-Bus contracts
and requires the XML to be fixed when each capability is implemented. Phase 1
replaces the experimental singleton provider with a dynamic registry.

## Decision

Use the four versioned interfaces documented in the [Plugin API](../plugin-api/README.md).
The daemon owns public application proxies and operation objects. Provider command
methods are asynchronous D-Bus calls whose eventual replies complete those public
operations; providers do not duplicate the operation registry. Public clients retain
the existing request UUID, deadline and deduplication semantics.

Read installed applications and running instances in one coherent paged snapshot.
Page the provider registry separately, with immutable caller-bound snapshot tokens.
Keep the minimal platform/status/Home projection on the daemon while the full shell
is developed in later phases. All shell capability calls address daemon-owned paths.
The application model contains no arbitrary metadata or plugin-supplied UI.

## Consequences

The SDK and Qt bindings share explicit field order and signatures. The private-bus
contract suite checks actual introspection against committed XML. API major 1 remains
pre-stable; other capabilities require their own reviewed model and fixtures.
Operations cannot yet be cancelled, so every operation declares `cancellable=false`.

## Alternatives considered

A second provider-owned operation tree duplicates lifetime and deduplication state.
Separate installed/running reads make cross-list consistency harder for clients.

## Verification

G-1.1–G-1.5: typed contract client, live XML comparison, private-bus recovery/limits
and Qt decoding tests. AC-11: dynamic manifest reload. AC-12: daemon-only shell calls.
See the [implementation record](../architecture/phase-1-progress.md) for executed
checks and the outstanding real-session, CI and release verification.
