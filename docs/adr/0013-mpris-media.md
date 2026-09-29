# ADR-0013: MediaProvider based on MPRIS

- **Status:** Accepted
- **Date:** 2026-09-27 (proposed); accepted 2026-09-27 by [ADR-0028](0028-documentation-governance.md)
- **Deciders:** project owner (Developer Specification v0.3)
- **Amends / Supersedes:** —
- **Amended by / Superseded by:** [ADR-0021](0021-plugin-runtime-ownership.md) (session priority, units, position anchors), [ADR-0024](0024-local-library-mpv.md) (rich playback through mpv), [ADR-0054](0054-media-and-library-wire-contract.md) (wire schema)
- **Specification:** [§7.3](../specifications/arinn-specification.md)

## Context

Arinn needs one media UI over existing players and browsers, without an own media engine.

## Decision

On Linux the MVP discovers media sessions through `arinn-plugin-mpris`. The shell reads data and sends commands only through `arinnd` and MediaProvider.

A session has id, title, subtitle/artist, artwork, state, duration, position and capabilities (available actions). Operations: listSessions, getSession, play, pause, playPause, seek, next, previous, stop.

The global media UI shows the current session and its available actions; MPRIS specifics stay in the plugin.

## Consequences

Several external media applications are controlled from one UI. Supported actions depend on session capabilities; Activity aggregation must account for media and browser sources.

## Alternatives considered

Direct MPRIS access from the shell and an own media engine are excluded.

## Verification

AC-09. Phase 4: current media, artwork, play/pause, seek, next/previous; session disappearance; Unsupported for unavailable operations.
