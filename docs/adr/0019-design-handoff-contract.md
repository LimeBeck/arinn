# ADR-0019: The design handoff as a separate presentation contract

- **Status:** Accepted
- **Date:** 2026-09-27 (proposed); accepted 2026-09-27 by [ADR-0028](0028-documentation-governance.md)
- **Deciders:** project owner (design handoff 2026-09-27)
- **Amends / Supersedes:** —
- **Amended by / Superseded by:** [ADR-0022](0022-state-and-interaction-baseline.md) (ownership of prototype state), [ADR-0023](0023-mvp-product-clarifications.md) (Back on Home, Sleep), [ADR-0027](0027-activity-background-snapshot.md) (Activity background)
- **Specification:** [§15](../specifications/arinn-specification.md)

## Context

A standalone design package was delivered: specification, HTML prototype, screens, Design System v2, direction study and 22 screenshots. It refines Quiet Spatial OS with direction 2a "Depth with Light". Visual parameters are declared final, but some prototype integration points go beyond Plugin API v1 and the original MVP.

## Decision

The package is stored unchanged in `docs/design/handoff` and is the source of visual parameters. Presentation is implemented in Qt 6/QML (ADR-0002). The handoff itself asks to recreate the design in the target stack and not to ship the HTML; its mention of Electron/Tauri does not override the chosen architecture.

The visual contract:

- Logical canvas 1920×1080, uniform scale `min(vw/1920, vh/1080)`, safe area and sizes from the handoff.
- Onest and Material Symbols Rounded; minimum 24 for text and 26 for anything an action depends on; tabular numerals; room for longer Russian labels.
- Ember marks status, progress and brand; focus uses light, scale and a ring over artwork. Other tokens, radii, elevation and motion come from the handoff.
- The light field follows artwork hue, crossfades over 900 ms, can be disabled and is absent on system screens.
- D-pad and pointer modes, spatial navigation, topmost focus scopes `dialog > power > sheet > nav > screen`, per-screen focus memory and 500 ms hover suppression after scroll.
- Overlay, loading, empty, error, disabled, pressed and focus states are checked against the design system and screenshots. Placeholder artwork is not finished content.

The prototype's JavaScript model demonstrates behaviour only. Provider-owned library, downloads and playback are not moved into the shell as its own backend. The shell keeps interaction state and gets data and commands through the daemon.

Conflicts and new capabilities are tracked in the [integration review](../design/integration-review.md). A screen existing in the handoff does not extend the MVP or define an API by itself.

## Consequences

Developers have exact visual references and do not reinvent the design system. Dynamic APIs, compositor effects and disputed transitions must be agreed before implementation; an HTML mockup does not prove feasibility on Wayland.

## Alternatives considered

Shipping the HTML as the shell contradicts the handoff. Replacing Qt/QML with the prototype stack or moving simulated integrations into QML does not follow from receiving the design.

## Verification

Compare the implementation with Design System v2 and the 22 screenshot states at logical 1920×1080 and 4K scale. Check D-pad/pointer, scope trapping, focus return, 3-metre readability and behaviour with delayed or unavailable providers.
