# ADR-0005: A typed, versioned Plugin API

- **Status:** Accepted
- **Date:** 2026-09-27 (proposed); accepted 2026-09-27 by [ADR-0028](0028-documentation-governance.md)
- **Deciders:** project owner (Developer Specification v0.3)
- **Amends / Supersedes:** —
- **Amended by / Superseded by:** [ADR-0020](0020-dbus-contract-baseline.md) — selects session D-Bus for the daemon ↔ plugin transport and fixes limits
- **Specification:** [§9.7, §9.8](../specifications/arinn-specification.md)

## Context

External processes of different versions and languages need an explicit contract. Extensible metadata must not destroy the typing of core fields.

## Decision

The Plugin API version is 1. The manifest declares `api_version`; an incompatible plugin never becomes a working provider, and the reason is shown in diagnostics.

Core fields and operations stay typed. Optional metadata has an extension mechanism; on D-Bus an `a{sv}` map is acceptable, but the API as a whole never becomes an untyped dictionary.

D-Bus is fixed for shell ↔ daemon. The daemon ↔ plugin transport was left open by the source specification and is chosen separately before the wire contract is frozen (resolved by ADR-0020).

## Consequences

Versions and types allow compatibility checks before a capability is exposed. Until the wire contract exists, compatibility of implementations written from method lists alone cannot be promised.

## Alternatives considered

An untyped dictionary as the whole API is excluded.

## Verification

An incompatible version is refused with a diagnostic; typed fields are validated; optional metadata works once the contract is defined.
