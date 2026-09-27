# ADR-0012: Desktop provider and dedicated Steam/ES-DE entry points

- **Status:** Accepted
- **Date:** 2026-09-27 (proposed); accepted 2026-09-27 by [ADR-0028](0028-documentation-governance.md)
- **Deciders:** project owner (Developer Specification v0.3)
- **Amends / Supersedes:** —
- **Amended by / Superseded by:** [ADR-0021](0021-plugin-runtime-ownership.md) — discovery, launch scopes, instances and graceful terminate ([plugin runtime](../architecture/plugin-runtime.md))
- **Specification:** [§7.2](../specifications/arinn-specification.md)

## Context

Ordinary Linux applications must be usable without changes to Arinn. Launching Steam and ES-DE is enough for the MVP; own game libraries are not needed.

## Decision

`arinn-plugin-desktop` implements ApplicationProvider: discover/list/get, launch, activate, terminate and running state. `.desktop` entries are the source; the core never scans them.

Steam and EmulationStation have dedicated Home tiles and launch through the generic ApplicationProvider. Game libraries, artwork, recent games, continue playing and metadata wait for dedicated providers.

Window activation goes through the platform boundary (ADR-0009); an `activate` operation in a capability does not allow plugins to manage Wayland windows themselves.

## Consequences

A working game entry point exists before rich integrations. Application identity must be linked to running instances and platform activation without product-specific core logic.

## Alternatives considered

An own Steam library client is not needed for the MVP. Scanning `.desktop` files in the daemon and launching directly from QML are excluded.

## Verification

AC-07, AC-08. Phase 2: Applications → Chrome/Steam/ES-DE → Home overlay → Back → the same application; the core contains no specific integrations.
