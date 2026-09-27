# ADR-0003: Integrations run in external processes

- **Status:** Accepted
- **Date:** 2026-09-27 (proposed); accepted 2026-09-27 by [ADR-0028](0028-documentation-governance.md)
- **Deciders:** project owner (Developer Specification v0.3)
- **Amends / Supersedes:** —
- **Amended by / Superseded by:** [ADR-0021](0021-plugin-runtime-ownership.md) (lifecycle and failure states)
- **Specification:** [§8.4, §9.10](../specifications/arinn-specification.md)

## Context

Integrations have independent dependencies and can crash. Their updates and implementation languages must not be tied to the daemon's Rust ABI.

## Decision

Every plugin is a separate OS process. Integration code is never loaded into `arinnd` as `.so` or other native libraries. Communication uses the stable IPC boundary of the Plugin API.

A plugin crash does not bring down the shell, the daemon or other plugins. When the Chrome provider crashes, Home, Steam launching and media control keep working, and the Browser section becomes unavailable.

## Consequences

Crash isolation, independent dependencies, updates and languages. Handshake, diagnostics, reconnect and supervision become necessary. Process isolation is not a sandbox for untrusted code.

## Alternatives considered

In-process native plugins are explicitly forbidden by the source specification. A hostile-plugin sandbox is not an MVP promise.

## Verification

AC-10: kill the Chrome plugin and check the remaining components. Integration tests cover crash/restart and disappear/reappear.
