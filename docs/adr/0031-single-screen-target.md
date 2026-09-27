# ADR-0031: The target HTPC has one screen

- **Status:** Accepted
- **Date:** 2026-09-27
- **Deciders:** project owner (answer P-08)
- **Amends / Supersedes:** amends [ADR-0026](0026-multiple-monitors.md)
- **Amended by / Superseded by:** —
- **Specification:** [§4, §12.2](../specifications/arinn-specification.md)

## Context

ADR-0026 was written for the two-monitor development machine. The owner confirmed that the target HTPC has a single screen.

## Decision

The primary acceptance scenario is one screen, with no output selection by the user. The ADR-0026 rule stays: with one screen it selects that screen automatically. Two-monitor behaviour is extra robustness of the implementation, verified on the development machine, not a product requirement.

## Consequences

No output selection UI is needed. Multi-monitor edge cases (unplug, mirroring) are not MVP acceptance items.

## Alternatives considered

None evaluated.

## Verification

Single-screen smoke test on the target HTPC at deployment.
