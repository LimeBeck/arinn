# ADR-0001: Arinn as an orchestrator for a shared household HTPC

- **Status:** Accepted
- **Date:** 2026-09-27 (proposed); accepted 2026-09-27 by [ADR-0028](0028-documentation-governance.md)
- **Deciders:** project owner (Developer Specification v0.3)
- **Amends / Supersedes:** —
- **Amended by / Superseded by:** [ADR-0023](0023-mvp-product-clarifications.md), [ADR-0024](0024-local-library-mpv.md), [ADR-0025](0025-downloads-mvp.md) — MVP scope now includes the local library, mpv and Transmission
- **Specification:** [§2–4, §16](../specifications/arinn-specification.md)

## Context

The main scenario is a "Balanced HTPC": browser and online video, local and network media, Steam, ES-DE and ordinary Linux applications. The browser takes about 70% of usage time. The device is shared by two people.

## Decision

Arinn provides a TV UX, Activity orchestration, capability aggregation and a plugin platform on top of an existing Linux system. It reuses the existing browser, media player, Steam and ES-DE instead of replacing them.

Version 1 uses one shared household space: no login screen, no profile selection, no per-user favorites, history or settings. Browser integration is part of the MVP. Steam and EmulationStation get their own entry points using the generic ApplicationProvider. The minimum plugin set is demo, desktop, chrome and mpris.

The first official target is Arch Linux / EndeavourOS with KDE Plasma 6 on Wayland. The core does not depend on pacman. Downloads stays a UI route; a DownloadProvider needs its own API review and is not added to v1 in advance.

Out of scope for v1: own browser or media engines, a desktop environment, a plugin marketplace, a remote plugin repository, arbitrary QML plugins, automation and recommendation engines, multi-user accounts, cloud sync, Windows and macOS, own implementations of a Steam library or Jellyfin.

## Consequences

The MVP covers the daily browser workflow and game launching without waiting for rich library integrations. No internal accounts keeps the UX simple; external applications keep their own user models.

## Alternatives considered

Own engines and a full desktop environment were excluded by the source specification. Rich Steam, ES-DE, Jellyfin, download and TV/webOS providers are deferred to later phases.

## Verification

AC-03, AC-07, AC-08, AC-13: no mandatory profile selection; browser, Steam and ES-DE entry points exist.
