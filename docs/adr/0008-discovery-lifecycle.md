# ADR-0008: XDG discovery and lifecycle through systemd user services

- **Status:** Accepted
- **Date:** 2026-09-27 (proposed); accepted 2026-09-27 by [ADR-0028](0028-documentation-governance.md)
- **Deciders:** project owner (Developer Specification v0.3)
- **Amends / Supersedes:** —
- **Amended by / Superseded by:** [ADR-0021](0021-plugin-runtime-ownership.md) — manifest schema, XDG precedence, transient units, explicit reload, restart policy
- **Specification:** [§9.9, §9.10](../specifications/arinn-specification.md)

## Context

A provider must be added by installing a manifest, without product changes in the core. Linux already has a process supervisor; the daemon should not duplicate it.

## Decision

Every plugin has a manifest with a stable `id`, `name`, `version`, `api_version`, `executable` and `capabilities`. System and user plugins use the same format.

Discovery looks in `$XDG_DATA_HOME/arinn/plugins` and in `arinn/plugins` under every entry of `$XDG_DATA_DIRS`.

`arinnd` orchestrates discover, start, handshake, monitor, restart and stop. After start a plugin completes a handshake and registers its capabilities. Supervision uses systemd user services or transient units.

Packaging installs binaries and the Chrome Native Messaging manifest. The architecture does not depend on pacman.

## Consequences

Installing a plugin is a self-contained operation and the lifecycle reuses Linux facilities. Discovery conflicts and the daemon/systemd responsibility split need precise rules (ADR-0021).

## Alternatives considered

Turning `arinnd` into a full process supervisor is not recommended.

## Verification

AC-11: install and remove a provider without product-specific changes. Check handshake, incompatible version, restart/reconnect and diagnostics (restart count, last error).
