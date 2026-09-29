# ADR-0024: Local library and external mpv with a rich media API

- **Status:** Accepted
- **Date:** 2026-09-27
- **Deciders:** project owner (scenario, P-02); components by delegated choice
- **Amends / Supersedes:** amends [ADR-0001](0001-product-scope.md), [ADR-0013](0013-mpris-media.md)
- **Amended by / Superseded by:** amended by [ADR-0046](0046-shell-input-layer-during-playback.md) (input path of the managed mpv session), [ADR-0052](0052-media-and-library-wire-contract.md) (wire schema)
- **Specification:** [§7.4](../specifications/arinn-specification.md)

## Context

The project owner chose a catalogue of local and mounted network folders, metadata, an external player and audio/subtitle selection.

## Decision

A LibraryProvider for folders and an mpv MediaProvider over a private JSON IPC socket; the QML OSD stays in the shell. Resume is owned by the library provider; transient playback by the media provider. Rich typed interfaces are defined by a separate [API review](../plugin-api/mvp-extensions.md). The detailed design is [media MVP](../architecture/media-mvp.md).

## Consequences

MPRIS remains the base integration for other players and does not have to implement tracks or thumbnails. An integration gate for the OSD and input over the video window is added.

## Alternatives considered

Jellyfin as a mandatory server, and MPRIS transport controls only, do not satisfy the chosen scenario. No own media engine.

## Verification

AC-15, AC-16. Wire schemas and runtime integration tests are part of development.
