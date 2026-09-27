# ADR-0015: Trusted local plugins with IPC validation

- **Status:** Accepted
- **Date:** 2026-09-27 (proposed); accepted 2026-09-27 by [ADR-0028](0028-documentation-governance.md)
- **Deciders:** project owner (Developer Specification v0.3)
- **Amends / Supersedes:** —
- **Amended by / Superseded by:** [ADR-0020](0020-dbus-contract-baseline.md) (limits, logging privacy), [ADR-0021](0021-plugin-runtime-ownership.md) (peer registration)
- **Specification:** [§13](../specifications/arinn-specification.md)

## Context

MVP plugins are locally installed trusted software. Still, bad data or commands must not break the shell or core or allow arbitrary execution through the API.

## Decision

The MVP is not a hostile-plugin sandbox. It keeps process isolation, IPC and command validation, bounded message sizes and timeouts. QML never runs arbitrary shell commands; the Plugin API allows no arbitrary code injection.

Plugins supply data, metadata, actions and capabilities, never QML, HTML or other UI. The Chrome extension requests minimal permissions.

Regular logs never contain cookies, authorization headers, passwords or browser page content.

## Consequences

The protocol has defensive boundaries even for trusted software. Process isolation limits crash propagation but does not protect against a malicious local plugin.

## Alternatives considered

Loading arbitrary UI, a command shell as API and in-process native plugins are excluded. A marketplace and remote plugin repositories are outside v1.

## Verification

Malformed and oversized IPC, invalid arguments and timeouts are rejected. Review confirms no QML shell execution, no UI injection and no forbidden logged data.
