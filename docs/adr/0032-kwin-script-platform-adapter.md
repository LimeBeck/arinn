# ADR-0032: A packaged KWin script as the first KDE platform adapter

- **Status:** Accepted
- **Date:** 2026-09-27 (recorded retroactively; the path was chosen for Phase 0)
- **Deciders:** delegated technical choice, confirmed by the Phase 0 spike
- **Amends / Supersedes:** amends [ADR-0009](0009-kde-wayland-platform.md)
- **Amended by / Superseded by:** [ADR-0042](0042-opt-in-session-autostart.md) — loading the packaged script per session
- **Specification:** [§12.1](../specifications/arinn-specification.md)

## Context

ADR-0009 required a platform mechanism for global Home, overlay and focus return on KDE Plasma 6/Wayland but did not choose one. The choice was documented only in the Phase 0 gate plan.

## Decision

`KdePlatformService` starts with a packaged KWin script: global shortcut through KWin, foreground window tracking, shell activation and restoring the previous window. A typed bridge between the daemon and the adapter accepts only predefined operations; plugins can never submit scripts. Product-specific window logic stays in providers; compositor operations stay in the adapter. After a daemon crash the adapter tracks the D-Bus owner, retries its long-poll, drops stale replies and republishes the saved Activity context.

## Consequences

No compositor plugin needs to be compiled. The adapter depends on the KWin scripting API of the running Plasma version.

## Alternatives considered

Not evaluated in depth: a layer-shell overlay, a KWin effect only, and activation tokens from the shell alone.

## Verification

[Phase 0 report](../architecture/phase-0-report.md): 20/20 Home/Return cycles for Chrome, Steam and a fullscreen game on both outputs, daemon crash recovery.
