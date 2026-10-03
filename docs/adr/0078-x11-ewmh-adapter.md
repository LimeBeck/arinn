# ADR-0078: Optional generic X11/EWMH adapter

- **Status:** Proposed
- **Date:** 2026-10-03
- **Deciders:** Owner requested generic EWMH support while retaining Wayland; implementation prepared by Codex, owner review and merge pending
- **Amends / Supersedes:** ADR-0040
- **Amended by / Superseded by:** —
- **Specification:** §6.3, §6.4, §8.5, §12; AC-16, AC-18, R-01, R-05, R-10

## Context

Issue #186 describes a second platform for XFCE and other X11 desktops. The
owner explicitly requested this adapter alongside KDE/X11 (#267) on 2026-10-03.
KWin script APIs are not available on generic window managers. The existing
trusted platform transport already separates inventory and controls from core
capability routing.

## Decision

Add a separate Rust EWMH process under `platform/x11`, using x11rb and owning
`org.arinn.X11`. Select one trusted adapter name through explicit daemon process
configuration; default remains KWin. Invalid selection fails closed. Reuse the
internal scalar transport without public D-Bus, SDK or XML changes. Keep all
native window discovery, keyboard grabs, controls and video layering in the
platform layer.

Use EWMH manager validation, bounded property reads and per-lifetime opaque
tokens. Resolve only exact desktop metadata; require the exact managed video
launch UUID in the native WM_CLASS instance. Home and Return retain exact target
evidence. Require an unlocked ScreenSaver reply for presentation and controls,
and an active compositor for transparent playback. EWMH acknowledgements cover
request dispatch, not a promise that an arbitrary manager accepted focus policy.
Install an opt-in session target alongside existing KDE units.

## Consequences

Wayland and KDE remain available. The daemon authenticates exactly one selected
adapter owner, so an EWMH process cannot submit records to the default KWin mode.
The internal interface names retain KWin naming for compatibility. Explicit
systemd environment selection must precede daemon startup.

The initial adapter does not provide XComposite background capture or RandR
output placement. Real XFCE/compositor/TV and session-lifecycle qualification
remain separate work; no phase or MVP criteria change. This proposed ADR and
platform specification addition require owner review and owner-only merge.

## Alternatives considered

- Reuse the KWin script on every X11 desktop: it depends on KWin-only APIs.
- Put X11 operations in shell/core routing: violates the platform boundary R-10.
- Change all bridge wire names: unnecessary contract churn for an internal
  transport that already carries the required records.

## Verification

Unit tests cover canonical native identity, token retirement and explicit
backend selection. Native tests spawn isolated Xvfb with a synthetic EWMH
manager to verify inventory, activation/close dispatch, manager loss,
Home/Return and compositor/lock cleanup. Private-bus tests reject the unselected
adapter owner. Full repository checks and an independent agent review are
required. Operator tests require separate explicit owner permission and have
not been run.

Review clarification: retain exact desktop-file IDs including their suffix; probe
the canonical, legacy and XFCE lock endpoints in priority order in both adapter
and shell. A functioning lock API remains mandatory. The selected bus-name
owner is trusted without unit attestation. Arch CI runs isolated Xvfb scenarios;
manual XFCE setup supplies session type and the EWMH target pulls graphical
session activation as a dependency (the graphical target refuses direct start).
