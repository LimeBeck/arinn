# ADR-0074: Explicit managed playback window identity

- **Status:** Accepted
- **Date:** 2026-10-01
- **Deciders:** Delegated technical choice in owner-directed playback task #189
- **Amends / Supersedes:** —
- **Amended by / Superseded by:** —
- **Specification:** §6.4, §9, §12; AC-16, G-4.1, R-01, R-03, R-05, R-09, R-10

## Context

The player OSD is bound in #197, but ADR-0046 needs an exact player window before
the platform can put a transparent shell above it. Media1 has no window identity.
Return association ADR-0073 concerns Home and cannot identify a media session.

## Decision

Use a dedicated random player-process launch identity carried as an exact Wayland
application ID. The registered provider exposes it through optional
MediaWindowProvider1; the trusted adapter reports exact object/token observations
through KWinMedia1. The daemon joins current provider and inventory lifetimes,
exposes optional MediaWindow1 and dispatches acknowledged presentation controls.
[Managed playback](../architecture/managed-playback.md) defines the normative
wire, bounds, invalidation and Home/Return/teardown sequence.

## Consequences

Legacy media sessions keep their existing wire layout and behaviour. A missing
or ambiguous association fails closed. Provider and platform responsibilities
stay separate, with no product-name routing or direct shell/player calls.
Contract, producer and platform/shell integration remain separate tasks.

## Alternatives considered

- Resource class, title, only player window or current focus: cannot prove session
  ownership and can place input over an unrelated application.
- Letting a provider assert arbitrary platform tokens: conflates provider and
  compositor identity; the dedicated launch identity is independently observed.
- Required fields in Session or Window: unnecessary incompatible wire changes.
- Reusing the Home Return association: it identifies Home context, not playback.

## Verification

Protocol validation and private-bus introspection/lifecycle fixtures; production
routing, adapter tests and shell binding tests follow in separately scoped tasks.
Real KDE/TV tests are owner-run. No AC-16 or G-4.1 qualification is claimed.
