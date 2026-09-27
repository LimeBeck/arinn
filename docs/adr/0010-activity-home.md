# ADR-0010: Activity and one Continue/Home overlay model

- **Status:** Accepted
- **Date:** 2026-09-27 (proposed); accepted 2026-09-27 by [ADR-0028](0028-documentation-governance.md)
- **Deciders:** project owner (Developer Specification v0.3)
- **Amends / Supersedes:** —
- **Amended by / Superseded by:** [ADR-0022](0022-state-and-interaction-baseline.md) (Activity schema, ownership, deduplication), [ADR-0023](0023-mvp-product-clarifications.md) (Home/Back table)
- **Specification:** [§5.1, §5.2, §6.3, §10](../specifications/arinn-specification.md)

## Context

A process list does not describe what the user is doing: Chrome is an application, a specific tab is an activity. A running external application is a normal state of Arinn.

## Decision

An Activity is what the user is doing right now: a tab, a game, ES-DE, movie playback. An Activity is not necessarily an Application.

Top-level routes: Home, Library, Downloads, Applications, Settings. Home is both the main screen and the global overlay. Continue merges resumable content and the active Activity; there are no competing sections such as "Now running", "Continue watching" or "Recent apps".

Favorites/Online holds direct entry points: Browser, online services, Steam, EmulationStation. New shows new or recommended items from providers; Arinn has no recommendation engine.

Home appears over the Activity. It does not terminate the application, does not destroy its state and does not pause it unless the provider requires that. Closing Home returns to the same Activity.

Back is handled in order modal → overlay → nested screen → Home → active Activity. Back inside Home never terminates the active application.

## Consequences

Users resume without choosing an application again. Activity identity must be consistent across data sources. Layout and transitions come from the [design handoff](../design/handoff/README.md).

## Alternatives considered

"An external application means leaving the launcher" contradicts the specification. Independent competing resume sections are excluded.

## Verification

AC-05, AC-06, AC-07, AC-08: application state survives Home/Back, Continue is unified, Back follows the priority.
