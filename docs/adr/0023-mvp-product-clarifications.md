# ADR-0023: Back, system sleep and the target HTPC

- **Status:** Accepted
- **Date:** 2026-09-27
- **Deciders:** project owner (explicit answers P-01–P-05); technical realization by delegated choice
- **Amends / Supersedes:** amends [ADR-0001](0001-product-scope.md), [ADR-0010](0010-activity-home.md), [ADR-0017](0017-navigation-design-boundary.md), [ADR-0018](0018-demo-testing-gates.md), [ADR-0019](0019-design-handoff-contract.md)
- **Amended by / Superseded by:** [ADR-0030](0030-phase0-machine-and-standard-input.md) — Phase 0 on the development machine, remote as standard keyboard input
- **Specification:** [§4, §5.7, §6.3, §19](../specifications/arinn-specification.md)

## Context

Explicit answers from the project owner resolved UX conflicts between the developer specification and the design handoff and defined acceptance hardware.

## Decision

- Home/Back follows one transition table (P-01): Back on Home returns to the Activity if there is one, otherwise opens navigation.
- "Sleep" is OS suspend, separate from the ambient screen (P-04).
- The MVP includes a local folder library with an external player (P-02) and Transmission (P-03).
- Acceptance runs on a separate KDE/Wayland HTPC with the G60S Pro air mouse (P-05).

This extends v0.3 §13, §50 and §57 and the design handoff's Keys, Power and State Management sections. For v0.3 §48 it only adds the case "Back on Home without an Activity", which v0.3 left undefined. The new acceptance criteria are AC-15–AC-19.

## Consequences

The specification v0.4 reflects the table, the extended MVP and the new criteria.

## Alternatives considered

"Back always opens navigation" and "Sleep shows the screensaver" (design handoff) were rejected by the project owner.

## Verification

AC-15–AC-19. Compositor, input and media acceptance on the target HTPC is not implied by accepting this ADR.
