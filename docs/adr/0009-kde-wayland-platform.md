# ADR-0009: PlatformService and a mandatory KDE/Wayland spike

- **Status:** Accepted
- **Date:** 2026-09-27 (proposed); accepted 2026-09-27 by [ADR-0028](0028-documentation-governance.md)
- **Deciders:** project owner (Developer Specification v0.3)
- **Amends / Supersedes:** —
- **Amended by / Superseded by:** [ADR-0026](0026-multiple-monitors.md) (output selection), [ADR-0027](0027-activity-background-snapshot.md) (Activity background), [ADR-0032](0032-kwin-script-platform-adapter.md) (KWin script adapter)
- **Specification:** [§8.5, §12](../specifications/arinn-specification.md)

## Context

Wayland restricts control of foreign windows and focus stealing; global shortcuts and overlays depend on the compositor. The core UX is impossible without a reliable Home over an external Activity.

## Decision

Platform-specific code lives behind `PlatformService`; the first implementation is `KdePlatformService` for KDE Plasma 6 on Wayland. The boundary covers global shortcuts, application activation, overlay/window integration, session state and system actions. Plugins do not share control of foreign windows among themselves.

The platform layer registers a Home shortcut that works while Chrome, Steam, a game or ES-DE has focus. The shell never relies on receiving ordinary key events while unfocused.

Before significant UI work, an architecture gate is run: global Home, Arinn over Chrome, focus returned to Chrome, and the same cycle for Steam and a game. Phase 0 also checks Qt/QML, the Rust daemon, D-Bus and an external plugin process.

A system overlay provides Home, Back, Play/Pause, volume, mute, the power menu and Settings. Actions are executed by the platform layer or the matching provider. CEC is optional; future TV/webOS integrations are plugins.

## Consequences

Compositor integration is validated early. A failed spike forces a mechanism change before UI work. A second platform implementation is possible without spreading KDE code across the daemon.

## Alternatives considered

The source specification did not choose KWin APIs, scripts, protocols or layer-shell; ADR-0032 records the chosen path.

## Verification

Record the environment, mechanism and Phase 0 results. AC-05/AC-06 plus the same cycle for Steam and a game. The check counts only after a real integration spike ([Phase 0 report](../architecture/phase-0-report.md)).
