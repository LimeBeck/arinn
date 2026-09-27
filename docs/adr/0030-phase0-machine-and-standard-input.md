# ADR-0030: Phase 0 on the development machine and the remote as standard keyboard input

- **Status:** Accepted
- **Date:** 2026-09-27
- **Deciders:** project owner (answers P-06, P-07)
- **Amends / Supersedes:** amends [ADR-0023](0023-mvp-product-clarifications.md), [ADR-0017](0017-navigation-design-boundary.md)
- **Amended by / Superseded by:** —
- **Specification:** [§4, §6.2](../specifications/arinn-specification.md)

## Context

ADR-0023 required acceptance on a separate HTPC with the G60S Pro air mouse. The owner then confirmed that the current machine is technically equivalent for the spike and that the remote emits standard keyboard and mouse events.

## Decision

- The technical spike (Phase 0) runs on the development machine.
- The G60S Pro maps to standard keys (arrows, Enter, Backspace, Esc, …). No dedicated remote integration, driver or SDK is needed, and it does not block Phase 0.
- Inside the shell, Esc and Backspace mean Back; arrows navigate; Enter selects. In an editable text field Backspace deletes a character; Esc closes the topmost scope.
- Outside the shell, local keys belong to the active application. Global interception of Esc or Backspace is forbidden. Global Home is a separate configurable shortcut (Meta+F12 in the spike).
- Keyboard testing is sufficient for the input gate; a physical remote smoke test is an additional usability check at deployment.

## Consequences

AC-18 is verified with standard keyboard and mouse events. Acceptance on the target HTPC stays a deployment step, not a development blocker.

## Alternatives considered

A dedicated remote protocol integration was not needed.

## Verification

AC-18; Qt tests for Backspace/Esc handling.
