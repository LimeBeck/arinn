# ADR-0014: Provider-owned state and minimal Arinn persistence

- **Status:** Accepted
- **Date:** 2026-09-27 (proposed); accepted 2026-09-27 by [ADR-0028](0028-documentation-governance.md)
- **Deciders:** project owner (Developer Specification v0.3)
- **Amends / Supersedes:** —
- **Amended by / Superseded by:** [ADR-0022](0022-state-and-interaction-baseline.md) — TOML config, SQLite state, retention and cache rules ([implementation baseline](../architecture/implementation-baseline.md))
- **Specification:** [§11](../specifications/arinn-specification.md)

## Context

Copying the databases of the browser, Steam or a media service creates competing sources of truth. Arinn stores only its own settings and data.

## Decision

Each provider owns its integration state: Chrome tabs, MPRIS playback, desktop applications. The core aggregates that state and does not copy foreign databases.

Arinn persists favorites, the Home layout, UI preferences, provider configuration and, if needed, recent Arinn activities. Chrome history, the Steam library database and media databases are not copied when a provider can supply them.

XDG config, data, state and cache are separate: `~/.config/arinn/`, `~/.local/share/arinn/`, `~/.local/state/arinn/`, `~/.cache/arinn/` by default. Manifests and plugin data are kept apart from config and state.

## Consequences

Stored data stays small and owned by Arinn. Recovering external state depends on the provider; a missing provider is shown as unavailable, never as current data from a duplicate database.

## Alternatives considered

Own copies of Chrome history or the Steam library are rejected as a persistence basis.

## Verification

Own settings survive a restart, provider state is re-read, XDG paths are separate. AC-13: shared household data is not split by profiles.
