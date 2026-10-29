# ADR-0117: Prefer native browser media over a verified Plasma proxy

- **Status:** Accepted
- **Date:** 2026-10-29
- **Deciders:** Owner, explicit source-precedence choice in #581
- **Amends / Supersedes:** ADR-0096 (source selection before association)
- **Amended by / Superseded by:** —
- **Specification:** §5.1, §7.1; AC-09, AC-16; R-01–R-05

## Context

One browser can export a native MPRIS endpoint while Plasma Browser Integration
exports another. Continue can then show the tab and both sessions. In read-only
TV inspection for [#581](https://github.com/LimeBeck/arinn/issues/581), the
proxy's OS parent owned the native endpoint, and its `kde:pid` metadata agreed
with that parent. No titles, URLs or process IDs are retained as evidence.

Plasma publishes its browser parent as `kde:pid` but uses a fixed track ID in
[its MPRIS implementation](https://github.com/KDE/plasma-browser-integration/blob/master/host/mprisplugin.cpp).
This establishes process provenance, not identical content: two sources can
select different tabs. The owner explicitly chose native media as primary and
Plasma as fallback for one verified browser process.

## Decision

The MPRIS provider omits a Plasma Browser Integration proxy from its published
sessions while exactly one available non-proxy endpoint is owned by the
proxy's verified browser parent. Recognize the proxy's documented well-known
name family, without deriving process IDs from its name. Read the bus owner's
process from D-Bus, require its direct OS parent to equal the positive
`kde:pid` claim, and require the candidate endpoint's bus-owner process to equal
that parent. Pin metadata reads to unique bus owners and discard observations
if the well-known owner changes during the read.

No native candidate, several candidates, a failed read or unverified provenance
keeps the proxy independent. When the native source disappears, restore the
proxy automatically. Track it while omitted; do not stop it or alter desktop
media-key handling. The preference is independent of titles, URLs and whether
playback is paused. Product-specific recognition stays in the provider.

This is source precedence, not a claim of identical media. The primary source
determines the media controls when the sources select different tabs. The
existing ADR-0096 join still requires one audible tab to establish a new link
and preserves an established link through pause. A daemon restart during
silence therefore cannot invent a tab association.

mpv exports the existing optional `MediaProcess1` interface. It reports the
current Unix IPC peer's kernel-provided process identity, or 0 when no peer is
connected. A retained stopped session is then a supported source with no
process rather than an unsupported provider that vetoes all associations.
Only the registered daemon owner can query it; generation/session checks and
owner revalidation match the existing contract. IPC peer changes invalidate
the provider snapshot even when visible session fields do not change.

No protocol/SDK/XML signatures, daemon ambiguity rules or shell matching rules
change.

## Consequences

A verified browser normally contributes one MPRIS source instead of two.
Fallback may briefly show both cards during uncertain reads; uncertainty
never silently hides a source. Different native endpoints remain distinct,
including same-title players. Process evidence stays in provider/daemon memory
and is never published to the shell, persisted or logged.

The owner reviews and merges this product-policy change. Acceptance of this
ADR does not qualify the real desktop or close a phase gate.

## Alternatives considered

- Match titles, URLs, duration or position: they do not establish identity.
- Treat a common browser process as proof of identical content: false with
  multiple tabs; explicit precedence makes the limitation visible instead.
- Disable Plasma integration globally: loses its fallback and unrelated desktop
  features.
- Weaken the daemon's unsupported-provider check: unnecessary when mpv can
  implement the existing optional interface.

## Verification

Private-bus tests use a real proxy child with a native endpoint in its parent:
preference, automatic fallback, reappearance, multiple-candidate ambiguity and
false origin claims. mpv tests cover peer identity, disconnected/stopped state,
authorization, generation/session checks and reconnect invalidation. Existing
same-title, owner-change and browser-association tests remain required.
