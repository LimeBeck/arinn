# ADR-0018: Demo provider, shared contract suite and architecture gates

- **Status:** Accepted
- **Date:** 2026-09-27 (proposed); accepted 2026-09-27 by [ADR-0028](0028-documentation-governance.md)
- **Deciders:** project owner (Developer Specification v0.3)
- **Amends / Supersedes:** —
- **Amended by / Superseded by:** [ADR-0020](0020-dbus-contract-baseline.md) (test harness, fixtures, performance budgets), [ADR-0023](0023-mvp-product-clarifications.md) (phases and AC-15–AC-19), [ADR-0033](0033-ci-cd-and-releases.md) (CI/CD and releases)
- **Specification:** [§17, §19, ROADMAP](../specifications/arinn-specification.md)

## Context

UI work must not require Chrome, Steam or MPRIS to be present. The Plugin API and fault tolerance must be tested the same way for every provider.

## Decision

`arinn-plugin-demo` is mandatory: UI development without external applications, contract tests, bug reproduction and Plugin API documentation.

Minimum test levels:

- Unit: daemon, state aggregation, parsing, routing, plugin SDK.
- One plugin contract suite for every provider: registration, valid IDs, snapshot revision growth, NotFound for unknown IDs, reconnect after restart; checks follow the declared capabilities.
- Integration: shell/core, core/plugin, crash/restart, disappear/reappear.
- UI: D-pad navigation and focus graph.

Failure scenarios: a Chrome crash does not break Home, Steam or media; an unavailable daemon does not crash the shell, which shows a degraded state and reconnects; a slow provider ends in timeout/error without blocking the UI.

Development follows phases (see [ROADMAP](../../ROADMAP.md)); the KDE/Wayland spike gates significant UI work. MVP acceptance covers all acceptance criteria and architecture review covers rules R-01–R-10.

## Consequences

Behaviour is reproducible without live integrations; provider compatibility is checked by one suite. The demo does not replace real KDE/Wayland, browser and TV checks.

## Alternatives considered

Happy-path-only or live-application-only testing is insufficient.

## Verification

Phase exit criteria in [ROADMAP](../../ROADMAP.md). Accepting this ADR does not mean any gate has passed.
