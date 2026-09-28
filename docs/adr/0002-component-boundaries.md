# ADR-0002: Qt/QML shell, Rust daemon and a public D-Bus API

- **Status:** Accepted
- **Date:** 2026-09-27 (proposed); accepted 2026-09-27 by [ADR-0028](0028-documentation-governance.md)
- **Deciders:** project owner (Developer Specification v0.3)
- **Amends / Supersedes:** —
- **Amended by / Superseded by:** [ADR-0020](0020-dbus-contract-baseline.md) (bindings, bus names, API defaults), [ADR-0021](0021-plugin-runtime-ownership.md) (units and autostart), [ADR-0041](0041-independent-session-lifecycle-guard.md) (independent session observation)
- **Specification:** [§8, §14.3, §18](../specifications/arinn-specification.md)

## Context

Presentation, system orchestration and external integrations must evolve independently. Product-specific logic in QML or in the daemon defeats extension through providers.

## Decision

`arinn-shell` uses Qt 6/QML. Rust is the language of `arinnd`. The shell owns rendering, navigation, focus, animations, overlays, screens, the design system and interaction state. All integration commands and data go through `arinnd` over D-Bus.

The daemon holds the plugin and capability registries, lifecycle orchestration, state aggregation, command routing, subscriptions, snapshots and revisions, error normalization, the platform abstraction, configuration and a small amount of own persistent state. Anything that can live in a provider lives in a provider.

`arinnctl` uses the same public daemon API as the shell. The shell never launches applications itself, never reads `.desktop` files, never talks to MPRIS and never runs shell commands.

The separation shell / core / protocol / plugins / platform is kept. Directory names may change without breaking that separation. Packaging ships `arinnd`, `arinn-shell`, `arinnctl` and `arinn-plugin-*`, and installs the Chrome Native Messaging manifest.

## Consequences

UI and CLI share one source of state and errors. IPC becomes a mandatory boundary that needs its own contract.

## Alternatives considered

Direct control of third-party applications from the shell is excluded.

## Verification

Phase 0 checks Qt/QML, Rust and D-Bus. Phase 1 checks that the shell receives all data through the daemon. AC-01, AC-12 and R-03 are checked separately.
