# ADR-0017: Local D-pad navigation and a separate design specification

- **Status:** Accepted
- **Date:** 2026-09-27 (proposed); accepted 2026-09-27 by [ADR-0028](0028-documentation-governance.md)
- **Deciders:** project owner (Developer Specification v0.3)
- **Amends / Supersedes:** —
- **Amended by / Superseded by:** [ADR-0022](0022-state-and-interaction-baseline.md) (focus and keys), [ADR-0023](0023-mvp-product-clarifications.md) (Home/Back), [ADR-0030](0030-phase0-machine-and-standard-input.md) (standard keyboard input)
- **Specification:** [§6, §15](../specifications/arinn-specification.md)

## Context

The TV-first shell is used from about three metres. Focus is shell interaction state and must not depend on providers or mouse hover.

## Decision

Every primary action is reachable with the D-pad/arrows, Enter/Select, Back and Home; media buttons are supported. An air mouse is supported but never required for shell navigation.

Focus is first-class state. Every focusable component distinguishes default, focused, pressed, disabled and loading. Visible focus never relies only on a small colour change, mouse hover or a thin border. Navigation is local to the shell and never waits for backend latency.

Back priority: modal → overlay → nested screen → Home → active Activity. Arinn draws capability UI itself and loads no UI plugins.

The visual direction of the source specification is Quiet Spatial OS ("Calm OS → Cinematic Content"). The [design handoff](../design/handoff/README.md) refines it as direction 2a "Depth with Light"; the visual contract is ADR-0019.

## Consequences

UI can be built and tested against the demo provider. The design must be implemented with D-pad support and explicit focus states.

## Alternatives considered

Mouse-only UX, focus through IPC and provider-specific UI are excluded.

## Verification

AC-02, AC-14: D-pad walkthrough and focus readability on a TV from three metres. UI tests check the focus graph; provider delays do not delay navigation.
