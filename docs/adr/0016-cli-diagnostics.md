# ADR-0016: One API for CLI, diagnostics and structured logs

- **Status:** Accepted
- **Date:** 2026-09-27 (proposed); accepted 2026-09-27 by [ADR-0028](0028-documentation-governance.md)
- **Deciders:** project owner (Developer Specification v0.3)
- **Amends / Supersedes:** —
- **Amended by / Superseded by:** [ADR-0020](0020-dbus-contract-baseline.md) — log schema, CLI output and exit codes, diagnostics counters ([implementation baseline](../architecture/implementation-baseline.md))
- **Specification:** [§14](../specifications/arinn-specification.md)

## Context

An integration failure must be observable, not a tile that silently does nothing. Diagnostics are needed independently of the TV UI.

## Decision

`arinnctl` provides `status`, `providers`, `capabilities`, `activities` and `diagnostics` over the same public daemon API as the shell. `provider restart <id>` is desirable.

Diagnostics show the `arinnd` status, plugin status and version, API version, registered capabilities, last error and restart count. An incompatible plugin version is visible in diagnostics.

All components write structured logs with at least `component`, `provider_id`, `request_id`, `operation`, `duration` and `error`. The Linux backend is journald. Secrets and page content never appear in regular logs (ADR-0015).

An unavailable provider, a degraded daemon connection and a timeout each have an explicit UI state.

## Consequences

Operators can find the cause of a failure without knowing the internal IPC. One API keeps shell and CLI views consistent.

## Alternatives considered

Silent errors and plugin stack traces in the QML API are rejected. A separate CLI-only API is rejected.

## Verification

UI and CLI statuses match on crash, incompatible version, timeout and reconnect. Request IDs correlate across components; logs contain no forbidden data.
