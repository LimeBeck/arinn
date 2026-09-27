# ADR-0026: Home on the screen of the active Activity

- **Status:** Accepted
- **Date:** 2026-09-27
- **Deciders:** project owner (two monitors on the development machine, P-06); technical rule by delegated choice
- **Amends / Supersedes:** amends [ADR-0009](0009-kde-wayland-platform.md)
- **Amended by / Superseded by:** [ADR-0031](0031-single-screen-target.md) — the target HTPC has one screen
- **Specification:** [§12.2](../specifications/arinn-specification.md)

## Context

The development machine has two monitors with different scales. Binding Home to the primary screen breaks the Activity context.

## Decision

When Home opens, the platform remembers the Activity window and its output. The single shell window is shown fullscreen on that output; with no Activity the active KWin screen is used. Repeated Home keeps the original window/output pair. Return activates the same window without moving it. Arinn never changes the user's resolution, scale, primary screen or layout.

KWin handles foreground and placement; Qt picks the matching `QScreen` before showing the shell. The shell receives the output name from the daemon. If the Activity disappears, Home stays available and Back opens navigation.

## Consequences

The spike checks both screens separately, including fractional scale.

## Alternatives considered

Always showing Home on the primary screen was rejected.

## Verification

[Phase 0 report](../architecture/phase-0-report.md). Hot-unplug, mirrored outputs and moving the Activity while Home is open need further tests.
