# ADR-0021: Transient systemd services and a separate Chrome native bridge

- **Status:** Accepted
- **Date:** 2026-09-27
- **Deciders:** delegated technical choice
- **Amends / Supersedes:** amends [ADR-0003](0003-external-plugin-processes.md), [ADR-0008](0008-discovery-lifecycle.md), [ADR-0011](0011-chrome-integration.md), [ADR-0012](0012-desktop-steam-esde.md), [ADR-0013](0013-mpris-media.md), [ADR-0015](0015-security-boundary.md)
- **Amended by / Superseded by:** —
- **Specification:** [§7.1, §9.9, §9.10](../specifications/arinn-specification.md)

## Context

Chrome starts its Native Messaging host itself, while a regular plugin must be supervised by systemd. Discovery and restart need deterministic rules.

## Decision

Manifests are TOML with XDG precedence and explicit reload. Providers run as transient systemd user services with bounded restart. Runtime status and backend status are separate. The Chrome native host is a browser adapter process that connects to the systemd-supervised Chrome provider instead of competing with it for lifecycle.

The detailed contract is the [plugin runtime](../architecture/plugin-runtime.md) document, which is part of this decision.

## Consequences

Installing a plugin needs no hand-written unit per integration. A crash or reload must not close user applications; cgroup separation is checked by integration tests.

## Alternatives considered

The daemon does not become a second supervisor, and the same Native Messaging host is never started by both Chrome and systemd.

## Verification

systemd and Chrome runtime scenarios in integration tests.
