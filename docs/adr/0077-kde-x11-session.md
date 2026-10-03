# ADR-0077: Optional KDE Plasma X11 session

<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

- **Status:** Proposed
- **Date:** 2026-10-03
- **Deciders:** owner requested additional X11 support while preserving Wayland; implementation prepared by Codex in [#267](https://github.com/LimeBeck/arinn/issues/267), owner merge pending
- **Amends / Supersedes:** amends [ADR-0074](0074-managed-playback-window.md) (platform observation of the launch identity)
- **Amended by / Superseded by:** —
- **Specification:** §6.4, §8.5, §12, §18; R-01, R-05

## Context

The owner requested X11 as an additional mode on 2026-10-03. The Wayland MVP
remains the primary target. KDE provides the existing KWin scripting bridge on
X11 too; generic EWMH/XFCE work in #186 requires a separate adapter.
The packaged shell previously forced Qt Wayland, and the media adapter rejected
X11 playback windows even when they carried a per-launch identity.

## Decision

Select Qt `wayland` or `xcb` from the graphical session type in a platform-owned
shell launcher. Allow an explicit backend override; reject unknown values.
Reuse the existing authenticated KWin inventory/control/Home bridge.

A player launch sets its UUID on both the Wayland app ID and the X11 instance
name. On an actual `KWin::X11Window`, read only the instance name with the exact
lowercase prefix and canonical UUID. On `KWin::XdgToplevelWindow`, retain the
existing app-ID observation. Generic window classes, titles, process names and
focus never establish this identity. No public wire type changes.

Request that mpv never bypass the X11 compositor so that the shell's transparent
input/OSD layer can be composited above video. Keep all existing lifecycle fences,
window-object identity and saved-flag restoration.

## Consequences

The same package can start in KDE Wayland or KDE X11. An X11 compositor is required
for playback overlay transparency. Operator qualification remains pending; this
ADR and code do not close any acceptance gate. Other desktops still need #186.
The platform choice adds no plugin capability or product-specific core routing.

## Alternatives considered

Implementing generic EWMH first remains the non-KDE route in #186. Reusing KWin
provides a smaller independently reviewable additional KDE mode.
Inferring playback from a generic WM_CLASS or title was rejected because it cannot
identify the exact managed launch. Replacing Wayland was rejected by the owner.

## Verification

Launcher tests cover Wayland, X11, explicit override and invalid values.
The mpv scope test checks both identity arguments share one launch UUID.
Node adapter and real QJSEngine tests check X11 instance identity and unchanged
Wayland observation. Full repository checks and independent review precede readiness.
Real-desktop tests need explicit owner permission and remain unverified.
