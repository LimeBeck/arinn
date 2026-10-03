# ADR-0080: MVP platforms — KDE Plasma 6 on Wayland and X11/EWMH desktops

<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

- **Status:** Accepted
- **Date:** 2026-10-03
- **Deciders:** owner decision on 2026-10-03 ([#297](https://github.com/LimeBeck/arinn/issues/297))
- **Amends / Supersedes:** amends [ADR-0078](0078-x11-ewmh-adapter.md) (the X11/EWMH adapter becomes an MVP platform, not only an option)
- **Amended by / Superseded by:** —
- **Specification:** header, §4.2, §4.4 (P-09), §12, §18; AC-01–AC-19

## Context

The specification named KDE Plasma 6 on Wayland as the only target. The
X11/EWMH adapter ([ADR-0078](0078-x11-ewmh-adapter.md)) and the KDE X11
session ([ADR-0077](0077-kde-x11-session.md)) were optional additions that
changed no acceptance criteria. The owner wants Arinn to run on any Linux
desktop environment, not only KDE. A second reason is testing: the owner wants
AI agents to run and test the real shell in their own virtual machines, and an
XFCE session on X11 is the desktop such a VM can run without a GPU or a
Wayland compositor.

Window control differs per desktop. KDE uses a KWin script; X11 window managers
follow EWMH; GNOME on Wayland exposes no foreign-window control without a shell
extension; wlroots compositors need their own protocols. Each platform therefore
needs its own adapter behind `PlatformService` (R-10).

## Decision

The MVP is accepted on two platforms:

1. **KDE Plasma 6 on Wayland**, through the KWin platform adapter.
2. **An X11 session with an EWMH window manager**, through the X11/EWMH adapter.
   XFCE is the first qualified desktop on this platform.

Every acceptance criterion AC-01–AC-19 holds on both platforms. An acceptance
criterion or gate is passed only when its evidence covers both, unless the
roadmap marks it platform-independent (for example a review or a private-bus
test).

Support for any other desktop is a design principle: core, shell, protocol and
plugins MUST NOT depend on a particular desktop environment, and desktop-specific
code stays in a platform adapter (R-10). GNOME and wlroots Wayland adapters are
post-MVP.

## Consequences

- The X11/EWMH adapter is a supported MVP platform. Its pending items in
  [x11-ewmh.md](../architecture/x11-ewmh.md) (output placement, background
  capture, session lifecycle, real XFCE/TV qualification) become MVP work.
- Text that says "Plasma handles media keys" (OQ-04) means "the desktop
  environment handles them" on other platforms.
- Packaging must start Arinn on both platforms (AC-01): KDE autostart and the
  EWMH session target.
- An XFCE session in an agent's own virtual machine is a test environment for
  the real shell, daemon and adapter, so agents can verify UI behaviour end to
  end instead of only with offscreen fixtures. Such a VM is not the owner's
  desktop; the rules for agent testing there are owner-gated and belong in
  `AGENTS.md`/`CONTRIBUTING.md`, which the owner updates separately.
- Operator acceptance on the owner's real desktop and TV still runs only with
  the owner's permission.

## Alternatives considered

- **MVP on KDE only, other desktops later:** faster to the MVP, but the owner
  wants a desktop-independent product from the start.
- **MVP on every desktop, including GNOME and wlroots Wayland:** needs a GNOME
  Shell extension and wlroots adapters before the MVP; it would delay the MVP
  considerably.

## Verification

The Phase 5 gate G-5.5 in the [roadmap](../../ROADMAP.md) requires the operator
acceptance of AC-01–AC-19 on an X11/EWMH desktop (XFCE) in addition to KDE Plasma
6 on Wayland. Accepting this ADR passes no criterion.
